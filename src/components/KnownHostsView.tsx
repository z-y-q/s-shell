import { useEffect, useState } from "react";
import { knownHostsDelete, knownHostsList, type KnownHost } from "../lib/ipc";

export default function KnownHostsView() {
  const [entries, setEntries] = useState<KnownHost[]>([]);
  const [loaded, setLoaded] = useState(false);

  const load = () => {
    void knownHostsList()
      .then((list) => {
        setEntries(list);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  };

  useEffect(load, []);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-6">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-xl font-semibold">已知主机</h1>
        <button className="sbtn" onClick={load}>
          刷新
        </button>
      </div>
      <p className="mb-4 text-xs leading-4 text-[var(--dim)]">
        首次连接时自动记录服务器指纹（SHA256）。删除条目后，下次连接会重新记录——仅在你核实过服务器身份（如重装系统）时操作。
      </p>
      {!loaded && <p className="text-sm text-[var(--faint)]">加载中…</p>}
      {loaded && entries.length === 0 && (
        <p className="text-sm text-[var(--faint)]">还没有记录。首次连接任意主机后会出现在这里。</p>
      )}
      <div className="flex flex-col gap-2">
        {entries.map((e) => (
          <div
            key={`${e.host}-${e.port}`}
            className="flex items-center gap-3 rounded-xl border border-[var(--border)] bg-[var(--panel)] px-4 py-3"
          >
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">
                {e.host}:{e.port}
              </div>
              <div className="truncate font-mono text-xs text-[var(--dim)]">
                {e.keyAlgo} · {e.fingerprint}
              </div>
              <div className="text-[11px] text-[var(--faint)]">首次记录 {e.firstSeen}</div>
            </div>
            <button
              className="shrink-0 rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-red-500"
              onClick={() => {
                if (window.confirm(`删除 ${e.host}:${e.port} 的指纹？删除后下次连接将重新记录。`)) {
                  void knownHostsDelete(e.host, e.port).then(load);
                }
              }}
            >
              删除
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
