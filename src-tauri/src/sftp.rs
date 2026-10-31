use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::ssh::session::ClientHandler;

const CHUNK: usize = 256 * 1024;

pub async fn open_sftp(handle: &Handle<ClientHandler>) -> Result<SftpSession, String> {
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 SFTP 通道失败：{e}"))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("请求 sftp 子系统失败：{e}"))?;
    SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("SFTP 初始化失败：{e}"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
}

fn sort_entries(out: &mut Vec<FsEntry>) {
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

pub async fn list(handle: &Handle<ClientHandler>, path: &str) -> Result<Vec<FsEntry>, String> {
    let sftp = open_sftp(handle).await?;
    let mut read_dir = sftp
        .read_dir(path)
        .await
        .map_err(|e| format!("读取目录失败：{e}"))?;
    let mut out = Vec::new();
    while let Some(entry) = read_dir.next() {
        let meta = entry.metadata();
        out.push(FsEntry {
            name: entry.file_name().to_string(),
            is_dir: meta.is_dir(),
            size: meta.size.unwrap_or(0),
            mtime: meta.mtime.map(|v| v as i64).unwrap_or(0),
        });
    }
    sort_entries(&mut out);
    Ok(out)
}

pub async fn mkdir(handle: &Handle<ClientHandler>, path: &str) -> Result<(), String> {
    open_sftp(handle)
        .await?
        .create_dir(path)
        .await
        .map_err(|e| format!("创建目录失败：{e}"))
}

pub async fn remove(
    handle: &Handle<ClientHandler>,
    path: &str,
    is_dir: bool,
    recursive: bool,
) -> Result<(), String> {
    let sftp = open_sftp(handle).await?;
    if is_dir {
        if recursive {
            remove_dir_recursive(&sftp, path).await?;
            Ok(())
        } else {
            sftp.remove_dir(path)
                .await
                .map_err(|e| format!("删除目录失败：{e}"))
        }
    } else {
        sftp.remove_file(path)
            .await
            .map_err(|e| format!("删除失败：{e}"))
    }
}

/// 递归删除远端目录：先删空文件，自底向上删子目录
async fn remove_dir_recursive(sftp: &SftpSession, path: &str) -> Result<(), String> {
    let mut read_dir = sftp
        .read_dir(path)
        .await
        .map_err(|e| format!("读取目录失败：{e}"))?;
    let mut subdirs = Vec::new();
    while let Some(entry) = read_dir.next() {
        let meta = entry.metadata();
        let name = entry.file_name().to_string();
        if name == "." || name == ".." {
            continue;
        }
        let child = format!("{path}/{name}");
        if meta.is_dir() {
            subdirs.push(child);
        } else {
            sftp.remove_file(&child)
                .await
                .map_err(|e| format!("删除 {child} 失败：{e}"))?;
        }
    }
    for sub in subdirs {
        Box::pin(remove_dir_recursive(sftp, &sub)).await?;
    }
    sftp.remove_dir(path)
        .await
        .map_err(|e| format!("删除目录失败：{e}"))
}

pub async fn rename(handle: &Handle<ClientHandler>, from: &str, to: &str) -> Result<(), String> {
    open_sftp(handle)
        .await?
        .rename(from, to)
        .await
        .map_err(|e| format!("重命名失败：{e}"))
}

// ---- 本地文件系统（双栏的本地侧）----

pub async fn local_list(path: &str) -> Result<Vec<FsEntry>, String> {
    let mut rd = tokio::fs::read_dir(path)
        .await
        .map_err(|e| format!("读取本地目录失败：{e}"))?;
    let mut out = Vec::new();
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| format!("遍历本地目录失败：{e}"))?
    {
        let meta = entry
            .metadata()
            .await
            .map_err(|e| format!("读取本地信息失败：{e}"))?;
        out.push(FsEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            size: meta.len(),
            mtime: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        });
    }
    sort_entries(&mut out);
    Ok(out)
}

pub async fn local_mkdir(path: &str) -> Result<(), String> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|e| format!("创建本地目录失败：{e}"))
}

pub async fn local_remove(path: &str, is_dir: bool) -> Result<(), String> {
    if is_dir {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|e| format!("删除本地目录失败：{e}"))
    } else {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| format!("删除本地文件失败：{e}"))
    }
}

// ---- 进度上报：内部可变性，克隆后可跨并发任务共享 ----

#[derive(Clone)]
struct Progress {
    app: Option<AppHandle>,
    task_id: u64,
    total: u64,
    started: Instant,
    state: Arc<Mutex<(u64, u64, u64)>>, // (done, last_emitted, base)
}

impl Progress {
    fn new(app: Option<AppHandle>, task_id: u64, total: u64) -> Progress {
        Progress {
            app,
            task_id,
            total,
            started: Instant::now(),
            state: Arc::new(Mutex::new((0, 0, 0))),
        }
    }

    fn set_base(&self, base: u64) {
        let mut s = self.state.lock().expect("进度锁中毒");
        s.2 = base;
        s.0 = 0;
        s.1 = 0;
    }

    fn transferred(&self) -> u64 {
        let s = self.state.lock().expect("进度锁中毒");
        s.2 + s.0
    }

    fn elapsed_secs(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn add(&self, n: u64) {
        let mut s = self.state.lock().expect("进度锁中毒");
        s.0 += n;
        if s.0 - s.1 >= CHUNK as u64 || s.2 + s.0 >= self.total {
            s.1 = s.0;
            if let Some(app) = &self.app {
                let _ = app.emit(
                    "sftp-progress",
                    serde_json::json!({ "taskId": self.task_id, "done": s.2 + s.0, "total": self.total }),
                );
            }
        }
    }
}

fn emit_done(app: &AppHandle, task_id: u64, error: Option<String>) {
    let _ = app.emit(
        "sftp-done",
        match error {
            None => serde_json::json!({ "taskId": task_id, "ok": true }),
            Some(e) => serde_json::json!({ "taskId": task_id, "ok": false, "error": e }),
        },
    );
}

/// 限速：按累计传输量与已用时间计算需要等待的时长。
async fn throttle(progress: &Progress, limit_kbps: u64) {
    if limit_kbps == 0 {
        return;
    }
    let rate = (limit_kbps as f64) * 1024.0;
    let target_secs = (progress.transferred() as f64) / rate;
    let wait = target_secs - progress.elapsed_secs();
    if wait > 0.001 {
        tokio::time::sleep(Duration::from_secs_f64(wait)).await;
    }
}

/// 入口：本地是文件就单发，是目录就递归上传，4 路并发；resume 启用断点续传。
pub async fn transfer_upload(
    handle: &Handle<ClientHandler>,
    local: &str,
    remote: &str,
    app: Option<AppHandle>,
    task_id: u64,
    resume: bool,
    limit_kbps: u64,
) -> Result<(), String> {
    let local_path = PathBuf::from(local);
    let meta = tokio::fs::metadata(&local_path)
        .await
        .map_err(|e| format!("读取本地路径失败：{e}"))?;
    let sftp = Arc::new(open_sftp(handle).await?);

    if meta.is_file() {
        let progress = Progress::new(app, task_id, meta.len());
        copy_local_to_remote(&sftp, &local_path, remote, &progress, resume, limit_kbps).await?;
        return Ok(());
    }

    let mut files = Vec::new();
    walk_local(&local_path, &local_path, &mut files).await?;
    let total: u64 = files.iter().map(|(_, _, size)| *size).sum();
    let progress = Progress::new(app, task_id, total);

    let mut dirs = std::collections::BTreeSet::new();
    for (_, rel, _) in &files {
        dirs.insert(remote_parent_of(remote, rel));
    }
    for dir in &dirs {
        // mkdir -p 语义：已存在视为成功，真正的失败会在文件写入时暴露
        let _ = sftp.create_dir(dir).await;
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = Vec::new();
    for (local_file, rel, _) in files {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let sftp = sftp.clone();
        let progress = progress.clone();
        let remote_parent = remote_parent_of(remote, &rel);
        let file_name = local_file
            .file_name()
            .expect("文件名缺失")
            .to_string_lossy()
            .into_owned();
        let remote_file = format!("{remote_parent}/{file_name}");
        tasks.push(tokio::spawn(async move {
            let _permit = permit;
            let _ = sftp.create_dir(&remote_parent).await;
            copy_local_to_remote(&sftp, &local_file, &remote_file, &progress, resume, limit_kbps).await
        }));
    }
    for task in tasks {
        task.await
            .map_err(|e| format!("上传任务崩溃：{e}"))?
            .map_err(|e| format!("上传失败：{e}"))?;
    }
    Ok(())
}

fn remote_parent_of(remote_root: &str, rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((parent, _)) => format!("{remote_root}/{parent}"),
        None => remote_root.to_string(),
    }
}

async fn walk_local(
    dir: &Path,
    base: &Path,
    out: &mut Vec<(PathBuf, String, u64)>,
) -> Result<(), String> {
    let mut rd = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("读取本地目录失败：{e}"))?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| format!("遍历本地目录失败：{e}"))?
    {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(walk_local(&path, base, out)).await?;
        } else {
            let size = entry
                .metadata()
                .await
                .map_err(|e| format!("读取文件大小失败：{e}"))?
                .len();
            let rel = path
                .strip_prefix(base)
                .expect("遍历路径必然位于基目录之下")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((path, rel, size));
        }
    }
    Ok(())
}

async fn copy_local_to_remote(
    sftp: &SftpSession,
    local: &Path,
    remote: &str,
    progress: &Progress,
    resume: bool,
    limit_kbps: u64,
) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
    let mut lf = tokio::fs::File::open(local)
        .await
        .map_err(|e| format!("打开本地文件失败：{e}"))?;
    let mut start = 0u64;
    if resume {
        if let Ok(meta) = sftp.metadata(remote).await {
            start = meta.size.unwrap_or(0);
            lf.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|e| format!("本地断点定位失败：{e}"))?;
        }
    }
    let mut rf = if resume {
        sftp.open_with_flags(remote, OpenFlags::WRITE | OpenFlags::CREATE)
            .await
            .map_err(|e| format!("打开远端文件失败：{e}"))?
    } else {
        sftp.create(remote).await.map_err(|e| format!("打开远端文件失败：{e}"))?
    };
    if resume && start > 0 {
        rf.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| format!("远端断点定位失败：{e}"))?;
        progress.set_base(start);
    }
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = lf
            .read(&mut buf)
            .await
            .map_err(|e| format!("读取本地文件失败：{e}"))?;
        if n == 0 {
            break;
        }
        rf.write_all(&buf[..n])
            .await
            .map_err(|e| format!("写入远端文件失败：{e}"))?;
        progress.add(n as u64);
        throttle(progress, limit_kbps).await;
    }
    rf.sync_all().await.ok();
    Ok(())
}

pub async fn transfer_download(
    handle: &Handle<ClientHandler>,
    remote: &str,
    local: &str,
    app: Option<AppHandle>,
    task_id: u64,
    resume: bool,
    limit_kbps: u64,
) -> Result<(), String> {
    let sftp = Arc::new(open_sftp(handle).await?);
    let meta = sftp
        .metadata(remote)
        .await
        .map_err(|e| format!("读取远端信息失败：{e}"))?;
    let local_path = PathBuf::from(local);

    if !meta.is_dir() {
        let progress = Progress::new(app, task_id, meta.size.unwrap_or(0));
        copy_remote_to_local(&sftp, remote, &local_path, &progress, resume, limit_kbps).await?;
        return Ok(());
    }

    let mut files = Vec::new();
    walk_remote(&sftp, remote, &local_path, &mut files).await?;
    let total: u64 = files.iter().map(|(_, _, size)| *size).sum();
    let progress = Progress::new(app, task_id, total);
    for (remote_file, local_file, _) in files {
        copy_remote_to_local(&sftp, &remote_file, &local_file, &progress, resume, limit_kbps).await?;
    }
    Ok(())
}

async fn walk_remote(
    sftp: &SftpSession,
    remote_dir: &str,
    local_base: &Path,
    out: &mut Vec<(String, PathBuf, u64)>,
) -> Result<(), String> {
    let mut read_dir = sftp
        .read_dir(remote_dir)
        .await
        .map_err(|e| format!("读取远端目录失败：{e}"))?;
    while let Some(entry) = read_dir.next() {
        let meta = entry.metadata();
        let name = entry.file_name().to_string();
        if name == "." || name == ".." {
            continue;
        }
        let remote_path = format!("{remote_dir}/{name}");
        if meta.is_dir() {
            Box::pin(walk_remote(sftp, &remote_path, local_base, out)).await?;
        } else {
            let flat = remote_dir.trim_start_matches('/').replace('/', "_");
            let local_file = local_base.join(flat).join(&name);
            out.push((remote_path, local_file, meta.size.unwrap_or(0)));
        }
    }
    Ok(())
}

async fn copy_remote_to_local(
    sftp: &SftpSession,
    remote: &str,
    local: &Path,
    progress: &Progress,
    resume: bool,
    limit_kbps: u64,
) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
    if let Some(parent) = local.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("创建本地目录失败：{e}"))?;
    }
    let mut rf = sftp
        .open(remote)
        .await
        .map_err(|e| format!("打开远端文件失败：{e}"))?;
    let mut start = 0u64;
    let mut lf = if resume && local.exists() {
        start = tokio::fs::metadata(local)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        tokio::fs::OpenOptions::new()
            .write(true)
            .open(local)
            .await
            .map_err(|e| format!("打开本地文件失败：{e}"))?
    } else {
        tokio::fs::File::create(local)
            .await
            .map_err(|e| format!("创建本地文件失败：{e}"))?
    };
    if resume && start > 0 {
        lf.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| format!("本地断点定位失败：{e}"))?;
        rf.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| format!("远端断点定位失败：{e}"))?;
        progress.set_base(start);
    }
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = rf
            .read(&mut buf)
            .await
            .map_err(|e| format!("读取远端文件失败：{e}"))?;
        if n == 0 {
            break;
        }
        lf.write_all(&buf[..n])
            .await
            .map_err(|e| format!("写入本地文件失败：{e}"))?;
        progress.add(n as u64);
        throttle(progress, limit_kbps).await;
    }
    lf.sync_all().await.ok();
    Ok(())
}

/// 供命令层使用：包装一次性任务的完成事件。
pub enum TransferKind {
    Upload { local: String, remote: String, resume: bool, limit_kbps: u64 },
    Download { remote: String, local: String, resume: bool, limit_kbps: u64 },
}

pub async fn run_transfer_task(
    handle: Arc<Handle<ClientHandler>>,
    kind: TransferKind,
    app: AppHandle,
    task_id: u64,
) {
    let result = match kind {
        TransferKind::Upload { local, remote, resume, limit_kbps } => {
            transfer_upload(&handle, &local, &remote, Some(app.clone()), task_id, resume, limit_kbps)
                .await
        }
        TransferKind::Download { remote, local, resume, limit_kbps } => {
            transfer_download(&handle, &remote, &local, Some(app.clone()), task_id, resume, limit_kbps)
                .await
        }
    };
    emit_done(&app, task_id, result.err());
}
