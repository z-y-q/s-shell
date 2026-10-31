use regex::Regex;
use std::sync::OnceLock;

struct Rules {
    pem: Regex,
    bearer: Regex,
    kv: Regex,
    ipv4: Regex,
    email: Regex,
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| Rules {
        pem: Regex::new(r"-----BEGIN [A-Z ]+-----[\s\S]*?-----END [A-Z ]+-----")
            .expect("内置正则非法"),
        bearer: Regex::new(r"(?i)bearer\s+[a-z0-9._\-]+").expect("内置正则非法"),
        kv: Regex::new(
            r"(?i)((?:[a-z0-9_\-]*)(?:password|passwd|pwd|token|secret|api[_-]?key)(?:[a-z0-9_\-]*))\s*[:=]\s*\S+",
        )
        .expect("内置正则非法"),
        ipv4: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("内置正则非法"),
        email: Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
            .expect("内置正则非法"),
    })
}

/// 对文本做尽力脱敏：密钥块、令牌、密码键值、IP、邮箱。
/// AI 上下文与连接日志共用这一套规则。
pub fn mask(input: &str) -> String {
    let r = rules();
    let s = r.pem.replace_all(input, "[已脱敏-密钥块]");
    let s = r.bearer.replace_all(&s, "[已脱敏-令牌]");
    let s = r.kv.replace_all(&s, "${1}=[已脱敏]");
    let s = r.ipv4.replace_all(&s, "[IP]");
    r.email.replace_all(&s, "[邮箱]").into_owned()
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn 脱敏密码键值() {
        assert_eq!(mask("password: hunter2"), "password=[已脱敏]");
        assert_eq!(mask("DB_TOKEN=abcdef123"), "DB_TOKEN=[已脱敏]");
    }

    #[test]
    fn 地址脱敏() {
        assert_eq!(mask("connect 10.1.2.3 ok"), "connect [IP] ok");
    }

    #[test]
    fn 脱敏密钥块() {
        let input = "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\ndef\n-----END OPENSSH PRIVATE KEY-----";
        assert_eq!(mask(input), "[已脱敏-密钥块]");
    }

    #[test]
    fn 脱敏令牌() {
        assert_eq!(mask("Bearer abc123.def456"), "[已脱敏-令牌]");
    }

    #[test]
    fn 普通命令不变() {
        assert_eq!(mask("df -h /data && free -m"), "df -h /data && free -m");
    }
}
