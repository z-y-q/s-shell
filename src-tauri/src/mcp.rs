use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::ssh::session;
use crate::store::Store;
use crate::sftp;

/// MCP stdio 服务端：s-shell mcp 子命令进入，行分隔 JSON-RPC 2.0。
/// run_command 默认要在主界面确认（写 mcp_requests 表等待裁决），
/// 用户可在设置里显式关闭审批开关。
pub async fn run() -> Result<(), String> {
    let store = Store::open();
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    loop {
        let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| format!("读取标准输入失败：{e}"))?
        else {
            return Ok(());
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = msg.get("id").cloned();
        let is_notification = id.is_none();
        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let params = msg.get("params").cloned().unwrap_or(json!({}));

        let result = match method.as_str() {
            "initialize" => Ok(json!({
                "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2024-11-05")),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "s-shell", "version": "0.1.0" }
            })),
            "tools/list" => Ok(tools_manifest()),
            "tools/call" => call_tool(&store, &params).await,
            "ping" => Ok(json!({})),
            other => {
                if is_notification {
                    continue;
                }
                Err(format!("未知方法：{other}"))
            }
        };

        if is_notification {
            continue;
        }
        let response = match result {
            Ok(v) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": e } }),
        };
        stdout
            .write_all(format!("{response}\n").as_bytes())
            .await
            .map_err(|e| format!("写出响应失败：{e}"))?;
        stdout.flush().await.map_err(|e| format!("刷新输出失败：{e}"))?;
    }
}

fn tools_manifest() -> Value {
    json!({
        "tools": [
            {
                "name": "list_hosts",
                "description": "列出 s-shell 里配置的全部主机",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "run_command",
                "description": "在一台主机上执行 shell 命令并返回输出；默认需要用户在 s-shell 界面确认",
                "inputSchema": {
                    "type": "object",
                    "required": ["hostId", "command"],
                    "properties": {
                        "hostId": { "type": "integer", "description": "主机 id（list_hosts 返回）" },
                        "command": { "type": "string", "description": "要执行的命令" }
                    }
                }
            },
            {
                "name": "upload_file",
                "description": "把本地文件通过 SFTP 上传到主机；默认需要用户在 s-shell 界面确认",
                "inputSchema": {
                    "type": "object",
                    "required": ["hostId", "localPath", "remotePath"],
                    "properties": {
                        "hostId": { "type": "integer" },
                        "localPath": { "type": "string" },
                        "remotePath": { "type": "string" }
                    }
                }
            }
        ]
    })
}

async fn call_tool(store: &Store, params: &Value) -> Result<Value, String> {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let text: String = match name {
        "list_hosts" => {
            let hosts = store.list()?;
            if hosts.is_empty() {
                "（还没有配置主机）".into()
            } else {
                hosts
                    .iter()
                    .map(|h| format!("- [{}] {}（{}@{}:{}）", h.id, h.name, h.username, h.host, h.port))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        "run_command" => {
            let host_id = args.get("hostId").and_then(|v| v.as_i64()).ok_or("缺少 hostId")?;
            let command = args.get("command").and_then(|v| v.as_str()).ok_or("缺少 command")?;
            let host = store.get(host_id)?.ok_or("主机不存在")?;
            approve(store, name, &args).await?;
            session::exec_command(store, &host, command, Duration::from_secs(30)).await?
        }
        "upload_file" => {
            let host_id = args.get("hostId").and_then(|v| v.as_i64()).ok_or("缺少 hostId")?;
            let local = args.get("localPath").and_then(|v| v.as_str()).ok_or("缺少 localPath")?;
            let remote = args.get("remotePath").and_then(|v| v.as_str()).ok_or("缺少 remotePath")?;
            let host = store.get(host_id)?.ok_or("主机不存在")?;
            approve(store, name, &args).await?;
            let (handle, _keepalive, _) =
                session::establish(store, &host, Arc::new(Mutex::new(HashMap::new()))).await?;
            sftp::transfer_upload(&handle, local, remote, None, 0, false, 0).await?;
            "上传完成".into()
        }
        other => return Err(format!("未知工具：{other}")),
    };
    Ok(json!({ "content": [ { "type": "text", "text": text } ] }))
}

async fn approve(store: &Store, tool: &str, args: &Value) -> Result<(), String> {
    if store.setting_get("mcp_approve_commands")?.as_deref() == Some("0") {
        return Ok(());
    }
    let id = store.mcp_request_create(tool, &args.to_string())?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        match store.mcp_request_status(id)? {
            Some(status) if status == "approved" => return Ok(()),
            Some(status) if status == "denied" => {
                return Err("外部 AI 工具的操作被你在 s-shell 界面上拒绝".into());
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = store.mcp_request_decide(id, false);
            return Err("等待界面确认超时（120 秒），已自动拒绝".into());
        }
    }
}
