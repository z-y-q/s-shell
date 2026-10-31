import { useEffect, useState } from "react";
import { mcpDecide, mcpPendingList, type McpRequest } from "../lib/ipc";

export default function McpApprovals() {
  const [pending, setPending] = useState<McpRequest[]>([]);

  useEffect(() => {
    let alive = true;
    const poll = () => {
      void mcpPendingList()
        .then((list) => {
          if (alive) {
            setPending(list);
          }
        })
        .catch(() => undefined);
    };
    poll();
    const timer = window.setInterval(poll, 2000);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, []);

  if (pending.length === 0) {
    return null;
  }

  const decide = (id: number, approved: boolean) => {
    void mcpDecide(id, approved).then(() =>
      setPending((prev) => prev.filter((p) => p.id !== id)),
    );
  };

  return (
    <div className="absolute inset-0 z-40 flex items-center justify-center bg-black/60">
      <div className="max-h-[80%] w-[460px] overflow-y-auto rounded-lg border border-amber-700 bg-[var(--ws-bg)] p-5">
        <h2 className="mb-1 text-base font-semibold text-amber-400">外部 AI 工具请求确认</h2>
        <p className="mb-4 text-xs text-[var(--dim)]">
          以下操作来自 MCP 外部工具（如 Claude Code），请逐条确认。
        </p>
        {pending.map((req) => (
          <div key={req.id} className="mb-3 rounded border border-[var(--border)] p-3">
            <div className="mb-1 text-sm text-[var(--text)]">工具：{req.tool}</div>
            <pre className="mb-3 max-h-32 overflow-y-auto whitespace-pre-wrap text-xs text-[var(--dim)]">
              {req.argsJson}
            </pre>
            <div className="flex justify-end gap-2">
              <button
                className="rounded border border-[var(--border)] px-3 py-1 text-xs text-[var(--text)] hover:bg-[var(--ws-btn)]"
                onClick={() => decide(req.id, false)}
              >
                拒绝
              </button>
              <button
                className="rounded bg-emerald-700 px-3 py-1 text-xs text-white hover:bg-emerald-600"
                onClick={() => decide(req.id, true)}
              >
                批准执行
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
