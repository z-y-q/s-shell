use argon2::Argon2;
use keyring::Entry;
use rand::RngCore;
use ring::aead::{Aad, LessSafeKey, Nonce as RingNonce, UnboundKey, AES_256_GCM};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Keyring,
    Master,
}

static MODE: OnceLock<Mode> = OnceLock::new();
static MASTER_KEY: OnceLock<[u8; 32]> = OnceLock::new();
static MASTER_SALT: OnceLock<Vec<u8>> = OnceLock::new();

#[cfg(test)]
static TEST_PATH: OnceLock<PathBuf> = OnceLock::new();

fn data_dir() -> PathBuf {
    dirs::data_dir().expect("无法定位数据目录").join("s-shell")
}

fn db_path() -> PathBuf {
    #[cfg(test)]
    if let Some(p) = TEST_PATH.get() {
        return p.clone();
    }
    data_dir().join("vault.bin")
}

/// 凭据库模式：Linux 无 Secret Service 时自动降级主密码模式，其余平台用系统钥匙串。
pub fn mode() -> Mode {
    *MODE.get_or_init(|| {
        #[cfg(target_os = "linux")]
        {
            if Entry::new("s-shell", "mode-probe").is_err() {
                Mode::Master
            } else {
                Mode::Keyring
            }
        }
        #[cfg(not(target_os = "linux"))]
        Mode::Keyring
    })
}

#[cfg(test)]
pub fn force_mode_for_test(m: Mode) {
    MODE.set(m).ok();
}

#[cfg(test)]
pub fn force_db_path_for_test(p: PathBuf) {
    TEST_PATH.set(p).ok();
}

pub fn master_unlocked() -> bool {
    MASTER_KEY.get().is_some()
}

fn entry(host_id: i64, field: &str) -> Result<Entry, String> {
    Entry::new("s-shell", &format!("host-{host_id}-{field}")).map_err(|e| e.to_string())
}

fn derive(password: &[u8], salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password, salt, &mut key)
        .expect("Argon2 派生失败");
    key
}

/// AES-256-GCM 加密：输出为 密文+16 字节认证标签。
fn seal(key: &[u8; 32], nonce12: &[u8; 12], plain: &[u8]) -> Result<Vec<u8>, String> {
    let unbound = UnboundKey::new(&AES_256_GCM, key).map_err(|e| e.to_string())?;
    let sealing_key = LessSafeKey::new(unbound);
    let nonce = RingNonce::try_assume_unique_for_key(nonce12)
        .map_err(|e| format!("nonce 非法：{e}"))?;
    let mut in_out = plain.to_vec();
    sealing_key
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| format!("加密失败：{e}"))?;
    Ok(in_out)
}

/// AES-256-GCM 解密：就地剥掉 16 字节认证标签，失败即口令错误或数据损坏。
fn open_in_place(key: &[u8; 32], nonce12: &[u8; 12], sealed: &mut Vec<u8>) -> Result<(), String> {
    let unbound = UnboundKey::new(&AES_256_GCM, key).map_err(|e| e.to_string())?;
    let opening_key = LessSafeKey::new(unbound);
    let nonce = RingNonce::try_assume_unique_for_key(nonce12)
        .map_err(|e| format!("nonce 非法：{e}"))?;
    opening_key
        .open_in_place(nonce, Aad::empty(), sealed)
        .map_err(|_| "解密失败（口令错误或数据损坏）".to_string())?;
    Ok(())
}

/// 首次调用设置主密码并建库；已有库时校验主密码解锁。重复解锁：口令一致则幂等成功。
pub fn master_unlock(password: &str) -> Result<(), String> {
    let path = db_path();
    if let (Some(existing_key), Some(existing_salt)) = (MASTER_KEY.get(), MASTER_SALT.get()) {
        if derive(password.as_bytes(), existing_salt) == *existing_key {
            return Ok(());
        }
        return Err("主密码错误".into());
    }
    let (salt, sealed): (Vec<u8>, Option<Vec<u8>>) = if path.exists() {
        let raw = std::fs::read(&path).map_err(|e| format!("读取主密码库失败：{e}"))?;
        if raw.len() < 29 {
            return Err("主密码库文件损坏".into());
        }
        (raw[..16].to_vec(), Some(raw[16..].to_vec()))
    } else {
        let mut s = vec![0u8; 16];
        rand::rng().fill_bytes(&mut s);
        (s, None)
    };
    let key = derive(password.as_bytes(), &salt);
    if let Some(mut sealed) = sealed {
        let (nonce_bytes, ct) = sealed.split_at_mut(12);
        let nonce_arr: [u8; 12] = nonce_bytes.try_into().unwrap();
        let mut ct_vec = ct.to_vec();
        open_in_place(&key, &nonce_arr, &mut ct_vec)
            .map_err(|_| "主密码错误，无法解锁凭据库".to_string())?;
    }
    let _ = MASTER_SALT.set(salt);
    MASTER_KEY.set(key).map_err(|_| "主密码库已处于解锁状态".to_string())?;
    Ok(())
}

fn require_master() -> Result<([u8; 32], Vec<u8>), String> {
    if !master_unlocked() {
        return Err("主密码库未解锁：请在设置里输入主密码".into());
    }
    let salt = MASTER_SALT.get().expect("已解锁必有盐").clone();
    let key = *MASTER_KEY.get().expect("已解锁必有密钥");
    Ok((key, salt))
}

fn load_master_db(key: &[u8; 32]) -> Result<BTreeMap<String, String>, String> {
    let path = db_path();
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read(&path).map_err(|e| format!("读取主密码库失败：{e}"))?;
    if raw.len() < 29 {
        return Err("主密码库文件损坏".into());
    }
    let (nonce_bytes, sealed) = raw[16..].split_at(12);
    let mut sealed = sealed.to_vec();
    open_in_place(key, nonce_bytes.try_into().unwrap(), &mut sealed)?;
    let plain_len = sealed.len() - 16;
    serde_json::from_slice(&sealed[..plain_len])
        .map_err(|e| format!("主密码库反序列化失败：{e}"))
}

fn save_master_db(key: &[u8; 32], salt: &[u8], db: &BTreeMap<String, String>) -> Result<(), String> {
    let plain = serde_json::to_vec(db).map_err(|e| e.to_string())?;
    let mut nonce12 = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce12);
    let sealed = seal(key, &nonce12, &plain)?;
    let mut raw = salt.to_vec();
    raw.extend_from_slice(&nonce12);
    raw.extend_from_slice(&sealed);
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, raw).map_err(|e| format!("写入主密码库失败：{e}"))
}

fn master_set(entry_key: &str, value: &str) -> Result<(), String> {
    let (key, salt) = require_master()?;
    let mut db = load_master_db(&key)?;
    db.insert(entry_key.into(), value.into());
    save_master_db(&key, &salt, &db)
}

fn master_get(entry_key: &str) -> Result<Option<String>, String> {
    let (key, _) = require_master()?;
    Ok(load_master_db(&key)?.get(entry_key).cloned())
}

fn master_delete(entry_key: &str) -> Result<(), String> {
    let (key, salt) = require_master()?;
    let mut db = load_master_db(&key)?;
    db.remove(entry_key);
    save_master_db(&key, &salt, &db)
}

pub fn set(host_id: i64, field: &str, value: &str) -> Result<(), String> {
    match mode() {
        Mode::Keyring => entry(host_id, field)?
            .set_password(value)
            .map_err(|e| e.to_string()),
        Mode::Master => master_set(&format!("host-{host_id}-{field}"), value),
    }
}

pub fn get(host_id: i64, field: &str) -> Result<Option<String>, String> {
    match mode() {
        Mode::Keyring => match entry(host_id, field)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        },
        Mode::Master => master_get(&format!("host-{host_id}-{field}")),
    }
}

pub fn delete(host_id: i64, field: &str) -> Result<(), String> {
    match mode() {
        Mode::Keyring => match entry(host_id, field)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        },
        Mode::Master => master_delete(&format!("host-{host_id}-{field}")),
    }
}

pub fn delete_all(host_id: i64) -> Result<(), String> {
    for field in ["password", "keyPassphrase"] {
        delete(host_id, field)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        delete, force_db_path_for_test, force_mode_for_test, get, master_unlock, set, Mode,
    };
    use std::path::PathBuf;

    fn test_path(name: &str) -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("vault-test")
            .join(name)
    }

    /// 单一生命周期测试：解锁幂等 → 增查 → 密文落盘 → 错误口令解密被拒 → 删查。
    /// 凭据库的全局解锁状态在进程内共享，拆成多个测试会互相污染。
    #[test]
    fn 主密码库完整生命周期() {
        let path = test_path("vault.bin");
        let _ = std::fs::remove_file(&path);
        force_db_path_for_test(path.clone());
        force_mode_for_test(Mode::Master);

        assert!(master_unlock("right-password").is_ok());
        // 幂等：同口令再次解锁成功
        assert!(master_unlock("right-password").is_ok());

        set(7, "password", "secret-value").unwrap();
        assert_eq!(get(7, "password").unwrap().as_deref(), Some("secret-value"));

        let raw = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("secret-value"));

        // 错误口令手工解密必须失败
        use argon2::Argon2;
        use ring::aead::{Aad, LessSafeKey, Nonce as RingNonce, UnboundKey, AES_256_GCM};
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&raw[..16]);
        let mut wrong_key = [0u8; 32];
        Argon2::default()
            .hash_password_into(b"wrong-password", &salt, &mut wrong_key)
            .unwrap();
        let (nonce_bytes, sealed) = raw[16..].split_at(12);
        let mut sealed = sealed.to_vec();
        let unbound = UnboundKey::new(&AES_256_GCM, &wrong_key).unwrap();
        let opening_key = LessSafeKey::new(unbound);
        let nonce = RingNonce::try_assume_unique_for_key(nonce_bytes.try_into().unwrap()).unwrap();
        assert!(opening_key.open_in_place(nonce, Aad::empty(), &mut sealed).is_err());

        delete(7, "password").unwrap();
        assert_eq!(get(7, "password").unwrap(), None);
        let _ = std::fs::remove_file(&path);
    }
}
