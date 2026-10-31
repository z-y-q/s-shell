use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedHost {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: Option<String>,
}

fn default_username() -> String {
    "root".into()
}

/// OpenSSH config：Host/HostName/Port/User/IdentityFile，跳过通配模式。
pub fn parse_ssh_config(path: &str) -> Result<Vec<ImportedHost>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("读取 {path} 失败：{e}"))?;
    let mut out: Vec<ImportedHost> = Vec::new();
    let mut current: Option<ImportedHost> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("").to_lowercase();
        let value = parts.collect::<Vec<_>>().join(" ");
        match key.as_str() {
            "host" => {
                if let Some(existing) = current.take() {
                    out.push(existing);
                }
                let first = value.split_whitespace().next().unwrap_or("");
                if first.is_empty() || first.contains('*') || first.contains('?') || first.contains('!') {
                    current = None;
                } else {
                    current = Some(ImportedHost {
                        name: first.to_string(),
                        host: first.to_string(),
                        port: 22,
                        username: default_username(),
                        key_path: None,
                    });
                }
            }
            "hostname" => {
                if let Some(c) = current.as_mut() {
                    c.host = value;
                }
            }
            "port" => {
                if let (Some(c), Ok(p)) = (current.as_mut(), value.parse::<u16>()) {
                    c.port = p;
                }
            }
            "user" => {
                if let Some(c) = current.as_mut() {
                    c.username = value;
                }
            }
            "identityfile" => {
                if let Some(c) = current.as_mut() {
                    if c.key_path.is_none() {
                        let expanded = value
                            .strip_prefix("~/")
                            .map(|rest| {
                                dirs::home_dir()
                                    .map(|home| home.join(rest).to_string_lossy().into_owned())
                                    .unwrap_or(value.clone())
                            })
                            .unwrap_or(value.clone());
                        c.key_path = Some(expanded);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(existing) = current.take() {
        out.push(existing);
    }
    Ok(out)
}

/// Xshell：目录下的 .xsh 文件（INI），name 取文件名。
pub fn parse_xshell_dir(dir: &str) -> Result<Vec<ImportedHost>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("读取目录 {dir} 失败：{e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("xsh") {
            continue;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "xshell".into());
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                println!("[s-shell] 跳过 {}: {e}", path.display());
                continue;
            }
        };
        let mut host = String::new();
        let mut port: u16 = 22;
        let mut username = default_username();
        let mut section = String::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_uppercase();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_uppercase();
            let value = value.trim().to_string();
            match (section.as_str(), key.as_str()) {
                ("CONNECTION", "HOST") => host = value,
                ("CONNECTION", "PORT") => {
                    if let Ok(p) = value.parse() {
                        port = p;
                    }
                }
                ("CONNECTION_AUTH", "USERNAME") => username = value,
                _ => {}
            }
        }
        if !host.is_empty() {
            out.push(ImportedHost { name, host, port, username, key_path: None });
        }
    }
    Ok(out)
}

/// Termius 导出 JSON：宽松解析 hosts 数组的 label/name、address/host、port、username。
pub fn parse_termius_json(path: &str) -> Result<Vec<ImportedHost>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取 {path} 失败：{e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败：{e}"))?;
    let empty = Vec::new();
    let hosts = value["hosts"].as_array().unwrap_or(&empty);
    let mut out = Vec::new();
    for entry in hosts {
        let name = entry["label"]
            .as_str()
            .or_else(|| entry["name"].as_str())
            .unwrap_or("termius")
            .to_string();
        let address = entry["address"]
            .as_str()
            .or_else(|| entry["host"].as_str())
            .unwrap_or("");
        if address.is_empty() {
            continue;
        }
        let port = entry["port"].as_u64().unwrap_or(22) as u16;
        let username = entry["username"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(default_username);
        out.push(ImportedHost {
            name,
            host: address.to_string(),
            port,
            username,
            key_path: None,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{parse_ssh_config, parse_termius_json};

    fn write_temp(name: &str, content: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("import-test")
            .join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn 解析ssh_config并跳过通配() {
        let path = write_temp(
            "config",
            "Host web-1\n  HostName 10.0.0.5\n  Port 2222\n  User deploy\n  IdentityFile ~/.ssh/id_ed25519\n\nHost *\n  Compression yes\n",
        );
        let hosts = parse_ssh_config(&path).unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "web-1");
        assert_eq!(hosts[0].host, "10.0.0.5");
        assert_eq!(hosts[0].port, 2222);
        assert_eq!(hosts[0].username, "deploy");
        assert!(hosts[0].key_path.as_deref().unwrap().ends_with(".ssh/id_ed25519"));
    }

    #[test]
    fn 解析termius导出() {
        let path = write_temp(
            "termius.json",
            r#"{"hosts":[{"label":"db-1","address":"192.168.1.9","port":2222,"username":"admin"}]}"#,
        );
        let hosts = parse_termius_json(&path).unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "db-1");
        assert_eq!(hosts[0].port, 2222);
    }
}
