use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub key_path: Option<String>,
    pub encoding: String,
    #[serde(default)]
    pub compat_mode: bool,
    pub tmux_policy: String,
    #[serde(default)]
    pub jump_host_id: Option<i64>,
    #[serde(default)]
    pub cert_path: Option<String>,
    #[serde(default)]
    pub shell_integration: bool,
    #[serde(default = "default_proxy_kind")]
    pub proxy_kind: String,
    #[serde(default)]
    pub proxy_host: Option<String>,
    #[serde(default)]
    pub proxy_port: Option<u16>,
    #[serde(default)]
    pub proxy_command: Option<String>,
    #[serde(default)]
    pub compress: bool,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub group_name: String,
}

fn default_proxy_kind() -> String {
    "none".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forward {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub host_id: i64,
    pub kind: String, // local | remote | dynamic
    pub listen_port: u16,
    pub target_host: String,
    pub target_port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRequest {
    pub id: i64,
    pub tool: String,
    pub args_json: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHostRow {
    pub host: String,
    pub port: u16,
    pub key_algo: String,
    pub fingerprint: String,
    pub first_seen: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRow {
    #[serde(flatten)]
    pub forward: Forward,
    pub host_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub host_id: Option<i64>,
    pub pattern: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    pub content: String,
    #[serde(default = "default_true")]
    pub send_enter: bool,
    #[serde(default)]
    pub auto_run: bool,
    #[serde(default)]
    pub host_id: Option<i64>,
}

const HOST_COLS: &str =
    "id, name, host, port, username, auth_method, key_path, encoding, compat_mode, tmux_policy, jump_host_id, shell_integration, cert_path, proxy_kind, proxy_host, proxy_port, proxy_command, compress, updated_at, group_name";

fn row_to_host(row: &rusqlite::Row) -> rusqlite::Result<Host> {
    Ok(Host {
        id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        port: row.get(3)?,
        username: row.get(4)?,
        auth_method: row.get(5)?,
        key_path: row.get(6)?,
        encoding: row.get(7)?,
        compat_mode: row.get::<_, i64>(8)? != 0,
        tmux_policy: row.get(9)?,
        jump_host_id: row.get(10)?,
        shell_integration: row.get::<_, Option<i64>>(11)?.unwrap_or(0) != 0,
        cert_path: row.get(12)?,
        proxy_kind: row.get::<_, Option<String>>(13)?.unwrap_or_else(default_proxy_kind),
        proxy_host: row.get(14)?,
        proxy_port: row.get(15)?,
        proxy_command: row.get(16)?,
        compress: row.get::<_, Option<i64>>(17)?.unwrap_or(0) != 0,
        updated_at: row.get::<_, Option<String>>(18)?.unwrap_or_default(),
        group_name: row.get::<_, Option<String>>(19)?.unwrap_or_default(),
    })
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn ensure_column(conn: &Connection, table: &str, column: &str, decl: &str) {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("无法读取表结构");
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("无法读取表列")
        .collect::<Result<_, _>>()
        .expect("无法读取表列");
    drop(stmt);
    if !existing.iter().any(|c| c == column) {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"),
            [],
        )
        .expect("无法扩展表列");
    }
}

impl Store {
    fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS hosts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL,
                auth_method TEXT NOT NULL DEFAULT 'password',
                key_path TEXT,
                encoding TEXT NOT NULL DEFAULT 'utf8',
                compat_mode INTEGER NOT NULL DEFAULT 0,
                tmux_policy TEXT NOT NULL DEFAULT 'none',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS known_hosts (
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                key_algo TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                first_seen TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (host, port)
            );
            CREATE TABLE IF NOT EXISTS forwards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                host_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                listen_port INTEGER NOT NULL,
                target_host TEXT NOT NULL DEFAULT '',
                target_port INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS mcp_requests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool TEXT NOT NULL,
                args_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS snippets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                content TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS triggers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                host_id INTEGER,
                pattern TEXT NOT NULL,
                payload TEXT NOT NULL
            );",
        )?;
        ensure_column(conn, "snippets", "send_enter", "INTEGER");
        ensure_column(conn, "snippets", "auto_run", "INTEGER");
        ensure_column(conn, "snippets", "host_id", "INTEGER");
        conn.execute("UPDATE snippets SET send_enter=1 WHERE send_enter IS NULL", [])
            .expect("回填 snippets.send_enter 默认值失败");
        conn.execute("UPDATE snippets SET auto_run=0 WHERE auto_run IS NULL", [])
            .expect("回填 snippets.auto_run 默认值失败");
        ensure_column(conn, "hosts", "jump_host_id", "INTEGER");
        ensure_column(conn, "hosts", "shell_integration", "INTEGER");
        ensure_column(conn, "hosts", "cert_path", "TEXT");
        ensure_column(conn, "hosts", "proxy_kind", "TEXT");
        ensure_column(conn, "hosts", "proxy_host", "TEXT");
        ensure_column(conn, "hosts", "proxy_port", "INTEGER");
        ensure_column(conn, "hosts", "proxy_command", "TEXT");
        ensure_column(conn, "hosts", "compress", "INTEGER");
        ensure_column(conn, "hosts", "updated_at", "TEXT");
        ensure_column(conn, "hosts", "group_name", "TEXT");
        conn.execute("UPDATE hosts SET group_name='' WHERE group_name IS NULL", [])
            .expect("回填 hosts.group_name 默认值失败");
        // ALTER 加列后旧行为 NULL：回填默认值，读取层也做了空值容忍（双保险）
        conn.execute(
            "UPDATE hosts SET shell_integration=0 WHERE shell_integration IS NULL",
            [],
        )
        .expect("回填 shell_integration 默认值失败");
        conn.execute("UPDATE hosts SET compress=0 WHERE compress IS NULL", [])
            .expect("回填 compress 默认值失败");
        conn.execute(
            "UPDATE hosts SET proxy_kind='none' WHERE proxy_kind IS NULL",
            [],
        )
        .expect("回填 proxy_kind 默认值失败");
        Ok(())
    }

    pub fn open() -> Store {
        let mut dir = dirs::data_dir().expect("无法定位用户数据目录");
        dir.push("s-shell");
        std::fs::create_dir_all(&dir).expect("无法创建数据目录");
        let path = dir.join("s-shell.db");
        let conn = Connection::open(&path).expect("无法打开数据库文件");
        Self::init_schema(&conn).expect("无法初始化数据库表");
        Store { conn: Mutex::new(conn) }
    }

    #[cfg(test)]
    pub fn open_memory() -> Store {
        let conn = Connection::open_in_memory().expect("无法创建内存数据库");
        Self::init_schema(&conn).expect("无法初始化数据库表");
        Store { conn: Mutex::new(conn) }
    }

    pub fn list(&self) -> Result<Vec<Host>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare(&format!("SELECT {HOST_COLS} FROM hosts ORDER BY id"))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_to_host)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn get(&self, id: i64) -> Result<Option<Host>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare(&format!("SELECT {HOST_COLS} FROM hosts WHERE id = ?1"))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map([id], row_to_host).map_err(|e| e.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn save(&self, id: Option<i64>, h: &Host) -> Result<i64, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        match id {
            Some(id) => {
                let n = conn
                    .execute(
                        "UPDATE hosts SET name=?1, host=?2, port=?3, username=?4, auth_method=?5, \
                         key_path=?6, encoding=?7, compat_mode=?8, tmux_policy=?9, jump_host_id=?10, \
                         shell_integration=?11, cert_path=?12, proxy_kind=?13, proxy_host=?14, \
                         proxy_port=?15, proxy_command=?16, compress=?17, group_name=?18, updated_at=datetime('now') WHERE id=?19",
                        params![
                            h.name,
                            h.host,
                            h.port,
                            h.username,
                            h.auth_method,
                            h.key_path,
                            h.encoding,
                            h.compat_mode,
                            h.tmux_policy,
                            h.jump_host_id,
                            h.shell_integration,
                            h.cert_path,
                            h.proxy_kind,
                            h.proxy_host,
                            h.proxy_port,
                            h.proxy_command,
                            h.compress,
                            h.group_name,
                            id
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err(format!("主机 {id} 不存在"));
                }
                Ok(id)
            }
            None => {
                conn.execute(
                    "INSERT INTO hosts (name, host, port, username, auth_method, key_path, \
                     encoding, compat_mode, tmux_policy, jump_host_id, shell_integration, cert_path, \
                     proxy_kind, proxy_host, proxy_port, proxy_command, compress, group_name, updated_at) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,datetime('now'))",
                    params![
                        h.name,
                        h.host,
                        h.port,
                        h.username,
                        h.auth_method,
                        h.key_path,
                        h.encoding,
                        h.compat_mode,
                        h.tmux_policy,
                        h.jump_host_id,
                        h.shell_integration,
                        h.cert_path,
                        h.proxy_kind,
                        h.proxy_host,
                        h.proxy_port,
                        h.proxy_command,
                        h.compress,
                        h.group_name
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(conn.last_insert_rowid())
            }
        }
    }

    pub fn delete(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute("DELETE FROM hosts WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM forwards WHERE host_id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn setting_get(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key=?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([key], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn setting_set(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn settings_all(&self) -> Result<Vec<(String, String)>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    // ---- known_hosts ----

    pub fn known_hosts_list(&self) -> Result<Vec<KnownHostRow>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT host, port, key_algo, fingerprint, first_seen FROM known_hosts ORDER BY first_seen DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(KnownHostRow {
                    host: row.get(0)?,
                    port: row.get(1)?,
                    key_algo: row.get(2)?,
                    fingerprint: row.get(3)?,
                    first_seen: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn known_hosts_delete(&self, host: &str, port: u16) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute(
            "DELETE FROM known_hosts WHERE host=?1 AND port=?2",
            params![host, port],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn known_hosts_get(&self, host: &str, port: u16) -> Result<Option<(String, String)>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT key_algo, fingerprint FROM known_hosts WHERE host=?1 AND port=?2")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![host, port], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn known_hosts_insert(&self, host: &str, port: u16, algo: &str, fingerprint: &str) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute(
            "INSERT OR REPLACE INTO known_hosts (host, port, key_algo, fingerprint, first_seen) \
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            params![host, port, algo, fingerprint],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- 转发规则 ----

    /// 全局规则列表（隧道中心用），附主机名
    pub fn forward_list_all(&self) -> Result<Vec<ForwardRow>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare(
                "SELECT f.id, f.host_id, f.kind, f.listen_port, f.target_host, f.target_port, \
                        f.enabled, COALESCE(h.name, '已删除主机') \
                 FROM forwards f LEFT JOIN hosts h ON h.id = f.host_id \
                 ORDER BY f.listen_port",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ForwardRow {
                    forward: Forward {
                        id: row.get(0)?,
                        host_id: row.get(1)?,
                        kind: row.get(2)?,
                        listen_port: row.get(3)?,
                        target_host: row.get(4)?,
                        target_port: row.get(5)?,
                        enabled: row.get::<_, i64>(6)? != 0,
                    },
                    host_name: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn forward_set_enabled(&self, id: i64, enabled: bool) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute(
            "UPDATE forwards SET enabled=?1 WHERE id=?2",
            [if enabled { 1 } else { 0 }, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn forward_host_id(&self, id: i64) -> Result<Option<i64>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT host_id FROM forwards WHERE id=?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([id]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => row.get(0).map_err(|e| e.to_string()).map(Some),
            None => Ok(None),
        }
    }

    pub fn forward_list(&self, host_id: i64) -> Result<Vec<Forward>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT id, host_id, kind, listen_port, target_host, target_port, enabled \
                      FROM forwards WHERE host_id=?1 ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([host_id], |row| {
                Ok(Forward {
                    id: row.get(0)?,
                    host_id: row.get(1)?,
                    kind: row.get(2)?,
                    listen_port: row.get(3)?,
                    target_host: row.get(4)?,
                    target_port: row.get(5)?,
                    enabled: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn forward_save(&self, id: Option<i64>, f: &Forward) -> Result<i64, String> {
        if !matches!(f.kind.as_str(), "local" | "remote" | "dynamic") {
            return Err(format!("未知转发类型：{}", f.kind));
        }
        let conn = self.conn.lock().expect("数据库锁中毒");
        match id {
            Some(id) => {
                let n = conn
                    .execute(
                        "UPDATE forwards SET host_id=?1, kind=?2, listen_port=?3, target_host=?4, \
                         target_port=?5, enabled=?6 WHERE id=?7",
                        params![f.host_id, f.kind, f.listen_port, f.target_host, f.target_port, f.enabled, id],
                    )
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err(format!("转发规则 {id} 不存在"));
                }
                Ok(id)
            }
            None => {
                conn.execute(
                    "INSERT INTO forwards (host_id, kind, listen_port, target_host, target_port, enabled) \
                     VALUES (?1,?2,?3,?4,?5,?6)",
                    params![f.host_id, f.kind, f.listen_port, f.target_host, f.target_port, f.enabled],
                )
                .map_err(|e| e.to_string())?;
                Ok(conn.last_insert_rowid())
            }
        }
    }

    pub fn forward_delete(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute("DELETE FROM forwards WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- MCP 审批 ----

    pub fn mcp_request_create(&self, tool: &str, args_json: &str) -> Result<i64, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute(
            "DELETE FROM mcp_requests WHERE created_at < datetime('now', '-1 day')",
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO mcp_requests (tool, args_json) VALUES (?1, ?2)",
            params![tool, args_json],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn mcp_request_status(&self, id: i64) -> Result<Option<String>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT status FROM mcp_requests WHERE id=?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn mcp_request_decide(&self, id: i64, approved: bool) -> Result<(), String> {
        let status = if approved { "approved" } else { "denied" };
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute("UPDATE mcp_requests SET status=?1 WHERE id=?2", params![status, id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn mcp_pending_list(&self) -> Result<Vec<McpRequest>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT id, tool, args_json FROM mcp_requests WHERE status='pending' ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(McpRequest {
                    id: row.get(0)?,
                    tool: row.get(1)?,
                    args_json: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    // ---- 片段 ----

    pub fn snippet_list(&self) -> Result<Vec<Snippet>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare(
                "SELECT id, name, content, send_enter, auto_run, host_id \
                 FROM snippets ORDER BY name",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Snippet {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    content: row.get(2)?,
                    send_enter: row.get::<_, i64>(3).unwrap_or(1) != 0,
                    auto_run: row.get::<_, i64>(4).unwrap_or(0) != 0,
                    host_id: row.get(5).ok(),
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn snippet_save(&self, id: Option<i64>, s: &Snippet) -> Result<i64, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        match id {
            Some(id) => {
                let n = conn
                    .execute(
                        "UPDATE snippets SET name=?1, content=?2, send_enter=?3, auto_run=?4, host_id=?5 WHERE id=?6",
                        params![
                            s.name,
                            s.content,
                            s.send_enter as i64,
                            s.auto_run as i64,
                            s.host_id,
                            id
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err(format!("片段 {id} 不存在"));
                }
                Ok(id)
            }
            None => {
                conn.execute(
                    "INSERT INTO snippets (name, content, send_enter, auto_run, host_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        s.name,
                        s.content,
                        s.send_enter as i64,
                        s.auto_run as i64,
                        s.host_id
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(conn.last_insert_rowid())
            }
        }
    }

    pub fn snippet_delete(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute("DELETE FROM snippets WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- 触发器 ----

    pub fn trigger_list(&self) -> Result<Vec<Trigger>, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        let mut stmt = conn
            .prepare("SELECT id, host_id, pattern, payload FROM triggers ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Trigger {
                    id: row.get(0)?,
                    host_id: row.get(1).ok(),
                    pattern: row.get(2)?,
                    payload: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn trigger_save(&self, id: Option<i64>, t: &Trigger) -> Result<i64, String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        match id {
            Some(id) => {
                let n = conn
                    .execute(
                        "UPDATE triggers SET host_id=?1, pattern=?2, payload=?3 WHERE id=?4",
                        params![t.host_id, t.pattern, t.payload, id],
                    )
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err(format!("触发器 {id} 不存在"));
                }
                Ok(id)
            }
            None => {
                conn.execute(
                    "INSERT INTO triggers (host_id, pattern, payload) VALUES (?1, ?2, ?3)",
                    params![t.host_id, t.pattern, t.payload],
                )
                .map_err(|e| e.to_string())?;
                Ok(conn.last_insert_rowid())
            }
        }
    }

    pub fn trigger_delete(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().expect("数据库锁中毒");
        conn.execute("DELETE FROM triggers WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- 同步：快照合并 ----

    /// 快照合并：同一性以主机名为准（改名视为新主机，改地址视为更新），
    /// 冲突按更新时间新者胜。返回 (新增, 更新, 保持不变)。
    pub fn merge_snapshot(
        &self,
        hosts: &[Host],
        settings: &[(String, String)],
    ) -> Result<(usize, usize, usize), String> {
        let mut inserted = 0;
        let mut updated = 0;
        let mut skipped = 0;
        for h in hosts {
            let all = self.list()?;
            let found = all.iter().find(|e| e.name == h.name);
            match found {
                Some(local) => {
                    if h.updated_at > local.updated_at {
                        self.save(Some(local.id), h)?;
                        updated += 1;
                    } else {
                        skipped += 1;
                    }
                }
                None => {
                    self.save(None, h)?;
                    inserted += 1;
                }
            }
        }
        for (key, value) in settings {
            self.setting_set(key, value)?;
        }
        Ok((inserted, updated, skipped))
    }
}

#[cfg(test)]
mod tests {
    use super::{Forward, Host, Store};

    fn sample(name: &str) -> Host {
        Host {
            id: 0,
            name: name.into(),
            group_name: String::new(),
            host: "10.0.0.8".into(),
            port: 2222,
            username: "ops".into(),
            auth_method: "key".into(),
            key_path: Some("~/.ssh/id_ed25519".into()),
            encoding: "gbk".into(),
            compat_mode: true,
            tmux_policy: "wrap".into(),
            jump_host_id: None,
            cert_path: None,
            shell_integration: false,
            proxy_kind: "none".into(),
            proxy_host: None,
            proxy_port: None,
            proxy_command: None,
            compress: false,
            updated_at: String::new(),
        }
    }

    #[test]
    fn 增删改查全流程() {
        let store = Store::open_memory();
        let id = store.save(None, &sample("web-1")).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().name, "web-1");

        let mut updated = sample("web-2");
        updated.port = 22;
        store.save(Some(id), &updated).unwrap();
        let got = store.get(id).unwrap().unwrap();
        assert_eq!(got.name, "web-2");
        assert_eq!(got.port, 22);

        assert_eq!(store.list().unwrap().len(), 1);
        store.delete(id).unwrap();
        assert!(store.get(id).unwrap().is_none());
    }

    #[test]
    fn 更新不存在的主机报错() {
        let store = Store::open_memory();
        assert!(store.save(Some(999), &sample("ghost")).is_err());
    }

    #[test]
    fn 设置读写覆盖() {
        let store = Store::open_memory();
        assert_eq!(store.setting_get("ai_model").unwrap(), None);
        store.setting_set("ai_model", "deepseek-chat").unwrap();
        store.setting_set("ai_model", "gpt-test").unwrap();
        assert_eq!(store.setting_get("ai_model").unwrap().as_deref(), Some("gpt-test"));
    }

    #[test]
    fn 主机指纹首次记录后可查() {
        let store = Store::open_memory();
        assert!(store.known_hosts_get("srv1", 22).unwrap().is_none());
        store
            .known_hosts_insert("srv1", 22, "ssh-ed25519", "SHA256:abc")
            .unwrap();
        let got = store.known_hosts_get("srv1", 22).unwrap().unwrap();
        assert_eq!(got, ("ssh-ed25519".into(), "SHA256:abc".into()));
    }

    #[test]
    fn 转发规则增删改查() {
        let store = Store::open_memory();
        let host_id = store.save(None, &sample("fw-host")).unwrap();
        let rule = Forward {
            id: 0,
            host_id,
            kind: "local".into(),
            listen_port: 8080,
            target_host: "127.0.0.1".into(),
            target_port: 80,
            enabled: true,
        };
        let id = store.forward_save(None, &rule).unwrap();
        let list = store.forward_list(host_id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].listen_port, 8080);
        let mut edited = rule.clone();
        edited.kind = "dynamic".into();
        store.forward_save(Some(id), &edited).unwrap();
        assert_eq!(store.forward_list(host_id).unwrap()[0].kind, "dynamic");
        store.forward_delete(id).unwrap();
        assert!(store.forward_list(host_id).unwrap().is_empty());
    }

    #[test]
    fn mcp审批流转() {
        let store = Store::open_memory();
        let id = store.mcp_request_create("run_command", "{\"hostId\":1}").unwrap();
        assert_eq!(store.mcp_pending_list().unwrap().len(), 1);
        store.mcp_request_decide(id, false).unwrap();
        assert!(store.mcp_pending_list().unwrap().is_empty());
        assert_eq!(store.mcp_request_status(id).unwrap().as_deref(), Some("denied"));
    }

    #[test]
    fn 快照合并新增与更新() {
        let store = Store::open_memory();
        let id = store.save(None, &sample("web-1")).unwrap();
        let mut remote = sample("web-1");
        remote.updated_at = "2999-01-01 00:00:00".into();
        remote.host = "10.0.0.9".into();
        let (ins, upd, skip) = store.merge_snapshot(&[remote], &[("ai_model".into(), "m1".into())]).unwrap();
        assert_eq!((ins, upd, skip), (0, 1, 0));
        assert_eq!(store.get(id).unwrap().unwrap().host, "10.0.0.9");

        let mut older = sample("web-1");
        older.updated_at = "2000-01-01 00:00:00".into();
        let (ins, upd, skip) = store.merge_snapshot(&[older], &[]).unwrap();
        assert_eq!((ins, upd, skip), (0, 0, 1));

        let mut brand_new = sample("web-2");
        brand_new.updated_at = "2999-01-01 00:00:00".into();
        let (ins, _, _) = store.merge_snapshot(&[brand_new], &[]).unwrap();
        assert_eq!(ins, 1);
        assert_eq!(store.list().unwrap().len(), 2);
    }
}
