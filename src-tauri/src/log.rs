use std::io::Write as IoWrite;
use std::path::PathBuf;

/// 连接事件日志：写本地文件，内容经 masker::mask 脱敏（与 AI 共用同一套规则）。
pub fn event(kind: &str, detail: &str) {
    let Some(path) = log_file() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = format!("{} [{kind}] {}", human_time(secs), crate::masker::mask(detail));
        let _ = writeln!(file, "{line}");
    }
}

pub fn tail(n: usize) -> Vec<String> {
    let Some(path) = log_file() else { return Vec::new() };
    let Ok(content) = std::fs::read_to_string(&path) else { return Vec::new() };
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].to_vec()
}

fn log_file() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("s-shell").join("logs").join("s-shell.log"))
}

/// 无 chrono 依赖的 UTC 可读时间（儒略日转日期，标准 civil_from_days 算法）。
fn human_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::{event, human_time, tail};

    #[test]
    fn 儒略日转时间正确() {
        // 2026-10-28 00:00:00 UTC = 1793145600
        assert_eq!(human_time(1_793_145_600), "2026-10-28 00:00:00");
        assert_eq!(human_time(0), "1970-01-01 00:00:00");
        // 2026-10-28 14:38:33 UTC（对应本地 Asia/Shanghai 22:38:33）
        assert_eq!(human_time(1_793_198_313), "2026-10-28 14:38:33");
    }

    #[test]
    fn 事件写入并脱敏读取() {
        event("test", "connect 10.0.0.9 password: secret123");
        let lines = tail(1);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("[test]"));
        assert!(lines[0].contains("[IP]"));
        assert!(!lines[0].contains("secret123"));
    }
}
