use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh::{ChannelMsg, Disconnect};
use tauri::ipc::{Channel as TauriChannel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use super::compat;
use crate::ai;
use crate::codec::{self, SessionEncoding};
use crate::detect::ZmodemDetector;
use crate::forward::ForwardManager;
use crate::log;
use crate::record::Recorder;
use crate::store::{Host, Store};
use crate::tmux;
use crate::vault;

type CwdTable = Arc<Mutex<HashMap<u64, String>>>;

type PresentedKey = Arc<Mutex<Option<(String, String)>>>;
type RemoteForwardMap = Arc<Mutex<HashMap<(String, u16), (String, u16)>>>;

const RECONNECT_MAX: u32 = 5;

pub struct ClientHandler {
    expected_fp: Option<(String, String)>,
    presented: PresentedKey,
    remote_forwards: RemoteForwardMap,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // 只处理裸主机密钥；证书主机明确排除（见方案范围声明）
        let russh::keys::PublicKeyOrCertificate::PublicKey { key: public_key, .. } = key else {
            return Ok(false);
        };
        let algo = public_key.algorithm().as_str().to_string();
        let fingerprint = public_key
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        match &self.expected_fp {
            Some(expected) => {
                let matched = expected.0 == algo && expected.1 == fingerprint;
                if matched {
                    Ok(true)
                } else {
                    // 主机密钥与首次记录不一致：阻断连接
                    log::event(
                        "host-key-mismatch",
                        &format!("期望 {} {}，实际 {fingerprint}", expected.0, expected.1),
                    );
                    Ok(false)
                }
            }
            None => {
                *self.presented.lock().expect("指纹锁中毒") = Some((algo, fingerprint));
                Ok(true)
            }
        }
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let target = self
            .remote_forwards
            .lock()
            .expect("转发表锁中毒")
            .get(&(connected_address.to_string(), connected_port as u16))
            .cloned();
        let Some((target_host, target_port)) = target else {
            return Ok(());
        };
        let _ = reply.accept().await;
        let mut stream = channel.into_stream();
        tokio::spawn(async move {
            match tokio::net::TcpStream::connect((target_host.as_str(), target_port)).await {
                Ok(mut tcp) => {
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
                }
                Err(e) => println!("[s-shell] 远程转发目标连接失败：{e}"),
            }
        });
        Ok(())
    }
}

enum SessionCommand {
    Write(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    RecordStart { path: String },
    RecordStop,
    LogStart { path: String },
    LogStop,
    Close,
}

type SessionTable = Arc<Mutex<HashMap<u64, SessionEntry>>>;

struct SessionEntry {
    host_id: i64,
    cmd: mpsc::UnboundedSender<SessionCommand>,
    handle: Arc<Handle<ClientHandler>>,
}

pub struct SessionManager {
    next_id: AtomicU64,
    sessions: SessionTable,
    cwds: CwdTable,
}

struct SessionEntryRef {
    cmd: mpsc::UnboundedSender<SessionCommand>,
    handle: Arc<Handle<ClientHandler>>,
}

impl SessionManager {
    pub fn new() -> SessionManager {
        SessionManager {
            next_id: AtomicU64::new(1),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            cwds: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 外壳集成上报的当前目录（OSC 1337;CurrentDir）。
    pub fn cwd_of(&self, id: u64) -> Option<String> {
        self.cwds.lock().expect("目录表锁中毒").get(&id).cloned()
    }

    pub fn cwd_clone(&self) -> CwdTable {
        self.cwds.clone()
    }

    fn entry(&self, id: u64) -> Result<SessionEntryRef, String> {
        let table = self.sessions.lock().expect("会话表锁中毒");
        table
            .get(&id)
            .map(|e| SessionEntryRef { cmd: e.cmd.clone(), handle: e.handle.clone() })
            .ok_or_else(|| format!("会话 {id} 不存在"))
    }

    fn dispatch(&self, id: u64, cmd: SessionCommand) -> Result<(), String> {
        let entry = self.entry(id)?;
        if entry.cmd.send(cmd).is_err() {
            self.sessions.lock().expect("会话表锁中毒").remove(&id);
            return Err(format!("会话 {id} 已退出"));
        }
        Ok(())
    }

    pub fn write(&self, id: u64, data: Vec<u8>) -> Result<(), String> {
        self.dispatch(id, SessionCommand::Write(data))
    }

    pub fn resize(&self, id: u64, cols: u16, rows: u16) -> Result<(), String> {
        self.dispatch(id, SessionCommand::Resize { cols: cols as u32, rows: rows as u32 })
    }

    pub fn record_start(&self, id: u64, path: String) -> Result<(), String> {
        self.dispatch(id, SessionCommand::RecordStart { path })
    }

    pub fn record_stop(&self, id: u64) -> Result<(), String> {
        self.dispatch(id, SessionCommand::RecordStop)
    }

    pub fn log_start(&self, id: u64, path: String) -> Result<(), String> {
        self.dispatch(id, SessionCommand::LogStart { path })
    }

    pub fn log_stop(&self, id: u64) -> Result<(), String> {
        self.dispatch(id, SessionCommand::LogStop)
    }

    pub fn close(&self, id: u64) -> Result<(), String> {
        let result = self.dispatch(id, SessionCommand::Close);
        self.sessions.lock().expect("会话表锁中毒").remove(&id);
        result
    }

    pub fn handle_of(&self, id: u64) -> Result<Arc<Handle<ClientHandler>>, String> {
        Ok(self.entry(id)?.handle)
    }

    pub fn session_for_host(&self, host_id: i64) -> Option<u64> {
        let table = self.sessions.lock().expect("会话表锁中毒");
        table
            .iter()
            .find(|(_, e)| e.host_id == host_id)
            .map(|(id, _)| *id)
    }

    pub fn host_id_of_session(&self, id: u64) -> Result<i64, String> {
        let table = self.sessions.lock().expect("会话表锁中毒");
        table
            .get(&id)
            .map(|e| e.host_id)
            .ok_or_else(|| format!("会话 {id} 不存在"))
    }
}

/// 推送带 1 字节模式标的数据块：0 = 普通输出（前端直写 xterm 快路径），
/// 1 = zmodem 传输数据（前端喂给协议哨兵）。
fn push(on_data: &TauriChannel<InvokeResponseBody>, zmodem: bool, bytes: Vec<u8>) -> bool {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.push(if zmodem { 1 } else { 0 });
    out.extend_from_slice(&bytes);
    on_data.send(InvokeResponseBody::Raw(out)).is_ok()
}

fn notice(text: &str) -> Vec<u8> {
    format!("\r\n\u{1b}[36m[s-shell] {text}\u{1b}[0m\r\n").into_bytes()
}

/// 跳板链解析：反转后最外层跳板在前，目标在末位，最多 8 级并检测环路。
fn resolve_chain(store: &Store, target: &Host) -> Result<Vec<Host>, String> {
    let mut chain = vec![target.clone()];
    let mut current = target.clone();
    for _ in 0..8 {
        match current.jump_host_id {
            None => break,
            Some(jid) => {
                if chain.iter().any(|h| h.id == jid) {
                    return Err("跳板链存在环路".into());
                }
                let hop = store
                    .get(jid)?
                    .ok_or_else(|| format!("跳板主机 {jid} 不存在"))?;
                chain.push(hop.clone());
                current = hop;
            }
        }
    }
    chain.reverse();
    Ok(chain)
}

async fn authenticate(hop: &Host, handle: &mut Handle<ClientHandler>) -> Result<(), String> {
    match hop.auth_method.as_str() {
        "agent" => {
            let mut agent = connect_agent()
                .await
                .map_err(|e| format!("连接 ssh-agent 失败：{e}"))?;
            let identities = agent
                .request_identities()
                .await
                .map_err(|e| format!("读取 agent 密钥失败：{e}"))?;
            if identities.is_empty() {
                return Err("ssh-agent 里没有可用密钥".into());
            }
            let mut last_error = String::from("没有匹配的密钥");
            for identity in identities {
                let public_key = identity.public_key().into_owned();
                let hash_alg = if matches!(public_key.algorithm(), russh::keys::Algorithm::Rsa { .. })
                {
                    handle.best_supported_rsa_hash().await.map_err(|e| e.to_string())?.flatten()
                } else {
                    None
                };
                match handle
                    .authenticate_publickey_with(&hop.username, public_key, hash_alg, &mut agent)
                    .await
                {
                    Ok(auth) if auth.success() => return Ok(()),
                    Ok(_) => {
                        last_error = "服务器拒绝了 agent 密钥".into();
                    }
                    Err(e) => {
                        last_error = format!("agent 签名失败：{e}");
                    }
                }
            }
            Err(last_error)
        }
        "cert" => {
            let key_path = hop
                .key_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .ok_or("证书认证但未配置私钥路径")?;
            let cert_path = hop
                .cert_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .ok_or("证书认证但未配置证书路径")?;
            let passphrase = vault::get(hop.id, "keyPassphrase")?;
            let key = load_secret_key(&key_path, passphrase.as_deref())
                .map_err(|e| format!("读取私钥 {key_path} 失败：{e}"))?;
            let cert_text = std::fs::read_to_string(&cert_path)
                .map_err(|e| format!("读取证书 {cert_path} 失败：{e}"))?;
            let cert = russh::keys::Certificate::from_openssh(&cert_text)
                .map_err(|e| format!("解析证书失败：{e}"))?;
            let result = handle
                .authenticate_openssh_cert(&hop.username, Arc::new(key), cert)
                .await
                .map_err(|e| format!("证书认证失败：{e}"))?;
            if !result.success() {
                return Err(format!("服务器拒绝了证书：用户 {}", hop.username));
            }
            Ok(())
        }
        "key" => {
            let key_path = hop
                .key_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .ok_or("密钥认证但未配置私钥路径")?;
            let passphrase = vault::get(hop.id, "keyPassphrase")?;
            let key = load_secret_key(&key_path, passphrase.as_deref())
                .map_err(|e| format!("读取私钥 {key_path} 失败：{e}"))?;
            let hash_alg = if matches!(key.algorithm(), russh::keys::Algorithm::Rsa { .. }) {
                handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|e| e.to_string())?
                    .flatten()
            } else {
                None
            };
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);
            let result = handle
                .authenticate_publickey(&hop.username, key_with_hash)
                .await
                .map_err(|e| format!("密钥认证失败：{e}"))?;
            if !result.success() {
                return Err(format!("服务器拒绝了密钥：用户 {}", hop.username));
            }
            Ok(())
        }
        _ => {
            let password = vault::get(hop.id, "password")?
                .ok_or("未保存密码：请先在主机设置里填写密码")?;
            let result = handle
                .authenticate_password(&hop.username, password)
                .await
                .map_err(|e| format!("密码认证失败：{e}"))?;
            if !result.success() {
                return Err(format!("密码被拒绝：用户 {}", hop.username));
            }
            Ok(())
        }
    }
}

/// 连接本机 ssh-agent：优先 SSH_AUTH_SOCK；GUI 启动缺失时回退 macOS launchd 套接字。
/// 返回类型擦除后的客户端，调用方无需关心底层流类型。
async fn connect_agent()
-> Result<russh::keys::agent::client::AgentClient<Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>>, String>
{
    #[cfg(unix)]
    {
        use russh::keys::agent::client::AgentClient;
        if let Ok(path) = std::env::var("SSH_AUTH_SOCK") {
            if !path.is_empty() {
                let client = AgentClient::connect_uds(&path)
                    .await
                    .map_err(|e| format!("套接字 {path} 连接失败：{e}"))?;
                return Ok(client.dynamic());
            }
        }
        // Finder 启动的 GUI 应用没有 SSH_AUTH_SOCK：launchd 为每个登录用户
        // 准备了 /private/tmp/com.apple.launchd.*/Listeners 套接字
        let mut socket: Option<String> = None;
        if let Ok(entries) = std::fs::read_dir("/private/tmp") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("com.apple.launchd.") {
                    continue;
                }
                if let Ok(inner) = std::fs::read_dir(entry.path()) {
                    for f in inner.flatten() {
                        socket = Some(f.path().to_string_lossy().into_owned());
                    }
                }
            }
        }
        match socket {
            Some(path) => {
                let client = AgentClient::connect_uds(&path)
                    .await
                    .map_err(|e| format!("套接字 {path} 连接失败：{e}"))?;
                Ok(client.dynamic())
            }
            None => Err("未找到 ssh-agent 套接字".into()),
        }
    }
    #[cfg(windows)]
    {
        use russh::keys::agent::client::AgentClient;
        let client = AgentClient::connect_env()
            .await
            .map_err(|e| e.to_string())?;
        Ok(client.dynamic())
    }
}

/// 建立一条到 target 的连接：跳板链、逐级认证、known_hosts 固定。
/// 返回 (目标句柄, 必须保活的上级句柄, 首次连接记录到的指纹)。
pub async fn establish(
    store: &Store,
    target: &Host,
    remote_forwards: RemoteForwardMap,
) -> Result<(Handle<ClientHandler>, Vec<Handle<ClientHandler>>, Option<(String, String)>), String> {
    let chain = resolve_chain(store, target)?;
    let mut over: Option<russh::ChannelStream<client::Msg>> = None;
    let mut keepalive: Vec<Handle<ClientHandler>> = Vec::new();
    let mut new_fingerprint: Option<(String, String)> = None;

    for (i, hop) in chain.iter().enumerate() {
        let expected = store.known_hosts_get(&hop.host, hop.port)?;
        let presented: PresentedKey = Arc::new(Mutex::new(None));
        let config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(600)),
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_max: 3,
            preferred: compat::preferred(hop.compat_mode, hop.compress),
            ..Default::default()
        };
        let handler = ClientHandler {
            expected_fp: expected.clone(),
            presented: presented.clone(),
            remote_forwards: remote_forwards.clone(),
        };
        let connect_result = match over.take() {
            Some(stream) => client::connect_stream(Arc::new(config), stream, handler).await,
            None => match hop.proxy_kind.as_str() {
                "socks5" | "http" | "command" => {
                    let default_port = if hop.proxy_kind == "http" { 8080 } else { 1080 };
                    let proxy_host = hop
                        .proxy_host
                        .clone()
                        .unwrap_or_else(|| "127.0.0.1".into());
                    let proxy_port = hop.proxy_port.unwrap_or(default_port);
                    let stream: Box<dyn ProxyStream> =
                        match hop.proxy_kind.as_str() {
                            "socks5" => Box::new(
                                connect_via_socks5(&proxy_host, proxy_port, &hop.host, hop.port)
                                    .await?,
                            ),
                            "http" => Box::new(
                                connect_via_http(&proxy_host, proxy_port, &hop.host, hop.port)
                                    .await?,
                            ),
                            _ => Box::new(
                                spawn_proxy_command(
                                    hop.proxy_command
                                        .as_deref()
                                        .filter(|c| !c.trim().is_empty())
                                        .ok_or("ProxyCommand 认证方式已选但未配置命令")?,
                                )
                                .await?,
                            ),
                        };
                    log::event(
                        "proxy-connect",
                        &format!("{}:{} 经 {} 代理 {}:{} 连接", hop.host, hop.port, hop.proxy_kind, proxy_host, proxy_port),
                    );
                    client::connect_stream(Arc::new(config), stream, handler).await
                }
                _ => {
                    // 直连：自建 TcpStream 并显式关闭 Nagle。tokio 默认 nodelay=false，
                    // 击键这类小包会被 Nagle+延迟 ACK 拖出 100～400ms 的抖动
                    let tcp = tokio::net::TcpStream::connect((hop.host.clone(), hop.port))
                        .await
                        .map_err(|e| format!("连接 {}:{} 失败：{e}", hop.host, hop.port))?;
                    tcp.set_nodelay(true)
                        .map_err(|e| format!("设置 TCP_NODELAY 失败：{e}"))?;
                    client::connect_stream(Arc::new(config), tcp, handler).await
                }
            },
        };
        let mut handle = connect_result.map_err(|e| {
            if expected.is_some() {
                format!(
                    "连接 {}:{} 失败：{e}（若为密钥变更阻断，请先核实服务器身份，再到主机编辑里清除指纹）",
                    hop.host, hop.port
                )
            } else {
                format!("连接 {}:{} 失败：{e}", hop.host, hop.port)
            }
        })?;
        authenticate(hop, &mut handle).await?;

        if expected.is_none() {
            if let Some((algo, fp)) = presented.lock().expect("指纹锁中毒").take() {
                store.known_hosts_insert(&hop.host, hop.port, &algo, &fp)?;
                if i + 1 == chain.len() {
                    new_fingerprint = Some((algo, fp));
                }
            }
        }

        if i + 1 < chain.len() {
            let next = &chain[i + 1];
            let channel = handle
                .channel_open_direct_tcpip(next.host.clone(), next.port as u32, "127.0.0.1", 0)
                .await
                .map_err(|e| {
                    format!(
                        "经 {}:{} 打开通往 {}:{} 的隧道失败：{e}",
                        hop.host, hop.port, next.host, next.port
                    )
                })?;
            over = Some(channel.into_stream());
        }
        // 每一级（含已开隧道的跳板）都必须保活，drop 会立即断链
        keepalive.push(handle);
    }

    let final_handle = keepalive.pop().ok_or("跳板链解析失败")?;
    Ok((final_handle, keepalive, new_fingerprint))
}

/// 打开 PTY 与 shell，注入 tmux 策略，返回会话通道（连接与重连共用）。
/// 出站代理传输流的统一对象类型（dyn 只能挂一个非自动 trait，用 supertrait 聚合）。
trait ProxyStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ProxyStream for T {}

async fn connect_via_socks5(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> Result<tokio::net::TcpStream, String> {
    let mut tcp = tokio::net::TcpStream::connect((proxy_host, proxy_port))
        .await
        .map_err(|e| format!("连接 SOCKS5 代理 {proxy_host}:{proxy_port} 失败：{e}"))?;
    tcp.set_nodelay(true)
        .map_err(|e| format!("设置 TCP_NODELAY 失败：{e}"))?;
    // 无认证握手
    tcp.write_all(&[5, 1, 0]).await.map_err(|e| e.to_string())?;
    let mut method = [0u8; 2];
    tcp.read_exact(&mut method).await.map_err(|e| e.to_string())?;
    if method[0] != 5 || method[1] != 0 {
        return Err("SOCKS5 代理不支持无认证模式".into());
    }
    let mut req: Vec<u8> = vec![5, 1, 0];
    if let Ok(ip) = target_host.parse::<std::net::Ipv4Addr>() {
        req.push(1);
        req.extend_from_slice(&ip.octets());
    } else if let Ok(ip6) = target_host.parse::<std::net::Ipv6Addr>() {
        req.push(4);
        req.extend_from_slice(&ip6.octets());
    } else {
        req.push(3);
        req.push(target_host.len() as u8);
        req.extend_from_slice(target_host.as_bytes());
    }
    req.extend_from_slice(&target_port.to_be_bytes());
    tcp.write_all(&req).await.map_err(|e| e.to_string())?;
    let mut head = [0u8; 4];
    tcp.read_exact(&mut head).await.map_err(|e| e.to_string())?;
    if head[1] != 0 {
        return Err(format!("SOCKS5 代理拒绝连接（错误码 {}）", head[1]));
    }
    let skip = match head[3] {
        1 => 4,
        3 => {
            let mut l = [0u8; 1];
            tcp.read_exact(&mut l).await.map_err(|e| e.to_string())?;
            l[0] as usize
        }
        4 => 16,
        other => return Err(format!("SOCKS5 代理返回未知地址类型 {other}")),
    };
    let mut rest = vec![0u8; skip + 2];
    tcp.read_exact(&mut rest).await.map_err(|e| e.to_string())?;
    Ok(tcp)
}

async fn connect_via_http(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> Result<tokio::net::TcpStream, String> {
    let mut tcp = tokio::net::TcpStream::connect((proxy_host, proxy_port))
        .await
        .map_err(|e| format!("连接 HTTP 代理 {proxy_host}:{proxy_port} 失败：{e}"))?;
    tcp.set_nodelay(true)
        .map_err(|e| format!("设置 TCP_NODELAY 失败：{e}"))?;
    let request = format!("CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n\r\n");
    tcp.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut response: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        tcp.read_exact(&mut byte).await.map_err(|e| e.to_string())?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
        if response.len() > 8192 {
            return Err("HTTP 代理响应超长".into());
        }
    }
    let head = String::from_utf8_lossy(&response);
    if !head.contains(" 200") {
        let first_line = head.lines().next().unwrap_or("空响应");
        return Err(format!("HTTP 代理拒绝连接：{first_line}"));
    }
    Ok(tcp)
}

/// ProxyCommand 子进程桥：子进程的标准输入输出当作 SSH 传输流。
struct ProcStream {
    _child: tokio::process::Child, // 持有子进程保证存活
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
}

impl AsyncRead for ProcStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for ProcStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // 子进程流不主动 shutdown，连接关闭时随进程退出
        std::task::Poll::Ready(Ok(()))
    }
}

async fn spawn_proxy_command(command: &str) -> Result<ProcStream, String> {
    #[cfg(unix)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    };
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ProxyCommand 启动失败：{e}"))?;
    let stdin = child.stdin.take().ok_or("无法取得代理命令标准输入")?;
    let stdout = child.stdout.take().ok_or("无法取得代理命令标准输出")?;
    Ok(ProcStream { _child: child, stdin, stdout })
}

/// 打开 PTY 与 shell，注入 tmux 策略，返回会话通道（连接与重连共用）。
async fn open_shell_channel(
    handle: &Handle<ClientHandler>,
    host: &Host,
    cols: u32,
    rows: u32,
) -> Result<russh::Channel<client::Msg>, String> {
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("打开会话通道失败：{e}"))?;
    channel
        .request_pty(true, "xterm-256color", cols.max(2), rows.max(2), 0, 0, &[])
        .await
        .map_err(|e| format!("申请伪终端失败：{e}"))?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| format!("启动远端 shell 失败：{e}"))?;
    if let Some(cmd) = tmux::launch_command(tmux::parse(&host.tmux_policy)) {
        channel
            .data_bytes(cmd.as_bytes().to_vec())
            .await
            .map_err(|e| format!("注入 tmux 命令失败：{e}"))?;
    }
    if host.shell_integration {
        // bash 与 zsh 通用的 OSC 1337 目录上报钩子，供 AI 上下文使用。
        // 用 stty -echo 包裹，避免钩子命令本身被 TTY 回显成明文
        let hook = r#"stty -echo; if [ -n "$ZSH_VERSION" ]; then precmd() { printf '\033]1337;CurrentDir=%s\007' "$PWD"; }; elif [ -n "$BASH_VERSION" ]; then PROMPT_COMMAND="printf '\033]1337;CurrentDir=%s\007' \"\$PWD\";${PROMPT_COMMAND:+ $PROMPT_COMMAND}"; fi; stty echo"#;
        channel
            .data_bytes(format!("{hook}\n").into_bytes())
            .await
            .map_err(|e| format!("注入外壳集成钩子失败：{e}"))?;
    }
    Ok(channel)
}

pub async fn open(
    app: AppHandle,
    manager: &SessionManager,
    store: &Store,
    host_id: i64,
    cols: u16,
    rows: u16,
    on_data: TauriChannel<InvokeResponseBody>,
) -> Result<u64, String> {
    let host = store
        .get(host_id)?
        .ok_or_else(|| format!("主机 {host_id} 不存在"))?;
    let encoding = SessionEncoding::parse(&host.encoding);

    // 远程转发的接受逻辑在 handler 里，连接前先把规则灌进去
    let rules = store.forward_list(host.id)?;
    let remote_forwards: RemoteForwardMap = Arc::new(Mutex::new(
        rules
            .iter()
            .filter(|r| r.kind == "remote")
            .map(|r| {
                (
                    ("127.0.0.1".to_string(), r.listen_port),
                    (r.target_host.clone(), r.target_port),
                )
            })
            .collect(),
    ));

    let started = std::time::Instant::now();
    // 显式连接超时：不可达的 IP 否则会挂满整个 TCP 超时，界面表现为长时间空白
    let (handle, keepalive, new_fingerprint) =
        tokio::time::timeout(Duration::from_secs(15), establish(store, &host, remote_forwards.clone()))
            .await
            .map_err(|_| {
                log::event(
                    "connect-timeout",
                    &format!("{}:{} 连接超时（15 秒）", host.host, host.port),
                );
                format!("连接超时（15 秒）：请检查 IP、端口与防火墙（{}:{}）", host.host, host.port)
            })?
            .map_err(|e| {
                log::event("connect-fail", &format!("{}:{} {e}", host.host, host.port));
                e
            })?;
    log::event(
        "connect",
        &format!(
            "会话建立 {}:{}（用户 {}）耗时 {}ms",
            host.host,
            host.port,
            host.username,
            started.elapsed().as_millis()
        ),
    );
    let handle = Arc::new(handle);

    let channel = open_shell_channel(&handle, &host, cols as u32, rows as u32)
        .await
        .map_err(|e| {
            log::event("shell-fail", &format!("{}:{} {e}", host.host, host.port));
            e
        })?;

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<SessionCommand>();
    let id = manager.next_id.fetch_add(1, Ordering::Relaxed);
    manager.sessions.lock().expect("会话表锁中毒").insert(
        id,
        SessionEntry { host_id: host.id, cmd: cmd_tx, handle: handle.clone() },
    );

    if let Some((algo, fp)) = &new_fingerprint {
        log::event("host-key-new", &format!("{}:{} {algo} {fp}", host.host, host.port));
        if !push(&on_data, false, notice(&format!("首次连接，已记录主机指纹（{algo}）：{fp}"))) {
            return Err("前端通道已关闭".into());
        }
    }

    // 随会话自启转发规则
    let enabled: Vec<_> = rules.iter().filter(|r| r.enabled).cloned().collect();
    if !enabled.is_empty() {
        let fm = app.state::<ForwardManager>();
        let mut started_rules = Vec::new();
        for rule in &enabled {
            match fm.start_rule(&app, id, handle.clone(), rule.clone()).await {
                Ok(_) => started_rules.push(format!("{} → {}:{}", rule.kind, rule.target_host, rule.target_port)),
                Err(e) => started_rules.push(format!("失败：{e}")),
            }
        }
        drop(fm);
        let _ = push(&on_data, false, notice(&format!("转发规则：{}", started_rules.join("，"))));
    }

    let (ch_read, ch_write) = channel.split();
    tokio::spawn(session_task(
        app,
        manager.sessions.clone(),
        id,
        host,
        encoding,
        remote_forwards,
        (handle, keepalive, ch_read, ch_write),
        cmd_rx,
        on_data,
    ));
    Ok(id)
}

/// 已编译的输出触发器：正则命中后在冷却期内自动回包
struct Trigger {
    re: regex::Regex,
    payload: String,
    last_fire: Option<std::time::Instant>,
}

struct SessionState {
    decoder: codec::Decoder,
    zmodem: ZmodemDetector,
    triggers: Vec<Trigger>,
    agg: Vec<u8>,
    deadline: Option<tokio::time::Instant>,
    recorder: Option<Recorder>,
    log_file: Option<std::fs::File>,
    dims: (u32, u32),
    recent: Vec<u8>,
    since_mark: u64,
    last_flush: Option<tokio::time::Instant>,
    auto_summary: bool,
    last_write: Option<tokio::time::Instant>,
    rtt_samples: Vec<u64>,
    write_waits: Vec<u64>,
    suppress_hook_until: Option<tokio::time::Instant>,
}

impl SessionState {
    fn new(encoding: SessionEncoding) -> SessionState {
        SessionState {
            decoder: codec::Decoder::new(encoding),
            zmodem: ZmodemDetector::new(),
            triggers: Vec::new(),
            agg: Vec::with_capacity(8192),
            deadline: None,
            recorder: None,
            log_file: None,
            dims: (0, 0),
            recent: Vec::with_capacity(50_000),
            since_mark: 0,
            last_flush: None,
            auto_summary: false,
            last_write: None,
            rtt_samples: Vec::with_capacity(64),
            write_waits: Vec::with_capacity(64),
            suppress_hook_until: None,
        }
    }

    /// 回显 RTT 采样：击键写出后到下一批输出到达的间隔。
    /// 攒满 10 个样本输出一条中位数/最大值日志并清空。
    fn sample_echo(&mut self, id: u64) {
        if let Some(t) = self.last_write.take() {
            self.rtt_samples.push(t.elapsed().as_micros() as u64);
        }
        if self.rtt_samples.len() >= 10 {
            let mut sorted = self.rtt_samples.clone();
            sorted.sort_unstable();
            let n = sorted.len();
            let median = sorted[n / 2];
            let max = sorted[n - 1];
            let min = sorted[0];
            let mut waits = self.write_waits.clone();
            waits.sort_unstable();
            let wait_med = waits[waits.len() / 2];
            let wait_max = waits[waits.len() - 1];
            log::event(
                "echo-rtt",
                &format!(
                    "会话 {id} 样本 {n}：回显最小 {}ms 中位 {}ms 最大 {}ms ｜ 写出等待中位 {}ms 最大 {}ms",
                    min / 1000,
                    median / 1000,
                    max / 1000,
                    wait_med / 1000,
                    wait_max / 1000
                ),
            );
            self.rtt_samples.clear();
            self.write_waits.clear();
        }
    }

    fn push_recent(&mut self, bytes: &[u8]) {
        self.recent.extend_from_slice(bytes);
        if self.recent.len() > 50_000 {
            let cut = self.recent.len() - 50_000;
            self.recent.drain(..cut);
        }
        self.since_mark += bytes.len() as u64;
    }
}

enum PumpOutcome {
    UserClose,
    Disconnected(String),
}

const AGG_WINDOW: Duration = Duration::from_millis(4);
const AGG_LIMIT: usize = 64 * 1024;
const SUMMARY_THRESHOLD: u64 = 200_000;
const SUMMARY_IDLE: Duration = Duration::from_secs(2);

const CWD_MARKER: &str = "]1337;CurrentDir=";

/// 从解码文本里提取外壳集成上报的当前目录（跨块截断时尽力而为，下个提示符会再报）。
fn extract_cwd(text: &str) -> Option<String> {
    let start = text.find(CWD_MARKER)? + CWD_MARKER.len();
    let rest = &text[start..];
    let end = rest
        .find('\u{7}')
        .or_else(|| rest.find("\u{1b}\\"))
        .unwrap_or(rest.len());
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn flush_agg(
    state: &mut SessionState,
    on_data: &TauriChannel<InvokeResponseBody>,
) -> bool {
    if let Some(r) = state.recorder.as_mut() {
        r.output(&String::from_utf8_lossy(&state.agg));
    }
    if let Some(f) = state.log_file.as_mut() {
        use std::io::Write as _;
        let _ = f.write_all(&state.agg);
        let _ = f.flush();
    }
    let taken = std::mem::take(&mut state.agg);
    state.push_recent(&taken);
    push(on_data, false, taken)
}

/// 输出爆发结束（距上次刷新超过 2 秒）且累计超阈值时，触发一次自动摘要。
fn maybe_summarize(state: &mut SessionState, app: &AppHandle, id: u64, now: tokio::time::Instant) {
    let idle_enough = state
        .last_flush
        .is_some_and(|prev| now - prev >= SUMMARY_IDLE);
    if idle_enough && state.since_mark > SUMMARY_THRESHOLD && state.auto_summary {
        state.since_mark = 0;
        let recent = String::from_utf8_lossy(&state.recent).into_owned();
        let app2 = app.clone();
        tokio::spawn(async move {
            let store = app2.state::<Store>();
            match ai::summarize(store.inner(), &recent).await {
                Ok(text) => {
                    let _ = app2.emit("ai-summary", serde_json::json!({ "sessionId": id, "text": text }));
                }
                Err(e) => println!("[s-shell] 自动摘要失败：{e}"),
            }
        });
    }
    state.last_flush = Some(now);
}

async fn pump(
    app: &AppHandle,
    id: u64,
    cwds: &CwdTable,
    ch_read: &mut russh::ChannelReadHalf,
    ch_write: &russh::ChannelWriteHalf<client::Msg>,
    state: &mut SessionState,
    encoding: SessionEncoding,
    cmd_rx: &mut mpsc::UnboundedReceiver<SessionCommand>,
    on_data: &TauriChannel<InvokeResponseBody>,
) -> PumpOutcome {
    loop {
        // 冲刷定时器：期限一到立即唤醒循环冲刷聚合缓冲。
        // 缺了它，聚合只被下一个事件驱动——表现为「打第二个字才显示第一个字」
        let mut flush_timer: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            match state.deadline {
                Some(d) => Box::pin(tokio::time::sleep_until(d)),
                None => Box::pin(std::future::pending()),
            };
        tokio::select! {
            _ = &mut flush_timer => {
                state.deadline = None;
                if !state.agg.is_empty() {
                    if !flush_agg(state, on_data) {
                        return PumpOutcome::Disconnected("前端通道已关闭".into());
                    }
                }
            }
            msg = ch_read.wait() => {
                let Some(msg) = msg else {
                    return PumpOutcome::Disconnected("SSH 连接已断开".into());
                };
                let bytes = match msg {
                    ChannelMsg::Data { data } => data.to_vec(),
                    ChannelMsg::ExtendedData { data, .. } => data.to_vec(),
                    ChannelMsg::Eof | ChannelMsg::Close => {
                        return PumpOutcome::Disconnected("远端关闭了会话".into());
                    }
                    _ => continue,
                };
                if state.last_write.is_some() {
                    state.sample_echo(id);
                }
                if state.zmodem.feed(&bytes) {
                    if !state.agg.is_empty() {
                        if let Some(r) = state.recorder.as_mut() {
                            r.output(&String::from_utf8_lossy(&state.agg));
                        }
                        if !push(on_data, false, std::mem::take(&mut state.agg)) {
                            return PumpOutcome::Disconnected("前端通道已关闭".into());
                        }
                        state.deadline = None;
                    }
                    if let Some(r) = state.recorder.as_mut() {
                        r.output(&String::from_utf8_lossy(&bytes));
                    }
                    if !push(on_data, true, bytes) {
                        return PumpOutcome::Disconnected("前端通道已关闭".into());
                    }
                    continue;
                }
                let text = state.decoder.decode(&bytes);
                // 外壳集成钩子的回显剥离（连接后 3 秒内生效）
                if let Some(until) = state.suppress_hook_until {
                    if tokio::time::Instant::now() >= until {
                        state.suppress_hook_until = None;
                    } else if text.contains("1337;CurrentDir=%s") {
                        let kept: Vec<&str> = text
                            .lines()
                            .filter(|l| !l.contains("1337;CurrentDir=%s"))
                            .collect();
                        if kept.is_empty() {
                            continue;
                        }
                        state.agg.extend_from_slice(kept.join("\n").as_bytes());
                        continue;
                    }
                }
                if let Some(cwd) = extract_cwd(&text) {
                    cwds.lock().expect("目录表锁中毒").insert(id, cwd);
                }
                // 触发器扫描：正则命中自动回包，冷却 2 秒防连发
                if !state.triggers.is_empty() {
                    let now = std::time::Instant::now();
                    for tr in state.triggers.iter_mut() {
                        if tr.re.is_match(&text)
                            && tr.last_fire.map_or(true, |t| {
                                now.duration_since(t) >= std::time::Duration::from_secs(2)
                            })
                        {
                            tr.last_fire = Some(now);
                            let mut payload = tr.payload.clone().into_bytes();
                            payload.push(b'\n');
                            let _ = ch_write.data_bytes(payload).await;
                        }
                    }
                }
                state.agg.extend_from_slice(text.as_bytes());
                let now = tokio::time::Instant::now();
                // 交互优先：距上次冲刷超过窗口（或首次输出）→ 立即冲刷，击键回显零排队；
                // 洪泛时才批处理，定时器兜底把窗口尾部的数据带出去
                let since_last = state
                    .last_flush
                    .map(|t| now.duration_since(t))
                    .unwrap_or(AGG_WINDOW);
                let full = state.agg.len() >= AGG_LIMIT;
                if full || since_last >= AGG_WINDOW {
                    if !flush_agg(state, on_data) {
                        return PumpOutcome::Disconnected("前端通道已关闭".into());
                    }
                    maybe_summarize(state, app, id, now);
                    state.deadline = None;
                    state.last_flush = Some(now);
                } else {
                    state.deadline = Some(state.last_flush.unwrap() + AGG_WINDOW);
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(SessionCommand::Write(bytes)) => {
                        // zmodem 传输中键盘与协议帧都要原样直写，绝不过编码
                        let write_start = tokio::time::Instant::now();
                        let outcome = if state.zmodem.active() {
                            ch_write.data_bytes(bytes).await
                        } else {
                            let text = String::from_utf8_lossy(&bytes).into_owned();
                            let encoded = codec::encode(encoding, &text);
                            ch_write.data_bytes(encoded).await
                        };
                        state.write_waits.push(write_start.elapsed().as_micros() as u64);
                        if let Err(e) = outcome {
                            return PumpOutcome::Disconnected(format!("写入通道失败：{e}"));
                        }
                        state.last_write = Some(tokio::time::Instant::now());
                    }
                    Some(SessionCommand::Resize { cols, rows }) => {
                        state.dims = (cols, rows);
                        if let Some(r) = state.recorder.as_mut() {
                            r.resize(cols, rows);
                        }
                        // 尺寸协商失败不影响会话本身
                        let _ = ch_write.window_change(cols, rows, 0, 0).await;
                    }
                    Some(SessionCommand::RecordStart { path }) => {
                        match Recorder::start(std::path::Path::new(&path), state.dims.0.max(80), state.dims.1.max(24)) {
                            Ok(r) => state.recorder = Some(r),
                            Err(e) => println!("[s-shell] 开启录制失败：{e}"),
                        }
                    }
                    Some(SessionCommand::RecordStop) => {
                        if let Some(r) = state.recorder.take() {
                            r.stop();
                        }
                    }
                    Some(SessionCommand::LogStart { path }) => {
                        match std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(std::path::Path::new(&path))
                        {
                            Ok(f) => state.log_file = Some(f),
                            Err(e) => println!("[s-shell] 开启会话日志失败：{e}"),
                        }
                    }
                    Some(SessionCommand::LogStop) => {
                        state.log_file = None;
                    }
                    Some(SessionCommand::Close) | None => return PumpOutcome::UserClose,
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn session_task(
    app: AppHandle,
    sessions: SessionTable,
    id: u64,
    host: Host,
    encoding: SessionEncoding,
    remote_forwards: RemoteForwardMap,
    first: (Arc<Handle<ClientHandler>>, Vec<Handle<ClientHandler>>, russh::ChannelReadHalf, russh::ChannelWriteHalf<client::Msg>),
    mut cmd_rx: mpsc::UnboundedReceiver<SessionCommand>,
    on_data: TauriChannel<InvokeResponseBody>,
) {
    let (mut handle, mut keepalive, mut ch_read, mut ch_write) = first;
    let mut state = SessionState::new(encoding);
    state.auto_summary = matches!(
        app.state::<Store>().setting_get("ai_autosummary").ok().flatten().as_deref(),
        Some("1")
    );
    // 触发器：全局 + 绑定本主机的正则规则，编译失败的跳过
    state.triggers = app
        .state::<Store>()
        .trigger_list()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t.host_id.is_none() || t.host_id == Some(host.id))
        .filter_map(|t| match regex::Regex::new(&t.pattern) {
            Ok(re) => Some(Trigger {
                re,
                payload: t.payload,
                last_fire: None,
            }),
            Err(e) => {
                println!("[s-shell] 触发器正则编译失败：{e}");
                None
            }
        })
        .collect();
    if host.shell_integration {
        state.suppress_hook_until = Some(tokio::time::Instant::now() + Duration::from_secs(3));
    }
    let cwds = app.state::<SessionManager>();
    let cwd_table = cwds.inner().cwd_clone();
    drop(cwds);

    let mut outcome = pump(&app, id, &cwd_table, &mut ch_read, &ch_write, &mut state, encoding, &mut cmd_rx, &on_data).await;
    let mut user_closed = false;
    let mut failure_reason = String::from("会话结束");

    loop {
        match outcome {
            PumpOutcome::UserClose => {
                user_closed = true;
                break;
            }
            PumpOutcome::Disconnected(reason) => {
                failure_reason = reason;
                let store_guard = app.state::<Store>();
                let store_ref = store_guard.inner();
                let mut reconnected = false;
                for attempt in 1..=RECONNECT_MAX {
                    let wait = Duration::from_secs(1 << (attempt - 1));
                    let _ = push(
                        &on_data,
                        false,
                        notice(&format!(
                            "连接断开（{failure_reason}），{} 秒后进行第 {attempt}/{RECONNECT_MAX} 次重连…",
                            wait.as_secs()
                        )),
                    );
                    log::event(
                        "reconnect",
                        &format!("会话 {id} {}:{} 第 {attempt} 次重连将在 {}s 后开始", host.host, host.port, wait.as_secs()),
                    );
                    tokio::time::sleep(wait).await;
                    let attempt_result = tokio::time::timeout(
                        Duration::from_secs(15),
                        establish(store_ref, &host, remote_forwards.clone()),
                    )
                    .await
                    .map_err(|_| format!("连接超时（15 秒）"));
                    match attempt_result.and_then(|r| r) {
                        Ok((new_handle, new_keepalive, _)) => {
                            let new_handle = Arc::new(new_handle);
                            match open_shell_channel(
                                &new_handle,
                                &host,
                                state.dims.0.max(80),
                                state.dims.1.max(24),
                            )
                            .await
                            {
                                Ok(channel) => {
                                    (ch_read, ch_write) = channel.split();
                                    sessions
                                        .lock()
                                        .expect("会话表锁中毒")
                                        .get_mut(&id)
                                        .expect("重连中的会话不应被移除")
                                        .handle = new_handle.clone();
                                    handle = new_handle.clone();
                                    keepalive = new_keepalive;
                                    state.decoder = codec::Decoder::new(encoding);
                                    state.agg.clear();
                                    state.deadline = None;
                                    let _ = push(
                                        &on_data,
                                        false,
                                        notice("重连成功，会话已恢复（进程现场由 tmux 策略决定）"),
                                    );
                                    log::event(
                                        "reconnect",
                                        &format!("会话 {id} {}:{} 第 {attempt} 次重连成功", host.host, host.port),
                                    );
                                    if let Ok(rules) = store_ref.forward_list(host.id) {
                                        let fm = app.state::<ForwardManager>();
                                        for rule in rules.iter().filter(|r| r.enabled) {
                                            let _ = fm
                                                .start_rule(&app, id, new_handle.clone(), rule.clone())
                                                .await;
                                        }
                                        drop(fm);
                                    }
                                    reconnected = true;
                                    break;
                                }
                                Err(e) => {
                                    failure_reason = format!("shell 重建失败：{e}");
                                    log::event("reconnect-fail", &format!("会话 {id} shell 重建失败：{e}"));
                                }
                            }
                        }
                        Err(e) => {
                            failure_reason = e;
                            log::event(
                                "reconnect-fail",
                                &format!("会话 {id} {}:{} 第 {attempt} 次重连失败：{failure_reason}", host.host, host.port),
                            );
                        }
                    }
                }
                drop(store_guard);
                if !reconnected {
                    let _ = push(
                        &on_data,
                        false,
                        notice(&format!("已重试 {RECONNECT_MAX} 次仍未连上，会话结束")),
                    );
                    log::event(
                        "reconnect-fail",
                        &format!("会话 {id} {}:{} 重试耗尽", host.host, host.port),
                    );
                    break;
                }
                outcome = pump(&app, id, &cwd_table, &mut ch_read, &ch_write, &mut state, encoding, &mut cmd_rx, &on_data).await;
            }
        }
    }

    if let Some(r) = state.recorder.take() {
        r.stop();
    }
    if let Some(fm) = app.try_state::<ForwardManager>() {
        fm.stop_session(id);
    }
    sessions.lock().expect("会话表锁中毒").remove(&id);
    let _ = ch_write.close().await;
    let _ = handle.disconnect(Disconnect::ByApplication, "s-shell 会话关闭", "zh").await;
    drop(keepalive);
    let _ = app.emit("session-closed", id);
    log::event(
        "session-close",
        &format!(
            "会话 {id} {}:{} {}",
            host.host,
            host.port,
            if user_closed { "用户关闭".to_string() } else { format!("异常结束：{failure_reason}") }
        ),
    );
    println!("[s-shell] 会话 {id} 退出");
}

/// 为 MCP 等外部调用建立连接并执行单条命令，收集全部输出。
pub async fn exec_command(store: &Store, host: &Host, command: &str, timeout: Duration) -> Result<String, String> {
    let connect = async {
        establish(store, host, Arc::new(Mutex::new(HashMap::new()))).await
    };
    let (handle, _keepalive, _) = tokio::time::timeout(Duration::from_secs(15), connect)
        .await
        .map_err(|_| format!("连接超时（15 秒）：请检查 IP、端口与防火墙（{}:{}）", host.host, host.port))?
        .map_err(|e| format!("连接失败：{e}"))?;
    let mut channel: russh::Channel<client::Msg> = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("打开执行通道失败：{e}"))?;
    channel
        .exec(true, command.as_bytes().to_vec())
        .await
        .map_err(|e| format!("下发命令失败：{e}"))?;

    let mut out: Vec<u8> = Vec::new();
    let wait_loop = async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => out.extend_from_slice(&data),
                Some(ChannelMsg::ExtendedData { data, .. }) => out.extend_from_slice(&data),
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    };
    tokio::time::timeout(timeout, wait_loop)
        .await
        .map_err(|_| "命令执行超时".to_string())?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ProbeHandler;

    impl client::Handler for ProbeHandler {
        type Error = russh::Error;

        async fn check_server_key(
            &mut self,
            _key: &russh::keys::PublicKeyOrCertificate,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    /// 真实 SSH 协议栈连通验证：完整走完密钥交换、主机密钥校验、认证交互。
    /// 需要外网，手动执行：cargo test -- --ignored
    #[tokio::test]
    #[ignore = "需要外网"]
    async fn ssh协议栈连通验证() {
        let config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        };
        let mut handle = client::connect(Arc::new(config), ("github.com", 22), ProbeHandler)
            .await
            .expect("SSH 连接握手失败");
        let result = handle
            .authenticate_password("s-shell-probe", "not-a-real-password")
            .await
            .expect("认证交互失败");
        assert!(!result.success(), "伪造凭据不应该认证成功");
        let _ = handle
            .disconnect(Disconnect::ByApplication, "probe", "zh")
            .await;
    }

    fn host_named(name: &str) -> Host {
        Host {
            id: 0,
            name: name.into(),
            host: "10.0.0.1".into(),
            port: 22,
            username: "ops".into(),
            auth_method: "password".into(),
            key_path: None,
            cert_path: None,
            encoding: "utf8".into(),
            compat_mode: false,
            tmux_policy: "none".into(),
            jump_host_id: None,
            shell_integration: false,
            proxy_kind: "none".into(),
            proxy_host: None,
            proxy_port: None,
            proxy_command: None,
            compress: false,
            updated_at: String::new(),
        }
    }

    /// 内核吞吐基准：100MB 输出经解码与聚合管线的完整处理。
    /// 口径说明：ASCII 载荷保证字节数无损可比（多字节跨块替换语义由单测覆盖）；
    /// 含解码、聚合窗口与缓冲管理，不含 IPC 推送与 WebView 渲染。
    /// 手动执行：cargo test -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "性能基准"]
    async fn 性能_百MB解码聚合吞吐() {
        let block = "mixed ascii output 12345 ".repeat(8);
        let block = block.as_bytes();
        let total: u64 = 100 * 1024 * 1024;
        let mut state = SessionState::new(SessionEncoding::Utf8);
        let mut fed: u64 = 0;
        let mut pushed: u64 = 0;
        let mut sink: Vec<u8> = Vec::new();
        let start = std::time::Instant::now();
        while fed < total {
            let n = std::cmp::min(block.len() as u64, total - fed) as usize;
            let text = state.decoder.decode(&block[..n]);
            state.agg.extend_from_slice(text.as_bytes());
            fed += n as u64;
            if state.agg.len() >= AGG_LIMIT {
                sink.extend_from_slice(&state.agg);
                pushed += state.agg.len() as u64;
                state.agg.clear();
            }
        }
        // 收尾冲刷聚合窗口尾部与解码器挂起的半字符
        sink.extend_from_slice(&state.agg);
        pushed += state.agg.len() as u64;
        state.agg.clear();
        let tail = state.decoder.finish();
        pushed += tail.len() as u64;
        sink.extend_from_slice(tail.as_bytes());
        let elapsed = start.elapsed();
        let mibs = (fed as f64 / 1_048_576.0) / elapsed.as_secs_f64();
        println!(
            "[bench] 100MB 解码+聚合：耗时 {:?}，吞吐 {:.1} MiB/s，聚合输出 {} MiB",
            elapsed,
            mibs,
            pushed / 1_048_576
        );
        assert_eq!(fed, total);
        assert_eq!(pushed, total);
        assert!(mibs > 50.0, "吞吐 {} MiB/s 低于 50MiB/s 验收下限", mibs);
        drop(sink);
    }

    #[test]
    fn 跳板链解析与防环() {        let store = Store::open_memory();
        let target_id = store.save(None, &host_named("target")).unwrap();
        let mut jump1 = host_named("jump1");
        jump1.jump_host_id = Some(target_id);
        let jump1_id = store.save(None, &jump1).unwrap();
        let mut target = store.get(target_id).unwrap().unwrap();
        target.jump_host_id = Some(jump1_id);

        // 环路：target → jump1 → target
        let err = resolve_chain(&store, &target).unwrap_err();
        assert!(err.contains("环路"));

        // 正常链：target 经 jump1 到达，解析结果应为 [jump1, target]
        let mut normal_target = store.get(target_id).unwrap().unwrap();
        normal_target.jump_host_id = Some(jump1_id);
        store.save(Some(target_id), &normal_target).unwrap();
        let mut hop = store.get(jump1_id).unwrap().unwrap();
        hop.jump_host_id = None;
        store.save(Some(jump1_id), &hop).unwrap();
        let chain = resolve_chain(&store, &store.get(target_id).unwrap().unwrap()).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].name, "jump1");
        assert_eq!(chain[1].name, "target");
    }
}
