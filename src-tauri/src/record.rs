use serde_json::json;
use std::io::Write as IoWrite;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// asciinema v2 格式录制器：头一行 JSON，随后每行一个 [秒, "o", 文本] 事件。
pub struct Recorder {
    file: std::fs::File,
    start: Instant,
}

impl Recorder {
    pub fn start(path: &Path, cols: u32, rows: u32) -> std::io::Result<Recorder> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(path)?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间早于纪元")
            .as_secs();
        let header = json!({
            "version": 2,
            "width": cols,
            "height": rows,
            "timestamp": ts,
            "env": { "TERM": "xterm-256color", "SHELL": "s-shell" }
        });
        writeln!(file, "{header}")?;
        Ok(Recorder { file, start: Instant::now() })
    }

    pub fn output(&mut self, text: &str) {
        let secs = self.start.elapsed().as_secs_f64();
        let event = json!([secs, "o", text]);
        let _ = writeln!(self.file, "{event}");
    }

    pub fn resize(&mut self, cols: u32, rows: u32) {
        let secs = self.start.elapsed().as_secs_f64();
        let event = json!([secs, "r", format!("{cols}x{rows}")]);
        let _ = writeln!(self.file, "{event}");
    }

    pub fn stop(mut self) {
        let _ = self.file.flush();
    }
}

/// 回放：按事件时间差把输出推回终端，单次停顿上限 2 秒（快进长空闲）。
pub async fn replay_file(path: &str, on_data: &tauri::ipc::Channel<tauri::ipc::InvokeResponseBody>) -> Result<(), String> {
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("读取录制文件失败：{e}"))?;
    let mut last: f64 = 0.0;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("录制文件行解析失败：{e}"))?;
        let Some(arr) = value.as_array() else {
            continue; // 头部对象
        };
        if arr.len() < 3 || arr.get(1).and_then(|v| v.as_str()) != Some("o") {
            continue;
        }
        let t = arr[0].as_f64().unwrap_or(0.0);
        let text = arr[2].as_str().unwrap_or("");
        let delta = (t - last).max(0.0).min(2.0);
        tokio::time::sleep(std::time::Duration::from_secs_f64(delta)).await;
        last = t;
        if !on_data
            .send(tauri::ipc::InvokeResponseBody::Raw(text.as_bytes().to_vec()))
            .is_ok()
        {
            return Err("回放通道已关闭".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::replay_file;
    use std::path::PathBuf;

    fn cast_file() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/replay-test/sample.cast")
    }

    #[tokio::test]
    async fn 录制回放还原输出() {
        let path = cast_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "{\"version\":2,\"width\":80,\"height\":24,\"timestamp\":0}\n[0.0,\"o\",\"he\"]\n[0.5,\"o\",\"llo\"]\n",
        )
        .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let channel = tauri::ipc::Channel::new(move |raw| {
            let _ = tx.send(match raw {
                tauri::ipc::InvokeResponseBody::Raw(bytes) => bytes,
                other => panic!("意外载荷：{other:?}"),
            });
            Ok(())
        });
        replay_file(path.to_str().unwrap(), &channel).await.unwrap();
        drop(channel);
        let mut got = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            got.extend_from_slice(&chunk);
        }
        assert_eq!(String::from_utf8_lossy(&got), "hello");
    }
}
