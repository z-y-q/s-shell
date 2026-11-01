import { useEffect, useState } from "react";
import {
  keyDeploy,
  keyGenerate,
  keyList,
  hostList,
  type Host,
} from "../lib/ipc";

interface KeyInfo {
  name: string;
  path: string;
  publicPath: string;
  fingerprint: string;
  createdAt: string;
}

export default function KeysSection() {
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [hosts, setHosts] = useState<Host[]>([]);
  const [newName, setNewName] = useState("");
  const [deployHost, setDeployHost] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => {
    void keyList().then(setKeys);
    void hostList().then(setHosts);
  };

  useEffect(load, []);

  const generate = () => {
    if (!newName.trim() || busy) {
      return;
    }
    setBusy(true);
    void keyGenerate(newName.trim())
      .then((res) => {
        setMessage(`已生成 ${res.name}（公钥已写入 ${res.publicKey.split(" ")[0]} 格式文件）`);
        setNewName("");
        load();
      })
      .catch((err) => setMessage(String(err)))
      .finally(() => setBusy(false));
  };

  const deploy = (key: KeyInfo) => {
    const hostId = Number(deployHost);
    if (!hostId || busy) {
      setMessage("先选择要部署到的主机");
      return;
    }
    setBusy(true);
    void keyDeploy(hostId, key.path)
      .then((out) => setMessage(`部署完成：${key.name} → ${hosts.find((h) => h.id === hostId)?.name}${out ? `（${out.trim()}）` : ""}`))
      .catch((err) => setMessage(String(err)))
      .finally(() => setBusy(false));
  };

  return (
    <div className="mb-4">
      <h3 className="mb-2 text-sm font-semibold text-[var(--text)]">密钥管理</h3>
      <p className="mb-2 text-xs leading-4 text-[var(--dim)]">
        密钥保存在 ~/.ssh/s-shell-keys/ 下；部署会把公钥追加到目标主机的 authorized_keys。
      </p>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <input
          className="sinput w-44"
          placeholder="密钥名称，如 prod"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
        />
        <button className="sbtn" disabled={busy || !newName.trim()} onClick={generate}>
          生成 ed25519
        </button>
        <select
          className="sinput w-44"
          value={deployHost}
          onChange={(e) => setDeployHost(e.target.value)}
        >
          <option value="">部署目标主机…</option>
          {hosts.map((h) => (
            <option key={h.id} value={h.id}>
              {h.name}
            </option>
          ))}
        </select>
      </div>
      {message && <p className="mb-2 text-xs text-emerald-600">{message}</p>}
      {keys.length === 0 ? (
        <p className="text-xs text-[var(--faint)]">还没有托管密钥。</p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {keys.map((k) => (
            <div
              key={k.name}
              className="flex items-center gap-2 rounded-lg border border-[var(--border)] px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium">{k.name}</div>
                <div className="truncate font-mono text-xs text-[var(--dim)]">
                  {k.fingerprint || "指纹未知"}
                </div>
                <div className="text-[11px] text-[var(--faint)]">创建于 {k.createdAt}</div>
              </div>
              <button
                className="shrink-0 rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--accent)] disabled:opacity-40"
                disabled={!deployHost || busy}
                onClick={() => deploy(k)}
              >
                部署公钥
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
