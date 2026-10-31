use regex::Regex;
use std::sync::OnceLock;

use crate::masker;
use crate::store::Store;
use crate::vault;

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("AI 清洗正则非法")
}

fn rules() -> &'static (Regex, Regex, Regex) {
    static RULES: OnceLock<(Regex, Regex, Regex)> = OnceLock::new();
    RULES.get_or_init(|| {
        (
            // OSC 序列：ESC ] ... (BEL 或 ESC \)
            compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)"),
            // CSI 序列：ESC [ 参数 字节
            compile(r"\x1b\[[0-9;:?]*[ -/]*[@-~]"),
            // 其余单字符转义
            compile(r"\x1b[@-Z\\_-]"),
        )
    })
}

/// 终端上下文进入 AI 提示词之前，剥离全部 ANSI 转义序列，避免污染提示词。
pub fn strip_ansi(input: &str) -> String {
    let (osc, csi, single) = rules();
    let s = osc.replace_all(input, "");
    let s = csi.replace_all(&s, "");
    single.replace_all(&s, "").into_owned()
}

/// 危险命令本地规则库：命中返回原因列表。定位是辅助提示，执行权永远在人。
pub fn check_command(command: &str) -> Vec<String> {
    danger_rules()
        .iter()
        .filter(|(re, _)| re.is_match(command))
        .map(|(_, reason)| reason.clone())
        .collect()
}

fn danger_rules() -> &'static Vec<(Regex, String)> {
    static RULES: OnceLock<Vec<(Regex, String)>> = OnceLock::new();
    RULES.get_or_init(|| {
        vec![
            rule(r"rm\s+(-[a-zA-Z]*[rf][a-zA-Z]*\s+)+(/|~|\$HOME)(\s|$)", "递归强制删除根目录或家目录"),
            rule(r"rm\s+-[a-zA-Z]*rf|\brm\s+-[a-zA-Z]*fr\b", "递归强制删除（rm -rf）"),
            rule(r"\bmkfs(\.\w+)?\b", "格式化文件系统"),
            rule(r"\bdd\b[^|;&]*of=/dev/", "dd 直接写块设备"),
            rule(r"chmod\s+-R\s+777\s+/(\s|$)", "递归 777 根目录"),
            rule(r">\s*/dev/(sd|nvme|vd|hd)", "重定向覆盖块设备"),
            rule(r"\b(shutdown|halt)\b|\breboot\b|\binit\s+[06]\b", "关机或重启"),
            rule(r"DROP\s+(TABLE|DATABASE)|TRUNCATE\s+TABLE", "删库或清表"),
            rule(r":\(\)\{\s*:\s*\|\s*:\s*&\s*\}\s*;:", "fork 炸弹"),
        ]
    })
}

fn rule(pattern: &str, reason: &str) -> (Regex, String) {
    (
        Regex::new(pattern).expect("危险命令正则非法"),
        reason.to_string(),
    )
}

/// 上下文组装：控制序列清洗 → 脱敏 → 超长截尾。发送预览与真正发送共用。
pub fn prepare_context(context: &str) -> String {
    let cleaned = strip_ansi(context);
    let masked = masker::mask(&cleaned);
    let total = masked.chars().count();
    if total > 6000 {
        masked.chars().skip(total - 6000).collect()
    } else {
        masked
    }
}

fn build_user_content(masked_context: &str, question: &str) -> String {
    let mut user_content = String::new();
    if !masked_context.trim().is_empty() {
        user_content.push_str("【终端上下文（已剥离转义序列并脱敏）】\n");
        user_content.push_str(masked_context);
        user_content.push_str("\n\n");
    }
    user_content.push_str("【问题】\n");
    user_content.push_str(question);
    user_content
}

/// 发送前预览：返回清洗脱敏后的上下文文本，不触发模型调用、不需要密钥。
/// 前端按行展示供用户勾选排除，最终把保留的行拼回上下文发送。
pub fn preview(context: &str) -> String {
    prepare_context(context)
}

async fn ai_settings(store: &Store) -> Result<(String, String, String), String> {
    let base_url = store
        .setting_get("ai_base_url")?
        .unwrap_or_else(|| "https://api.deepseek.com/v1".into());
    let model = store
        .setting_get("ai_model")?
        .unwrap_or_else(|| "deepseek-chat".into());
    let api_key =
        vault::get(0, "aiApiKey")?.ok_or("尚未配置 AI 密钥：请先在设置里填入 API Key")?;
    Ok((base_url, model, api_key))
}

async fn chat(store: &Store, system: &str, user: &str, temperature: f64) -> Result<String, String> {
    let (base_url, model, api_key) = ai_settings(store).await?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "temperature": temperature
    });
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let resp = client
        .post(url)
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求模型服务失败：{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("模型服务返回 {status}：{text}"));
    }
    let value: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("响应格式异常：{value}"))
}

const SYSTEM_PROMPT: &str = "你是 s-shell 终端里的运维助手，用户正在管理远程 Linux/Unix 服务器。回答规则：\n\
1. 给命令时放在独立的 ```bash 代码块里，一条命令一行。\n\
2. 命令下方用一句中文说明作用和风险。\n\
3. 破坏性操作（删除、格式化、重启、改防火墙）必须先警告再给命令。\n\
4. 不确定远端系统版本时，先给探测命令再给后续步骤。\n\
5. 全程用中文回答。";

const SUMMARY_PROMPT: &str = "你是终端输出的摘要器。用不超过 200 字的中文总结这段终端输出，\
指出异常、报错和需要人关注的行；没有异常就一句话说明输出正常。只输出摘要本身。";

/// 问答主入口。
pub async fn ask(store: &Store, question: &str, context: &str) -> Result<String, String> {
    let user_content = build_user_content(&prepare_context(context), question);
    chat(store, SYSTEM_PROMPT, &user_content, 0.2).await
}

/// 长输出自动摘要（内核阈值触发后调用）。
pub async fn summarize(store: &Store, recent_output: &str) -> Result<String, String> {
    let cleaned = prepare_context(recent_output);
    chat(store, SUMMARY_PROMPT, &cleaned, 0.2).await
}

#[cfg(test)]
mod tests {
    use super::{check_command, preview, strip_ansi};

    #[test]
    fn 清洗颜色序列() {
        assert_eq!(strip_ansi("\u{1b}[31m红字\u{1b}[0m"), "红字");
    }

    #[test]
    fn 清洗窗口标题序列() {
        assert_eq!(strip_ansi("\u{1b}]0;服务器标题\u{7}正文"), "正文");
    }

    #[test]
    fn 保留普通文本() {
        assert_eq!(strip_ansi("ls -la /var/log"), "ls -la /var/log");
    }

    #[test]
    fn 危险规则命中与放行() {
        assert!(!check_command("rm -rf /tmp/build").is_empty());
        assert!(check_command("rm file.txt").is_empty());
        assert!(!check_command("mkfs.ext4 /dev/sda1").is_empty());
        assert!(!check_command("shutdown -h now").is_empty());
        assert!(!check_command("DROP TABLE users;").is_empty());
        assert!(check_command("df -h").is_empty());
    }

    #[test]
    fn 预览走清洗与脱敏() {
        let out = preview("\u{1b}[31mpassword: abc\u{1b}[0m");
        assert!(out.contains("password=[已脱敏]"));
        assert!(!out.contains('\u{1b}'));
    }
}
