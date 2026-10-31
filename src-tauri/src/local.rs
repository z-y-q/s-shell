use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, State};

pub struct LocalEntry {
    pub writer: Box<dyn std::io::Write + Send>,
    pub master: Box<dyn MasterPty + Send>,
}

#[derive(Default)]
pub struct LocalManager(pub Mutex<HashMap<u64, LocalEntry>>);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[tauri::command]
pub fn local_open(
    app: AppHandle,
    locals: State<'_, LocalManager>,
    cols: u16,
    rows: u16,
    on_data: Channel<InvokeResponseBody>,
) -> Result<u64, String> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("创建本地终端失败：{e}"))?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "cmd.exe".to_string()
        } else if cfg!(target_os = "macos") {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    });
    let mut cmd = CommandBuilder::new(&shell);
    if let Some(home) = dirs::home_dir() {
        cmd.cwd(home);
    }
    cmd.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| format!("启动 shell 失败：{e}"))?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| format!("{e}"))?;
    let writer = pair.master.take_writer().map_err(|e| format!("{e}"))?;
    let master = pair.master;

    // 输出泵：PTY -> 前端通道（带标签字节 0，与 SSH 会话同一协议）
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut payload = Vec::with_capacity(n + 1);
                    payload.push(0u8);
                    payload.extend_from_slice(&buf[..n]);
                    if on_data.send(InvokeResponseBody::Raw(payload)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = child.wait();
        let _ = app.emit("session-closed", id);
    });

    locals
        .0
        .lock()
        .expect("本地会话表锁中毒")
        .insert(id, LocalEntry { writer, master });
    Ok(id)
}

#[tauri::command]
pub fn local_write(locals: State<'_, LocalManager>, id: u64, data: Vec<u8>) -> Result<(), String> {
    let mut table = locals.0.lock().expect("本地会话表锁中毒");
    let entry = table.get_mut(&id).ok_or("本地终端不存在")?;
    entry.writer.write_all(&data).map_err(|e| format!("写入失败：{e}"))
}

#[tauri::command]
pub fn local_resize(locals: State<'_, LocalManager>, id: u64, cols: u16, rows: u16) -> Result<(), String> {
    let table = locals.0.lock().expect("本地会话表锁中毒");
    let entry = table.get(&id).ok_or("本地终端不存在")?;
    entry
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("调整尺寸失败：{e}"))
}

#[tauri::command]
pub fn local_close(locals: State<'_, LocalManager>, id: u64) -> Result<(), String> {
    let mut table = locals.0.lock().expect("本地会话表锁中毒");
    // 移除即释放 master：从端挂断，shell 进程随之退出
    table.remove(&id);
    Ok(())
}
