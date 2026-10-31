import { useEffect, useState } from "react";
import {
  aiAsk,
  aiCheckCommand,
  aiPreview,
  batchExec,
  onAiSummary,
  sessionContext,
  sessionWrite,
  type BatchResult,
  type Host,
} from "../lib/ipc";
import { useHosts } from "../stores/hosts";

interface Props {
  sessionId: number | null;
  host: Host | null;
  getSelection: () => string;
  onNotice: (text: string) => void;
}

interface PreviewLine {
  text: string;
  included: boolean;
}

interface SummaryItem {
  text: string;
  time: string;
}

function buildContext(host: Host | null, selection: string): string {
  const hostLine = host
    ? `主机：${host.name}（${host.username}@${host.host}，编码 ${host.encoding}）`
    : "主机：无活动会话";
  return `${hostLine}\n\n${selection}`;
}

function renderAnswerWithWriteButtons(
  answer: string,
  sessionId: number | null,
  onNotice: (text: string) => void,
) {
  const parts: Array<{ kind: "text" | "bash"; body: string }> = [];
  const pattern = /```bash\n([\s\S]*?)```/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(answer)) !== null) {
    if (match.index > last) {
      parts.push({ kind: "text", body: answer.slice(last, match.index) });
    }
    parts.push({ kind: "bash", body: match[1].trimEnd() });
    last = match.index + match[0].length;
  }
  if (last < answer.length) {
    parts.push({ kind: "text", body: answer.slice(last) });
  }

  const write = async (command: string) => {
    if (sessionId === null) {
      onNotice("没有在线会话可写入");
      return;
    }
    const reasons = await aiCheckCommand(command);
    if (reasons.length > 0 && !window.confirm(`本地风险规则提示：\n${reasons.map((r) => `- ${r}`).join("\n")}\n\n仍要写入终端执行？`)) {
      return;
    }
    await sessionWrite(sessionId, new TextEncoder().encode(`${command}\r`));
    onNotice("命令已写入终端");
  };

  return parts.map((part, i) =>
    part.kind === "text" ? (
      <pre key={i} className="whitespace-pre-wrap text-[13px] leading-5 text-[var(--text)]">
        {part.body}
      </pre>
    ) : (
      <div key={i} className="my-1 rounded border border-[var(--border)] bg-[var(--ws-btn)]/70 p-2">
        <pre className="whitespace-pre-wrap text-[13px] leading-5 text-emerald-300">{part.body}</pre>
        <button
          className="mt-1 rounded bg-sky-800 px-2 py-0.5 text-xs text-white hover:bg-sky-700"
          onClick={() => void write(part.body)}
        >
          写入终端
        </button>
      </div>
    ),
  );
}

export default function AiDrawer({ sessionId, host, getSelection, onNotice }: Props) {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previewLines, setPreviewLines] = useState<PreviewLine[] | null>(null);
  const [cwd, setCwd] = useState<string | null>(null);
  const [summaries, setSummaries] = useState<SummaryItem[]>([]);
  const [batchMode, setBatchMode] = useState(false);
  const [batchIds, setBatchIds] = useState<number[]>([]);
  const [batchCommand, setBatchCommand] = useState("");
  const [batchResults, setBatchResults] = useState<BatchResult[] | null>(null);
  const { hosts } = useHosts();

  useEffect(() => {
    if (sessionId === null) {
      setCwd(null);
      return;
    }
    void sessionContext(sessionId)
      .then((ctx) => setCwd(ctx.cwd))
      .catch(() => setCwd(null));
  }, [sessionId]);

  useEffect(() => {
    let alive = true;
    void onAiSummary((s) => {
      if (!alive) {
        return;
      }
      setSummaries((prev) =>
        [{ text: s.text, time: new Date().toLocaleTimeString() }, ...prev].slice(0, 5),
      );
    });
    return () => {
      alive = false;
    };
  }, []);

  const startPreview = () => {
    const q = question.trim();
    if (!q || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    aiPreview(buildContext(host, sessionId !== null ? getSelection() : ""))
      .then((prepared) => {
        setPreviewLines(
          prepared.split("\n").map((text) => ({ text, included: text.trim().length > 0 })),
        );
        setBusy(false);
      })
      .catch((err) => {
        setError(String(err));
        setBusy(false);
      });
  };

  const send = () => {
    if (!previewLines || busy) {
      return;
    }
    const includedContext = previewLines
      .filter((l) => l.included)
      .map((l) => l.text)
      .join("\n");
    setBusy(true);
    setError(null);
    aiAsk(question.trim(), includedContext)
      .then((result) => {
        setAnswer(result);
        setPreviewLines(null);
        setBusy(false);
      })
      .catch((err) => {
        setError(String(err));
        setBusy(false);
      });
  };

  const runBatch = () => {
    if (batchIds.length === 0 || !batchCommand.trim() || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    batchExec(batchIds, batchCommand.trim())
      .then((results) => {
        setBatchResults(results);
        setBusy(false);
      })
      .catch((err) => {
        setError(String(err));
        setBusy(false);
      });
  };

  const summarizeBatch = () => {
    if (!batchResults || busy) {
      return;
    }
    setBusy(true);
    aiAsk(
      "以下是同一命令在多台主机上的执行结果，用中文汇总各台的差异与需要关注的点：",
      JSON.stringify(batchResults, null, 2),
    )
      .then((result) => {
        setAnswer(result);
        setBusy(false);
      })
      .catch((err) => {
        setError(String(err));
        setBusy(false);
      });
  };

  const inputCls =
    "w-full rounded border border-[var(--border)] bg-[var(--ws-btn)] p-2 text-sm text-[var(--text)] outline-none focus:border-sky-600";

  return (
    <aside className="ssh-drawer flex h-full w-96 flex-col border-l border-[var(--ws-border)] bg-[var(--ws-bg)]">
      <div className="flex items-center justify-between px-3 py-3">
        <span className="text-sm font-semibold text-[var(--text)]">AI 助手</span>
        <span className="text-xs text-[var(--dim)]">
          {host ? host.name : "无会话"}
          {cwd ? ` · ${cwd}` : cwd === null && sessionId !== null ? " · 目录未知" : ""}
        </span>
      </div>

      {summaries.length > 0 && (
        <div className="mx-3 mb-2 rounded border border-amber-800/60 bg-amber-900/10 p-2">
          <div className="mb-1 text-xs font-semibold text-amber-400">自动摘要</div>
          {summaries.map((s, i) => (
            <div key={i} className="mb-1 text-xs leading-4 text-[var(--text)]">
              <span className="text-[var(--dim)]">{s.time} </span>
              {s.text}
            </div>
          ))}
        </div>
      )}

      <div className="flex gap-2 px-3 text-xs">
        <button
          className={`rounded px-2 py-1 ${!batchMode ? "bg-sky-800 text-white" : "bg-[var(--ws-btn)] text-[var(--text)] hover:bg-[var(--ws-btn-hover)]"}`}
          onClick={() => setBatchMode(false)}
        >
          单会话问答
        </button>
        <button
          className={`rounded px-2 py-1 ${batchMode ? "bg-sky-800 text-white" : "bg-[var(--ws-btn)] text-[var(--text)] hover:bg-[var(--ws-btn-hover)]"}`}
          onClick={() => setBatchMode(true)}
        >
          批量执行
        </button>
      </div>

      {!batchMode ? (
        <>
          <div className="px-3 pt-3">
            <textarea
              className={`${inputCls} h-20 resize-none`}
              placeholder="描述你想做什么，例如：查看占用内存最高的 5 个进程"
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
            />
            <div className="mt-2 flex gap-2">
              <button
                className="flex-1 rounded border border-[var(--border)] py-1.5 text-sm text-[var(--text)] hover:bg-[var(--ws-btn)] disabled:opacity-50"
                disabled={busy || !question.trim()}
                onClick={startPreview}
              >
                预览待发送内容
              </button>
              <button
                className="flex-1 rounded bg-sky-700 py-1.5 text-sm text-white hover:bg-sky-600 disabled:opacity-50"
                disabled={busy || previewLines === null}
                onClick={send}
              >
                {busy ? "思考中" : "发送"}
              </button>
            </div>
          </div>

          {previewLines !== null && (
            <div className="mx-3 mt-2 rounded border border-[var(--border)] p-2">
              <div className="mb-1 text-xs text-[var(--dim)]">
                发送前检查：取消勾选的行不会发送给模型
              </div>
              <div className="max-h-40 overflow-y-auto">
                {previewLines.map((line, i) => (
                  <label key={i} className="flex items-start gap-1 text-xs text-[var(--text)]">
                    <input
                      type="checkbox"
                      className="mt-0.5"
                      checked={line.included}
                      onChange={() =>
                        setPreviewLines((prev) =>
                          prev!.map((l, j) => (j === i ? { ...l, included: !l.included } : l)),
                        )
                      }
                    />
                    <span className="min-w-0 break-all">{line.text}</span>
                  </label>
                ))}
              </div>
            </div>
          )}

          <div className="mt-3 min-h-0 flex-1 overflow-y-auto px-3 pb-3">
            {error && <p className="mb-2 text-xs text-red-400">{error}</p>}
            {renderAnswerWithWriteButtons(answer, sessionId, onNotice)}
          </div>
        </>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col px-3 pt-3">
          <div className="mb-2 max-h-40 overflow-y-auto rounded border border-[var(--ws-border)] p-2">
            {hosts.map((h) => (
              <label key={h.id} className="flex items-center gap-2 text-xs text-[var(--text)]">
                <input
                  type="checkbox"
                  checked={batchIds.includes(h.id)}
                  onChange={() =>
                    setBatchIds((prev) =>
                      prev.includes(h.id) ? prev.filter((x) => x !== h.id) : [...prev, h.id],
                    )
                  }
                />
                {h.name}（{h.host}）
              </label>
            ))}
          </div>
          <textarea
            className={`${inputCls} h-20 resize-none`}
            placeholder="要在选中主机上并行执行的命令"
            value={batchCommand}
            onChange={(e) => setBatchCommand(e.target.value)}
          />
          <button
            className="mt-2 rounded bg-sky-700 py-1.5 text-sm text-white hover:bg-sky-600 disabled:opacity-50"
            disabled={busy || batchIds.length === 0 || !batchCommand.trim()}
            onClick={runBatch}
          >
            {busy ? "执行中" : `并行执行（${batchIds.length} 台）`}
          </button>
          <div className="mt-2 min-h-0 flex-1 overflow-y-auto">
            {batchResults?.map((r) => (
              <div key={r.hostId} className="mb-2 rounded border border-[var(--ws-border)] p-2">
                <div className="text-xs font-semibold text-[var(--text)]">{r.name ?? r.hostId}</div>
                {r.error ? (
                  <p className="text-xs text-red-400">{r.error}</p>
                ) : (
                  <pre className="max-h-32 overflow-y-auto whitespace-pre-wrap text-xs text-[var(--dim)]">
                    {r.output}
                  </pre>
                )}
              </div>
            ))}
          </div>
          {batchResults && (
            <button
              className="mb-3 rounded border border-[var(--border)] py-1.5 text-sm text-[var(--text)] hover:bg-[var(--ws-btn)] disabled:opacity-50"
              disabled={busy}
              onClick={summarizeBatch}
            >
              AI 汇总差异
            </button>
          )}
        </div>
      )}
    </aside>
  );
}
