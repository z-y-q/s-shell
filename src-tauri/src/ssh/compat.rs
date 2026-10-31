use std::borrow::Cow;

use russh::cipher;
use russh::kex;
use russh::keys::Algorithm;
use russh::mac;
use russh::Preferred;

/// 连接配置：兼容模式追加老算法；高延迟线路可选 SSH 压缩。
pub fn preferred(compat_mode: bool, compress: bool) -> Preferred {
    let mut base = if compress {
        Preferred::COMPRESSED
    } else {
        Preferred::default()
    };
    if !compat_mode {
        return base;
    }

    let mut kex_list = base.kex.into_owned();
    kex_list.push(kex::DH_G14_SHA1);
    kex_list.push(kex::DH_G1_SHA1);
    base.kex = Cow::Owned(kex_list);

    let mut key_list = base.key.into_owned();
    // ssh-rsa（SHA-1 签名）供老服务器做主机密钥验证
    key_list.push(Algorithm::Rsa { hash: None });
    base.key = Cow::Owned(key_list);

    let mut cipher_list = base.cipher.into_owned();
    cipher_list.push(cipher::TRIPLE_DES_CBC);
    base.cipher = Cow::Owned(cipher_list);

    let mut mac_list = base.mac.into_owned();
    mac_list.push(mac::HMAC_SHA1);
    base.mac = Cow::Owned(mac_list);

    base
}
