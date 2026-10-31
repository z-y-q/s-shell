use serde_json::json;

use crate::store::Store;
use crate::vault;

async fn client(user: &str, pass: &str) -> Result<reqwest::Client, String> {
    let _ = (user, pass);
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}

async fn webdav_target(store: &Store) -> Result<(String, String, String), String> {
    let base = store
        .setting_get("webdav_url")?
        .filter(|s| !s.trim().is_empty())
        .ok_or("尚未配置 WebDAV 地址")?;
    let user = store.setting_get("webdav_user")?.unwrap_or_default();
    let pass = vault::get(0, "webdavPassword")?.unwrap_or_default();
    Ok((base.trim_end_matches('/').to_string(), user, pass))
}

/// 推送快照：主机配置（含跳板与更新时间）与全部设置，凭据不在其中。
pub async fn push(store: &Store) -> Result<String, String> {
    let (base, user, pass) = webdav_target(store).await?;
    let hosts = store.list()?;
    let settings = store.settings_all()?;
    let payload = json!({
        "version": 1,
        "hosts": hosts,
        "settings": settings,
    });
    let url = format!("{base}/s-shell-config.json");
    let resp = client(&user, &pass)
        .await?
        .put(&url)
        .basic_auth(&user, Some(&pass))
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .send()
        .await
        .map_err(|e| format!("上传失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("WebDAV 返回 {}", resp.status()));
    }
    Ok(format!("已推送 {} 台主机与 {} 项设置", hosts.len(), settings.len()))
}

/// 拉取快照并合并：同名主机按更新时间新者胜，本机没有的直接新增。
pub async fn pull(store: &Store) -> Result<String, String> {
    let (base, user, pass) = webdav_target(store).await?;
    let url = format!("{base}/s-shell-config.json");
    let resp = client(&user, &pass)
        .await?
        .get(&url)
        .basic_auth(&user, Some(&pass))
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("WebDAV 返回 {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("快照解析失败：{e}"))?;
    let hosts: Vec<crate::store::Host> = serde_json::from_value(value["hosts"].clone())
        .map_err(|e| format!("快照主机数据异常：{e}"))?;
    let settings: Vec<(String, String)> =
        serde_json::from_value(value["settings"].clone()).unwrap_or_default();
    let (ins, upd, skip) = store.merge_snapshot(&hosts, &settings)?;
    Ok(format!("合并完成：新增 {ins}，更新 {upd}，保持不变 {skip}"))
}
