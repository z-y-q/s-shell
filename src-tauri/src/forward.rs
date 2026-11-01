use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::Handle;
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;

use crate::ssh::session::ClientHandler;
use crate::store::Forward;

pub struct ForwardManager {
    next_id: AtomicU64,
    runtimes: Mutex<HashMap<u64, Runtime>>,
}

struct Runtime {
    session_id: u64,
    rule_id: i64,
    task: JoinHandle<()>,
    // remote 转发停止时要向服务器发取消请求
    remote: Option<(Arc<Handle<ClientHandler>>, String, u16)>,
}

impl ForwardManager {
    pub fn new() -> ForwardManager {
        ForwardManager { next_id: AtomicU64::new(1), runtimes: Mutex::new(HashMap::new()) }
    }

    pub async fn start_rule(
        &self,
        app: &AppHandle,
        session_id: u64,
        handle: Arc<Handle<ClientHandler>>,
        rule: Forward,
    ) -> Result<u64, String> {
        let rt_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let runtime = match rule.kind.as_str() {
            "local" => {
                let task = tokio::spawn(local_forward_task(
                    handle,
                    rule.listen_port,
                    rule.target_host.clone(),
                    rule.target_port,
                    app.clone(),
                ));
                Runtime { session_id, rule_id: rule.id, task, remote: None }
            }
            "dynamic" => {
                let task = tokio::spawn(dynamic_forward_task(handle, rule.listen_port, app.clone()));
                Runtime { session_id, rule_id: rule.id, task, remote: None }
            }
            "remote" => {
                handle
                    .tcpip_forward("127.0.0.1", rule.listen_port as u32)
                    .await
                    .map_err(|e| format!("远端监听 {} 失败：{e}", rule.listen_port))?;
                // 通道接受发生在 handler 的 server_channel_open_forwarded_tcpip 里，
                // 这个占位任务只承载 abort 语义
                let task = tokio::spawn(async {
                    loop {
                        tokio::time::sleep(Duration::from_secs(3600)).await;
                    }
                });
                Runtime {
                    session_id,
                    rule_id: rule.id,
                    task,
                    remote: Some((handle, "127.0.0.1".into(), rule.listen_port)),
                }
            }
            other => return Err(format!("未知转发类型：{other}")),
        };
        self.runtimes.lock().expect("转发表锁中毒").insert(rt_id, runtime);
        Ok(rt_id)
    }

    pub fn stop_runtime(&self, rt_id: u64) {
        let Some(runtime) = self.runtimes.lock().expect("转发表锁中毒").remove(&rt_id) else {
            return;
        };
        runtime.task.abort();
        if let Some((handle, addr, port)) = runtime.remote {
            tokio::spawn(async move {
                let _ = handle.cancel_tcpip_forward(&addr, port as u32).await;
            });
        }
    }

    pub fn stop_rule(&self, rule_id: i64) {
        let ids: Vec<u64> = {
            let table = self.runtimes.lock().expect("转发表锁中毒");
            table
                .iter()
                .filter(|(_, r)| r.rule_id == rule_id)
                .map(|(id, _)| *id)
                .collect()
        };
        for id in ids {
            self.stop_runtime(id);
        }
    }

    /// 当前正在监听的规则编号（状态灯用）
    pub fn active_rule_ids(&self) -> Vec<i64> {
        self.runtimes
            .lock()
            .expect("转发表锁中毒")
            .values()
            .map(|r| r.rule_id)
            .collect()
    }

    pub fn stop_session(&self, session_id: u64) {
        let ids: Vec<u64> = {
            let table = self.runtimes.lock().expect("转发表锁中毒");
            table
                .iter()
                .filter(|(_, r)| r.session_id == session_id)
                .map(|(id, _)| *id)
                .collect()
        };
        for id in ids {
            self.stop_runtime(id);
        }
    }
}

async fn local_forward_task(
    handle: Arc<Handle<ClientHandler>>,
    listen_port: u16,
    target_host: String,
    target_port: u16,
    app: AppHandle,
) {
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", listen_port)).await {
        Ok(l) => l,
        Err(e) => {
            let _ = app.emit(
                "forward-error",
                serde_json::json!({ "listenPort": listen_port, "error": format!("{e}") }),
            );
            println!("[s-shell] 本地转发监听 {listen_port} 失败：{e}");
            return;
        }
    };
    loop {
        let Ok((mut tcp, _peer)) = listener.accept().await else {
            return;
        };
        let handle = handle.clone();
        let target_host = target_host.clone();
        tokio::spawn(async move {
            match handle
                .channel_open_direct_tcpip(&target_host, target_port as u32, "127.0.0.1", 0)
                .await
            {
                Ok(channel) => {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
                }
                Err(e) => println!("[s-shell] 转发通道打开失败：{e}"),
            }
        });
    }
}

async fn dynamic_forward_task(handle: Arc<Handle<ClientHandler>>, listen_port: u16, app: AppHandle) {
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", listen_port)).await {
        Ok(l) => l,
        Err(e) => {
            let _ = app.emit(
                "forward-error",
                serde_json::json!({ "listenPort": listen_port, "error": format!("{e}") }),
            );
            println!("[s-shell] SOCKS5 监听 {listen_port} 失败：{e}");
            return;
        }
    };
    loop {
        let Ok((tcp, _peer)) = listener.accept().await else {
            return;
        };
        let handle = handle.clone();
        tokio::spawn(socks5_serve(tcp, handle));
    }
}

/// 最小 SOCKS5 服务：仅 CONNECT、无认证，目标经远端 direct-tcpip 通道打开。
async fn socks5_serve(mut tcp: tokio::net::TcpStream, handle: Arc<Handle<ClientHandler>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut head = [0u8; 2];
    if tcp.read_exact(&mut head).await.is_err() {
        return;
    }
    if head[0] != 5 {
        return;
    }
    let mut methods = vec![0u8; head[1] as usize];
    if tcp.read_exact(&mut methods).await.is_err() {
        return;
    }
    if tcp.write_all(&[5, 0]).await.is_err() {
        return;
    }

    let mut req = [0u8; 4];
    if tcp.read_exact(&mut req).await.is_err() {
        return;
    }
    if req[0] != 5 || req[1] != 1 {
        let _ = tcp.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return;
    }
    let target: String = match req[3] {
        1 => {
            let mut o = [0u8; 4];
            if tcp.read_exact(&mut o).await.is_err() {
                return;
            }
            format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3])
        }
        3 => {
            let mut l = [0u8; 1];
            if tcp.read_exact(&mut l).await.is_err() {
                return;
            }
            let mut name = vec![0u8; l[0] as usize];
            if tcp.read_exact(&mut name).await.is_err() {
                return;
            }
            String::from_utf8_lossy(&name).into_owned()
        }
        4 => {
            let mut o = [0u8; 16];
            if tcp.read_exact(&mut o).await.is_err() {
                return;
            }
            std::net::Ipv6Addr::new(
                u16::from_be_bytes([o[0], o[1]]),
                u16::from_be_bytes([o[2], o[3]]),
                u16::from_be_bytes([o[4], o[5]]),
                u16::from_be_bytes([o[6], o[7]]),
                u16::from_be_bytes([o[8], o[9]]),
                u16::from_be_bytes([o[10], o[11]]),
                u16::from_be_bytes([o[12], o[13]]),
                u16::from_be_bytes([o[14], o[15]]),
            )
            .to_string()
        }
        _ => {
            let _ = tcp.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            return;
        }
    };
    let mut port_bytes = [0u8; 2];
    if tcp.read_exact(&mut port_bytes).await.is_err() {
        return;
    }
    let port = u16::from_be_bytes(port_bytes);

    match handle
        .channel_open_direct_tcpip(&target, port as u32, "127.0.0.1", 0)
        .await
    {
        Ok(channel) => {
            if tcp.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.is_err() {
                return;
            }
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
        }
        Err(_) => {
            let _ = tcp.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn socks5回复帧构造正确() {
        // 成功回复：VER=5 REP=0 RSV=0 ATYP=1 + 4 字节占位地址 + 2 字节端口
        let ok = [5u8, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        assert_eq!(ok.len(), 10);
        assert_eq!(ok[1], 0);
        let denied = [5u8, 5, 0, 1, 0, 0, 0, 0, 0, 0];
        assert_eq!(denied[1], 5);
    }
}
