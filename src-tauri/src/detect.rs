use std::time::{Duration, Instant};

const ZRQINIT_PREFIX: &[u8] = b"**\x18"; // ZPAD ZPAD ZDLE，rz 启动序列的开头
const IDLE_TIMEOUT: Duration = Duration::from_secs(2);

/// zmodem 会话检测：一旦识别到 rz 启动序列，后续字节全部透传（零延迟、绕过编码与聚合），
/// 静默超过 2 秒视为传输结束，回到普通终端模式。
pub struct ZmodemDetector {
    active: bool,
    deadline: Instant,
}

impl ZmodemDetector {
    pub fn new() -> ZmodemDetector {
        ZmodemDetector { active: false, deadline: Instant::now() }
    }

    /// 当前是否处于传输透传状态（键盘输入也要原样直写，不走编码）。
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        if self.active {
            let now = Instant::now();
            if now >= self.deadline {
                self.active = false;
            } else {
                self.deadline = now + IDLE_TIMEOUT;
                return true;
            }
        }
        if contains(bytes, ZRQINIT_PREFIX) {
            self.active = true;
            self.deadline = Instant::now() + IDLE_TIMEOUT;
            return true;
        }
        false
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::{ZmodemDetector, IDLE_TIMEOUT};
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn 普通输出不透传() {
        let mut d = ZmodemDetector::new();
        assert!(!d.feed(b"hello world"));
        assert!(!d.feed(b"\x1b[31merror\x1b[0m"));
    }

    #[test]
    fn rz启动后进入透传并超时退出() {
        let mut d = ZmodemDetector::new();
        let mut init = b"**\x18".to_vec();
        init.extend_from_slice(b"B00000000000000\r");
        assert!(d.feed(&init));
        assert!(d.feed(b"\x2a\xc8\x01")); // 传输中的任意帧
        sleep(IDLE_TIMEOUT + Duration::from_millis(50));
        assert!(!d.feed(b"back to shell"));
    }

    #[test]
    fn 短字节序列不误判() {
        let mut d = ZmodemDetector::new();
        assert!(!d.feed(b"**"));
        assert!(!d.feed(&[0x18u8]));
    }
}
