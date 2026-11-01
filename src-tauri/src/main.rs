#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod codec;
mod detect;
mod forward;
mod import_hosts;
mod local;
mod log;
mod masker;
mod mcp;
mod record;
mod sftp;
mod ssh;
mod store;
mod sync;
mod tmux;
mod vault;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use forward::ForwardManager;
use ssh::SessionManager;
use store::Store;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

static SFTP_TASK: AtomicU64 = AtomicU64::new(1);

fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

#[tauri::command]
fn host_list(store: State<Store>) -> Result<Vec<store::Host>, String> {
    store.list()
}

#[tauri::command]
fn host_save(store: State<Store>, id: Option<i64>, host: store::Host) -> Result<i64, String> {
    store.save(id, &host)
}

#[tauri::command]
fn host_delete(
    store: State<Store>,
    sessions: State<'_, SessionManager>,
    id: i64,
) -> Result<(), String> {
    // 先关闭该主机的在线会话，再删数据
    if let Some(sid) = sessions.session_for_host(id) {
        sessions.close(sid)?;
    }
    store.delete(id)?;
    vault::delete_all(id)?;
    Ok(())
}

#[tauri::command]
fn vault_set(store: State<Store>, host_id: i64, field: String, value: String) -> Result<(), String> {
    if host_id != 0 && store.get(host_id)?.is_none() {
        return Err(format!("主机 {host_id} 不存在"));
    }
    vault::set(host_id, &field, &value)
}

#[tauri::command]
fn settings_set(store: State<Store>, key: String, value: String) -> Result<(), String> {
    store.setting_set(&key, &value)
}

#[tauri::command]
fn settings_get(store: State<Store>, key: String) -> Result<Option<String>, String> {
    store.setting_get(&key)
}

#[tauri::command]
fn ai_config_get(store: State<Store>) -> Result<serde_json::Value, String> {
    let has_key = vault::get(0, "aiApiKey")?.is_some();
    Ok(serde_json::json!({
        "baseUrl": store.setting_get("ai_base_url")?.unwrap_or_default(),
        "model": store.setting_get("ai_model")?.unwrap_or_default(),
        "hasKey": has_key
    }))
}

#[tauri::command]
fn forward_list(store: State<Store>, host_id: i64) -> Result<Vec<store::Forward>, String> {
    store.forward_list(host_id)
}

#[tauri::command]
fn forward_save(store: State<Store>, id: Option<i64>, forward: store::Forward) -> Result<i64, String> {
    store.forward_save(id, &forward)
}

#[tauri::command]
fn forward_delete(store: State<Store>, id: i64) -> Result<(), String> {
    store.forward_delete(id)
}

#[tauri::command]
fn forward_list_all(store: State<Store>) -> Result<Vec<store::ForwardRow>, String> {
    store.forward_list_all()
}

/// 启停规则：会话在线立即重绑该主机全部规则，离线时仅落库等待下次连接生效
#[tauri::command]
async fn forward_toggle(
    app: AppHandle,
    store: State<'_, Store>,
    sessions: State<'_, SessionManager>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    store.forward_set_enabled(id, enabled)?;
    let Some(host_id) = store.forward_host_id(id)? else {
        return Ok(());
    };
    let Some(session_id) = sessions.session_for_host(host_id) else {
        return Ok(());
    };
    let handle = sessions.handle_of(session_id)?;
    let rules = store.forward_list(host_id)?;
    let fm = app.state::<ForwardManager>();
    for rule in &rules {
        fm.stop_rule(rule.id);
    }
    for rule in rules.iter().filter(|r| r.enabled) {
        fm.start_rule(&app, session_id, handle.clone(), rule.clone())
            .await
            .ok();
    }
    Ok(())
}

#[tauri::command]
fn forward_active_rules(fm: State<'_, ForwardManager>) -> Result<Vec<i64>, String> {
    Ok(fm.active_rule_ids())
}

/// 本机端口占用探测：true = 可监听
#[tauri::command]
fn check_port_free(_app: AppHandle, port: u16) -> Result<bool, String> {
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// 会话在线时立即按当前规则重启转发；离线时返回 0，规则在下次连接自动生效。
#[tauri::command]
async fn forward_apply(
    app: AppHandle,
    sessions: State<'_, SessionManager>,
    store: State<'_, Store>,
    host_id: i64,
) -> Result<usize, String> {
    let Some(session_id) = sessions.session_for_host(host_id) else {
        return Ok(0);
    };
    let handle = sessions.handle_of(session_id)?;
    let rules = store.forward_list(host_id)?;
    let fm = app.state::<ForwardManager>();
    for rule in &rules {
        fm.stop_rule(rule.id);
    }
    let mut started = 0;
    for rule in rules.iter().filter(|r| r.enabled) {
        if fm.start_rule(&app, session_id, handle.clone(), rule.clone()).await.is_ok() {
            started += 1;
        }
    }
    Ok(started)
}

#[tauri::command]
async fn session_open(
    app: AppHandle,
    sessions: State<'_, SessionManager>,
    store: State<'_, Store>,
    host_id: i64,
    cols: u16,
    rows: u16,
    on_data: Channel<InvokeResponseBody>,
) -> Result<u64, String> {
    ssh::session::open(app, sessions.inner(), store.inner(), host_id, cols, rows, on_data).await
}

#[tauri::command]
async fn session_write(sessions: State<'_, SessionManager>, id: u64, data: Vec<u8>) -> Result<(), String> {
    sessions.write(id, data)
}

#[tauri::command]
async fn session_resize(sessions: State<'_, SessionManager>, id: u64, cols: u16, rows: u16) -> Result<(), String> {
    sessions.resize(id, cols, rows)
}

#[tauri::command]
async fn session_close(sessions: State<'_, SessionManager>, id: u64) -> Result<(), String> {
    sessions.close(id)
}

#[tauri::command]
async fn record_start(
    sessions: State<'_, SessionManager>,
    session_id: u64,
) -> Result<String, String> {
    sessions.handle_of(session_id)?;
    let mut dir = dirs::data_dir().ok_or("无法定位数据目录")?;
    dir.push("s-shell");
    dir.push("recordings");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建录制目录失败：{e}"))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时间早于纪元")
        .as_secs();
    let path = dir.join(format!("session-{ts}.cast"));
    sessions.record_start(session_id, path.to_string_lossy().into_owned())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
async fn record_stop(sessions: State<'_, SessionManager>, session_id: u64) -> Result<(), String> {
    sessions.record_stop(session_id)
}

#[tauri::command]
fn record_list() -> Result<Vec<String>, String> {
    let mut dir = dirs::data_dir().ok_or("无法定位数据目录")?;
    dir.push("s-shell");
    dir.push("recordings");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("cast") {
                out.push(path.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out.reverse();
    Ok(out)
}

#[tauri::command]
async fn replay_open(file: String, on_data: Channel<InvokeResponseBody>) -> Result<(), String> {
    tokio::spawn(async move {
        if let Err(e) = record::replay_file(&file, &on_data).await {
            println!("[s-shell] 回放失败：{e}");
        }
    });
    Ok(())
}

#[tauri::command]
async fn sftp_list(
    sessions: State<'_, SessionManager>,
    session_id: u64,
    path: String,
) -> Result<Vec<sftp::FsEntry>, String> {
    let handle = sessions.handle_of(session_id)?;
    sftp::list(&handle, &path).await
}

#[tauri::command]
async fn sftp_mkdir(
    sessions: State<'_, SessionManager>,
    session_id: u64,
    path: String,
) -> Result<(), String> {
    let handle = sessions.handle_of(session_id)?;
    sftp::mkdir(&handle, &path).await
}

#[tauri::command]
async fn sftp_remove(
    sessions: State<'_, SessionManager>,
    session_id: u64,
    path: String,
    is_dir: bool,
    recursive: Option<bool>,
) -> Result<(), String> {
    let handle = sessions.handle_of(session_id)?;
    sftp::remove(&handle, &path, is_dir, recursive.unwrap_or(false)).await
}

#[tauri::command]
async fn sftp_rename(
    sessions: State<'_, SessionManager>,
    session_id: u64,
    from: String,
    to: String,
) -> Result<(), String> {
    let handle = sessions.handle_of(session_id)?;
    sftp::rename(&handle, &from, &to).await
}

#[tauri::command]
async fn sftp_upload(
    app: AppHandle,
    sessions: State<'_, SessionManager>,
    session_id: u64,
    local_path: String,
    remote_path: String,
    resume: Option<bool>,
    limit_kbps: Option<u64>,
) -> Result<u64, String> {
    let handle = sessions.handle_of(session_id)?;
    let task_id = SFTP_TASK.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        sftp::run_transfer_task(
            handle,
            sftp::TransferKind::Upload {
                local: local_path,
                remote: remote_path,
                resume: resume.unwrap_or(false),
                limit_kbps: limit_kbps.unwrap_or(0),
            },
            app,
            task_id,
        )
        .await;
    });
    Ok(task_id)
}

#[tauri::command]
async fn sftp_download(
    app: AppHandle,
    sessions: State<'_, SessionManager>,
    session_id: u64,
    remote_path: String,
    local_path: String,
    resume: Option<bool>,
    limit_kbps: Option<u64>,
) -> Result<u64, String> {
    let handle = sessions.handle_of(session_id)?;
    let task_id = SFTP_TASK.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        sftp::run_transfer_task(
            handle,
            sftp::TransferKind::Download {
                remote: remote_path,
                local: local_path,
                resume: resume.unwrap_or(false),
                limit_kbps: limit_kbps.unwrap_or(0),
            },
            app,
            task_id,
        )
        .await;
    });
    Ok(task_id)
}

#[tauri::command]
async fn sync_push(store: State<'_, Store>) -> Result<String, String> {
    sync::push(store.inner()).await
}

#[tauri::command]
async fn sync_pull(store: State<'_, Store>) -> Result<String, String> {
    sync::pull(store.inner()).await
}

#[tauri::command]
fn mcp_pending_list(store: State<Store>) -> Result<Vec<store::McpRequest>, String> {
    store.mcp_pending_list()
}

#[tauri::command]
fn mcp_decide(store: State<Store>, id: i64, approved: bool) -> Result<(), String> {
    store.mcp_request_decide(id, approved)
}

#[tauri::command]
async fn ai_ask(store: State<'_, Store>, question: String, context: String) -> Result<String, String> {
    ai::ask(store.inner(), &question, &context).await
}

#[tauri::command]
fn zmodem_save(name: String, data_base64: String) -> Result<String, String> {
    use base64::Engine;
    let safe: String = name
        .split(['/', '\\'])
        .last()
        .filter(|s| !s.is_empty())
        .unwrap_or("file")
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("zmodem 数据解码失败：{e}"))?;
    let mut dir = dirs::data_dir().ok_or("无法定位数据目录")?;
    dir.push("s-shell");
    dir.push("zmodem");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建接收目录失败：{e}"))?;
    let path = dir.join(&safe);
    std::fs::write(&path, &bytes).map_err(|e| format!("写入接收文件失败：{e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn log_tail(lines: u32) -> Result<Vec<String>, String> {
    Ok(log::tail(lines as usize))
}

#[tauri::command]
async fn perf_stream(
    on_data: Channel<InvokeResponseBody>,
    total_mb: Option<u32>,
) -> Result<String, String> {
    let mb = total_mb.unwrap_or(8).max(1);
    let chunk = vec![0u8; 64 * 1024];
    let target = (mb as u64) * 1024 * 1024;
    let start = std::time::Instant::now();
    let mut sent: u64 = 0;
    let mut iter: u64 = 0;
    while sent < target {
        let mut out = Vec::with_capacity(chunk.len() + 1);
        out.push(0u8);
        out.extend_from_slice(&chunk);
        if on_data.send(InvokeResponseBody::Raw(out)).is_err() {
            return Err("前端通道关闭".into());
        }
        sent += (chunk.len() + 1) as u64;
        iter += 1;
        if iter % 32 == 0 {
            tokio::time::sleep(Duration::from_micros(300)).await;
        }
    }
    let elapsed = start.elapsed();
    let mibs = (sent as f64 / 1_048_576.0) / elapsed.as_secs_f64();
    let report = format!(
        "内核发送 {}MB 耗时 {:?}（{:.0} MiB/s）",
        mb,
        elapsed,
        mibs
    );
    log::event("perf", &report);
    Ok(report)
}

#[tauri::command]
fn perf_report(text: String) -> Result<(), String> {
    log::event("perf", &text);
    Ok(())
}

#[tauri::command]
fn perf_ping() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 通道突发回放：以会话同款节奏推送 300 块带序号的标记数据，
/// 前端对比「内核发送耗时」与「前端接收跨度」，差值即通道投递延迟。
#[tauri::command]
async fn perf_replay_burst(on_data: Channel<InvokeResponseBody>) -> Result<String, String> {
    let start = std::time::Instant::now();
    for seq in 0..300u64 {
        let mut out = Vec::with_capacity(8 * 1024 + 17);
        out.push(0u8);
        out.extend_from_slice(&seq.to_be_bytes());
        out.extend_from_slice(&(start.elapsed().as_micros() as u64).to_be_bytes());
        out.resize(8 * 1024 + 17, 0x61);
        if on_data.send(InvokeResponseBody::Raw(out)).is_err() {
            return Err("前端通道关闭".into());
        }
        if seq % 8 == 7 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    let elapsed = start.elapsed();
    let report = format!(
        "回放突发：内核发送 300 块耗时 {:?}（约 {} MiB/s）",
        elapsed,
        (300.0 * 8.0 / 1024.0) / elapsed.as_secs_f64()
    );
    log::event("perf", &report);
    Ok(report)
}

#[tauri::command]
fn known_hosts_list(store: State<Store>) -> Result<Vec<store::KnownHostRow>, String> {
    store.known_hosts_list()
}

#[tauri::command]
fn known_hosts_delete(store: State<Store>, host: String, port: u16) -> Result<(), String> {
    store.known_hosts_delete(&host, port)
}

#[tauri::command]
fn snippet_list(store: State<Store>) -> Result<Vec<store::Snippet>, String> {
    store.snippet_list()
}

#[tauri::command]
fn snippet_save(store: State<Store>, id: Option<i64>, snippet: store::Snippet) -> Result<i64, String> {
    store.snippet_save(id, &snippet)
}

#[tauri::command]
fn snippet_delete(store: State<Store>, id: i64) -> Result<(), String> {
    store.snippet_delete(id)
}

#[tauri::command]
fn trigger_list(store: State<Store>) -> Result<Vec<store::Trigger>, String> {
    store.trigger_list()
}

#[tauri::command]
fn trigger_save(store: State<Store>, id: Option<i64>, trigger: store::Trigger) -> Result<i64, String> {
    store.trigger_save(id, &trigger)
}

#[tauri::command]
fn trigger_delete(store: State<Store>, id: i64) -> Result<(), String> {
    store.trigger_delete(id)
}

#[tauri::command]
fn set_window_theme(window: tauri::WebviewWindow, dark: bool) -> Result<(), String> {
    window
        .set_theme(Some(if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }))
        .map_err(|e| e.to_string())
}

/// 会话日志：开启后终端输出逐字落盘到数据目录 session-logs/
#[tauri::command]
fn session_log_start(
    sessions: State<'_, SessionManager>,
    store: State<'_, Store>,
    session_id: u64,
) -> Result<String, String> {
    let host_id = session_host_id(sessions.inner(), session_id)?;
    let host_name = store.get(host_id)?.ok_or("主机不存在")?.name;
    let mut dir = dirs::data_dir().ok_or("无法定位数据目录")?;
    dir.push("s-shell");
    dir.push("session-logs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败：{e}"))?;
    let safe = host_name.replace(['/', '\\', ':', ' '], "_");
    let mut path = dir;
    path.push(format!("{}-{safe}.log", chrono_like_stamp()));
    let path_str = path.to_string_lossy().to_string();
    sessions.log_start(session_id, path_str.clone())?;
    Ok(path_str)
}

#[tauri::command]
fn session_log_stop(sessions: State<'_, SessionManager>, session_id: u64) -> Result<(), String> {
    sessions.log_stop(session_id)
}

fn session_host_id(sessions: &SessionManager, session_id: u64) -> Result<i64, String> {
    sessions.host_id_of_session(session_id)
}

fn chrono_like_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时间早于纪元")
        .as_secs();
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // 平赖恩日期算法：纪元天数转公历
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = doe / 365;
    let doy = doe % 365;
    let y = yoe + era * 400;
    let leap = (doy == 60) as i64;
    let mp = (doy + leap) * 5 - 3;
    let month = mp / 153;
    let day = (doy - (153 * mp + 2) / 5) + 1;
    let year = y + (month <= 1) as i64;
    format!("{year:04}{month:02}{day:02}-{h:02}{m:02}{s:02}")
}

#[tauri::command]
fn vault_status() -> serde_json::Value {    serde_json::json!({
        "mode": if vault::mode() == vault::Mode::Master { "master" } else { "keyring" },
        "unlocked": vault::master_unlocked()
    })
}

#[tauri::command]
fn vault_master_unlock(password: String) -> Result<(), String> {
    vault::master_unlock(&password)
}

#[tauri::command]
async fn local_list(path: String) -> Result<Vec<sftp::FsEntry>, String> {
    sftp::local_list(&path).await
}

#[tauri::command]
async fn local_mkdir(path: String) -> Result<(), String> {
    sftp::local_mkdir(&path).await
}

#[tauri::command]
async fn local_remove(path: String, is_dir: bool) -> Result<(), String> {
    sftp::local_remove(&path, is_dir).await
}

#[tauri::command]
fn import_hosts(store: State<Store>, kind: String, path: String) -> Result<usize, String> {    let parsed = match kind.as_str() {
        "ssh_config" => import_hosts::parse_ssh_config(&path)?,
        "xshell" => import_hosts::parse_xshell_dir(&path)?,
        "termius" => import_hosts::parse_termius_json(&path)?,
        other => return Err(format!("未知导入类型：{other}")),
    };
    let mut count = 0usize;
    for item in &parsed {
        let host = store::Host {
            id: 0,
            name: item.name.clone(),
            host: item.host.clone(),
            port: item.port,
            username: item.username.clone(),
            auth_method: if item.key_path.is_some() { "key".into() } else { "password".into() },
            key_path: item.key_path.clone(),
            encoding: "utf8".into(),
            compat_mode: false,
            tmux_policy: "none".into(),
            jump_host_id: None,
            cert_path: None,
            shell_integration: false,
            proxy_kind: "none".into(),
            proxy_host: None,
            proxy_port: None,
            proxy_command: None,
            compress: false,
            updated_at: String::new(),
        };
        if store.save(None, &host).is_ok() {
            count += 1;
        }
    }
    log::event("import", &format!("来源 {kind}:{path}，导入 {count} 台"));
    Ok(count)
}

#[tauri::command]
fn ai_check_command(command: String) -> Vec<String> {
    ai::check_command(&command)
}

#[tauri::command]
fn ai_preview(context: String) -> Result<String, String> {
    Ok(ai::preview(&context))
}

#[tauri::command]
fn session_context(
    sessions: State<SessionManager>,
    session_id: u64,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "cwd": sessions.cwd_of(session_id) }))
}

#[tauri::command]
async fn batch_exec(
    store: State<'_, Store>,
    host_ids: Vec<i64>,
    command: String,
) -> Result<Vec<serde_json::Value>, String> {
    use futures::StreamExt;
    let store_ref = store.inner();
    let tasks = host_ids.into_iter().map(|host_id| {
        let command = command.clone();
        async move {
        let host = match store_ref.get(host_id) {
            Ok(Some(h)) => h,
            Ok(None) => {
                return serde_json::json!({ "hostId": host_id, "error": "主机不存在" });
            }
            Err(e) => {
                return serde_json::json!({ "hostId": host_id, "error": e });
            }
        };
        let name = host.name.clone();
        match ssh::session::exec_command(store_ref, &host, &command, Duration::from_secs(30)).await {
            Ok(output) => serde_json::json!({ "hostId": host_id, "name": name, "output": output }),
            Err(e) => serde_json::json!({ "hostId": host_id, "name": name, "error": e }),
        }
        }
    });
    let results: Vec<serde_json::Value> =
        futures::stream::iter(tasks).buffer_unordered(4).collect().await;
    Ok(results)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "mcp" {
        // MCP 模式：stdio JSON-RPC，不初始化界面与 WebView
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("无法创建异步运行时")
            .block_on(async {
                if let Err(e) = mcp::run().await {
                    eprintln!("s-shell mcp 异常退出：{e}");
                    std::process::exit(1);
                }
            });
        return;
    }

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["CmdOrCtrl+Shift+S"])
                .expect("注册全局快捷键失败")
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .manage(Store::open())
        .manage(SessionManager::new())
        .manage(local::LocalManager::default())
        .manage(ForwardManager::new())
        .setup(|app| {
            // 顶栏菜单中文化：文件/编辑/显示/窗口/帮助
            use tauri::menu::PredefinedMenuItem;
            let file_menu = tauri::menu::Submenu::with_items(
                app,
                "文件",
                true,
                &[&PredefinedMenuItem::close_window(app, Some("关闭窗口"))?],
            )?;
            let edit_menu = tauri::menu::Submenu::with_items(
                app,
                "编辑",
                true,
                &[
                    &PredefinedMenuItem::undo(app, Some("撤销"))?,
                    &PredefinedMenuItem::redo(app, Some("重做"))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, Some("剪切"))?,
                    &PredefinedMenuItem::copy(app, Some("拷贝"))?,
                    &PredefinedMenuItem::paste(app, Some("粘贴"))?,
                    &PredefinedMenuItem::select_all(app, Some("全选"))?,
                ],
            )?;
            let view_menu = tauri::menu::Submenu::with_items(
                app,
                "显示",
                true,
                &[&PredefinedMenuItem::fullscreen(app, Some("切换全屏"))?],
            )?;
            let window_menu = tauri::menu::Submenu::with_items(
                app,
                "窗口",
                true,
                &[
                    &PredefinedMenuItem::minimize(app, Some("最小化"))?,
                    &PredefinedMenuItem::close_window(app, Some("关闭窗口"))?,
                ],
            )?;
            let settings_item = MenuItem::with_id(
                app,
                "open-settings",
                "设置…",
                true,
                Some("CmdOrCtrl+,"),
            )?;
            let help_menu =
                tauri::menu::Submenu::with_items(app, "帮助", true, &[&settings_item])?;
            let menubar = Menu::with_items(
                app,
                &[&file_menu, &edit_menu, &view_menu, &window_menu, &help_menu],
            )?;
            app.set_menu(menubar)?;
            app.on_menu_event(|app, event| {
                if event.id().as_ref() == "open-settings" {
                    let _ = app.emit("open-settings", ());
                }
            });

            // 菜单栏常驻：托盘提供显示切换与真正退出
            let toggle = MenuItem::with_id(
                app,
                "toggle",
                "显示 / 隐藏（Ctrl/Cmd+Shift+S）",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "退出 s-shell", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().expect("缺省图标缺失").clone())
                .tooltip("s-shell")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "toggle" => toggle_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 常驻语义：点关闭等于隐藏，真正退出走托盘菜单
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            host_list,
            host_save,
            host_delete,
            vault_set,
            settings_set,
            settings_get,
            ai_config_get,
            forward_list,
            forward_save,
            forward_delete,
            forward_apply,
            session_open,
            session_write,
            session_resize,
            session_close,
            record_start,
            record_stop,
            record_list,
            replay_open,
            sftp_list,
            sftp_mkdir,
            sftp_remove,
            sftp_rename,
            sftp_upload,
            sftp_download,
            local_list,
            local_mkdir,
            local_remove,
            vault_status,
            set_window_theme,
            session_log_start,
            session_log_stop,
            forward_list_all,
            forward_toggle,
            forward_active_rules,
            check_port_free,
            vault_master_unlock,
            local::local_open,
            local::local_write,
            local::local_resize,
            local::local_close,
            known_hosts_list,
            known_hosts_delete,
            snippet_list,
            trigger_list,
            trigger_save,
            trigger_delete,
            snippet_save,
            snippet_delete,
            sync_push,
            sync_pull,
            mcp_pending_list,
            mcp_decide,
            ai_ask,
            ai_check_command,
            ai_preview,
            session_context,
            batch_exec,
            zmodem_save,
            log_tail,
            import_hosts,
            perf_stream,
            perf_report,
            perf_ping,
            perf_replay_burst
        ])
        .run(tauri::generate_context!())
        .expect("s-shell 启动失败");
}
