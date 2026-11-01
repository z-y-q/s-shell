import { useEffect, useState } from "react";
import SidePanel from "./SidePanel";
import KeysSection from "./KeysSection";
import { applyTheme, THEMES } from "../theme";
import {
  aiConfigGet,
  settingsGet,
  settingsSet,
  syncPull,
  syncPush,
  vaultMasterUnlock,
  vaultSet,
  vaultStatus,
  type AiConfig,
} from "../lib/ipc";

interface Props {
  onClose: () => void;
}

export default function SettingsModal({ onClose }: Props) {
  const [config, setConfig] = useState<AiConfig | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [webdavUrl, setWebdavUrl] = useState("");
  const [webdavUser, setWebdavUser] = useState("");
  const [webdavPassword, setWebdavPassword] = useState("");
  const [mcpApprove, setMcpApprove] = useState(true);
  const [autoSummary, setAutoSummary] = useState(false);
  const [theme, setTheme] = useState("midnight");
  const [ligatures, setLigatures] = useState(false);
  const [scrollback, setScrollback] = useState("5000");
  const [vault, setVault] = useState<{ mode: "keyring" | "master"; unlocked: boolean } | null>(null);
  const [masterPassword, setMasterPassword] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void (async () => {
      const cfg = await aiConfigGet();
      setConfig(cfg);
      setBaseUrl(cfg.baseUrl);
      setModel(cfg.model);
      const v = await settingsGet("mcp_approve_commands");
      setMcpApprove(v !== "0");
      setAutoSummary((await settingsGet("ai_autosummary")) === "1");
      setTheme((await settingsGet("ui_theme")) ?? "midnight");
      setLigatures((await settingsGet("terminal_ligatures")) === "1");
      setScrollback((await settingsGet("terminal_scrollback")) ?? "5000");
      setWebdavUrl((await settingsGet("webdav_url")) ?? "");
      setWebdavUser((await settingsGet("webdav_user")) ?? "");
      setVault(await vaultStatus());
    })();
  }, []);

  const label = "slabel";
  const input = "sinput";

  const saveAll = async () => {
    setBusy(true);
    setMessage(null);
    const tasks: Array<Promise<void>> = [];
    if (baseUrl.trim()) {
      tasks.push(settingsSet("ai_base_url", baseUrl.trim()));
    }
    if (model.trim()) {
      tasks.push(settingsSet("ai_model", model.trim()));
    }
    if (apiKey) {
      tasks.push(vaultSet(0, "aiApiKey", apiKey));
    }
    if (webdavUrl.trim()) {
      tasks.push(settingsSet("webdav_url", webdavUrl.trim()));
    }
    if (webdavUser.trim()) {
      tasks.push(settingsSet("webdav_user", webdavUser.trim()));
    }
    if (webdavPassword) {
      tasks.push(vaultSet(0, "webdavPassword", webdavPassword));
    }
    tasks.push(settingsSet("mcp_approve_commands", mcpApprove ? "1" : "0"));
    tasks.push(settingsSet("ai_autosummary", autoSummary ? "1" : "0"));
    tasks.push(settingsSet("ui_theme", theme));
    tasks.push(settingsSet("terminal_ligatures", ligatures ? "1" : "0"));
    tasks.push(settingsSet("terminal_scrollback", scrollback));
    await Promise.all(tasks);
    setMessage("设置已保存");
    setBusy(false);
  };

  const runSync = async (kind: "push" | "pull") => {
    setBusy(true);
    setMessage(null);
    try {
      const result = kind === "push" ? await syncPush() : await syncPull();
      setMessage(result);
    } catch (err) {
      setMessage(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <SidePanel
      title="设置"
      subtitle="AI · 同步 · MCP · 终端外观 · 凭据库"
      onClose={onClose}
      footer={
        <div className="flex gap-2">
          <button className="sbtn flex-1" onClick={onClose}>
            关闭
          </button>
          <button
            className="sbtn sbtn-primary flex-1"
            disabled={busy}
            onClick={() => void saveAll()}
          >
            {busy ? "保存中" : "保存全部"}
          </button>
        </div>
      }
    >

        <h3 className="mb-2 text-sm font-semibold text-[var(--text)]">AI 助手</h3>
        <div className="mb-3">
          <label className={label}>接口地址（OpenAI 兼容，留空用 DeepSeek 默认）</label>
          <input className={input} value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.deepseek.com/v1" />
        </div>
        <div className="mb-3">
          <label className={label}>模型（留空用 deepseek-chat）</label>
          <input className={input} value={model} onChange={(e) => setModel(e.target.value)} placeholder="deepseek-chat" />
        </div>
        <div className="mb-4">
          <label className={label}>
            API Key
            {config?.hasKey && <span className="ml-2 text-emerald-500">已配置，留空表示不修改</span>}
          </label>
          <input className={input} type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} />
        </div>

        <h3 className="mb-2 text-sm font-semibold text-[var(--text)]">WebDAV 同步</h3>
        <div className="mb-3">
          <label className={label}>地址（含路径，如 https://dav.example.com/dav/s-shell）</label>
          <input className={input} value={webdavUrl} onChange={(e) => setWebdavUrl(e.target.value)} />
        </div>
        <div className="mb-3 flex gap-3">
          <div className="flex-1">
            <label className={label}>账号</label>
            <input className={input} value={webdavUser} onChange={(e) => setWebdavUser(e.target.value)} />
          </div>
          <div className="flex-1">
            <label className={label}>密码（存钥匙串，留空不改）</label>
            <input className={input} type="password" value={webdavPassword} onChange={(e) => setWebdavPassword(e.target.value)} />
          </div>
        </div>
        <div className="mb-4 flex gap-2">
          <button
            className="sbtn disabled:opacity-50"
            disabled={busy}
            onClick={() => void runSync("push")}
          >
            推送到 WebDAV
          </button>
          <button
            className="sbtn disabled:opacity-50"
            disabled={busy}
            onClick={() => void runSync("pull")}
          >
            从 WebDAV 拉取合并
          </button>
        </div>

        <h3 className="mb-2 text-sm font-semibold text-[var(--text)]">MCP 外部工具</h3>
        <label className="mb-2 flex items-center gap-2 text-sm text-[var(--text)]">
          <input
            type="checkbox"
            checked={mcpApprove}
            onChange={(e) => setMcpApprove(e.target.checked)}
          />
          run_command 等操作需在界面逐条确认（强烈建议保持开启）
        </label>

        <h3 className="mb-2 mt-4 text-sm font-semibold text-[var(--text)]">长输出自动摘要</h3>
        <label className="mb-4 flex items-center gap-2 text-sm text-[var(--text)]">
          <input
            type="checkbox"
            checked={autoSummary}
            onChange={(e) => setAutoSummary(e.target.checked)}
          />
          输出爆发超过 200KB 且停顿 2 秒时自动生成摘要（消耗模型额度）
        </label>

        <h3 className="mb-2 text-sm font-semibold text-[var(--text)]">主题（界面 + 终端联动，即时生效）</h3>
        <div className="mb-3 flex flex-col gap-1.5">
          {THEMES.map((t) => {
            const active = theme === t.id;
            const pick = () => {
              setTheme(t.id);
              applyTheme(t.id);
              void settingsSet("ui_theme", t.id);
              window.dispatchEvent(
                new CustomEvent("sshell-theme", { detail: { term: t.term } }),
              );
            };
            return (
              <button
                key={t.id}
                className={`flex items-center gap-3 rounded-xl border p-2 text-left ${
                  active
                    ? "border-[var(--accent)] bg-[var(--accent-soft)]"
                    : "border-[var(--border)] hover:bg-[var(--hover)]"
                }`}
                onClick={pick}
              >
                <span
                  className="flex h-10 w-16 shrink-0 flex-col justify-center gap-1 rounded-md px-2"
                  style={{ background: t.term.background }}
                >
                  <span className="block h-1 w-9 rounded" style={{ background: t.term.foreground }} />
                  <span className="block h-1 w-6 rounded" style={{ background: t.term.green }} />
                  <span className="block h-1 w-7 rounded" style={{ background: t.term.blue }} />
                </span>
                <span className="text-[13px]">{t.name}</span>
                {active && <span className="ml-auto text-xs text-[var(--accent)]">当前</span>}
              </button>
            );
          })}
        </div>
        <div className="mb-3 flex gap-3">
          <div className="flex-1">
            <label className={label}>滚动缓冲（越大越耗内存）</label>
            <select className={input} value={scrollback} onChange={(e) => setScrollback(e.target.value)}>
              <option value="5000">5 千行（约 5MB/标签）</option>
              <option value="50000">5 万行（约 48MB/标签）</option>
              <option value="100000">10 万行（约 96MB/标签）</option>
            </select>
          </div>
        </div>
        <label className="mb-4 flex items-center gap-2 text-sm text-[var(--text)]">
          <input
            type="checkbox"
            checked={ligatures}
            onChange={(e) => setLigatures(e.target.checked)}
          />
          启用字体连字（"=&gt;" 与 "===" 等符号合并显示）
        </label>

        <h3 className="mb-2 text-sm font-semibold text-[var(--text)]">密钥管理</h3>
        <div className="mb-4">
          <KeysSection />
        </div>

        <h3 className="mb-2 text-sm font-semibold text-[var(--text)]">凭据库</h3>
        {vault?.mode === "keyring" ? (
          <p className="mb-4 text-xs text-[var(--dim)]">
            当前使用系统钥匙串（{navigator.platform}）。凭据由操作系统保管。
          </p>
        ) : (
          <div className="mb-4">
            <p className="mb-1 text-xs text-amber-400">
              当前使用主密码模式（系统未提供凭据服务，凭据经 Argon2id + AES-256-GCM 加密存放）。
            </p>
            {vault?.unlocked ? (
              <p className="text-xs text-[var(--ok)]">已解锁。</p>
            ) : (
              <div className="flex gap-2">
                <input
                  className={input}
                  type="password"
                  placeholder="输入主密码解锁（首次输入即设置）"
                  value={masterPassword}
                  onChange={(e) => setMasterPassword(e.target.value)}
                />
                <button
                  className="shrink-0 rounded bg-[var(--ws-btn)] px-3 text-xs text-zinc-200 hover:bg-zinc-700"
                  onClick={() => {
                    void vaultMasterUnlock(masterPassword).then(() => {
                      setMasterPassword("");
                      void vaultStatus().then(setVault);
                    });
                  }}
                >
                  解锁
                </button>
              </div>
            )}
          </div>
        )}

        {message && <p className="mb-3 text-xs text-[var(--ok)]">{message}</p>}
    </SidePanel>
  );
}
