pub enum TmuxPolicy {
    None,
    Attach,
    Wrap,
}

pub fn parse(name: &str) -> TmuxPolicy {
    match name {
        "attach" => TmuxPolicy::Attach,
        "wrap" => TmuxPolicy::Wrap,
        _ => TmuxPolicy::None,
    }
}

/// 外壳建立后立即注入的 tmux 命令。
/// 断线重连只恢复连接本身；进程现场由服务器端 tmux 保管，这是方案明确的语义。
pub fn launch_command(policy: TmuxPolicy) -> Option<&'static str> {
    match policy {
        TmuxPolicy::None => None,
        TmuxPolicy::Wrap => Some("tmux new -A -s s-shell\r"),
        TmuxPolicy::Attach => Some("tmux attach -t s-shell 2>/dev/null || tmux new -A -s s-shell\r"),
    }
}

#[cfg(test)]
mod tests {
    use super::{launch_command, parse};

    #[test]
    fn 策略解析() {
        assert!(launch_command(parse("none")).is_none());
        assert!(launch_command(parse("wrap")).is_some());
        assert!(launch_command(parse("attach")).is_some());
        assert!(launch_command(parse("未知值")).is_none());
    }
}
