import { useEffect, useState } from "react";
import {
  snippetDelete,
  snippetList,
  snippetSave,
  triggerDelete,
  triggerList,
  triggerSave,
  type Host,
  type Snippet,
  type Trigger,
} from "../lib/ipc";

interface Props {
  onSendToTerminal: (content: string, sendEnter: boolean) => void;
  hasSession: boolean;
  hosts: Host[];
}

export default function SnippetsView({ onSendToTerminal, hasSession, hosts }: Props) {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [triggers, setTriggers] = useState<Trigger[]>([]);
  const [editing, setEditing] = useState<{
    id: number | null;
    name: string;
    content: string;
    sendEnter: boolean;
    autoRun: boolean;
    hostId: string;
  } | null>(null);
  const [newTrigger, setNewTrigger] = useState({ pattern: "", payload: "", hostId: "" });

  const load = () => {
    void snippetList().then(setSnippets);
    void triggerList().then(setTriggers);
  };

  useEffect(load, []);

  const save = () => {
    if (!editing || !editing.name.trim() || !editing.content.trim()) {
      return;
    }
    void snippetSave(
      {
        name: editing.name.trim(),
        content: editing.content,
        sendEnter: editing.sendEnter,
        autoRun: editing.autoRun,
        hostId: editing.autoRun && editing.hostId !== "" ? Number(editing.hostId) : null,
      },
      editing.id,
    ).then(() => {
      setEditing(null);
      load();
    });
  };

  const addTrigger = () => {
    if (!newTrigger.pattern.trim() || !newTrigger.payload.trim()) {
      return;
    }
    void triggerSave(
      {
        hostId: newTrigger.hostId === "" ? null : Number(newTrigger.hostId),
        pattern: newTrigger.pattern.trim(),
        payload: newTrigger.payload,
      },
      null,
    ).then(() => {
      setNewTrigger({ pattern: "", payload: "", hostId: "" });
      load();
    });
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-6">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-xl font-semibold">片段</h1>
        <button
          className="sbtn sbtn-primary"
          onClick={() =>
            setEditing({ id: null, name: "", content: "", sendEnter: true, autoRun: false, hostId: "" })
          }
        >
          新建片段
        </button>
      </div>
      <p className="mb-4 text-xs leading-4 text-[var(--dim)]">
        片段发送到当前终端；勾选「自动执行」后，连接绑定主机时会按序自动发送。
        {!hasSession && " 当前无在线会话，发送不可用。"}
      </p>

      {editing && (
        <div className="mb-4 rounded-xl border border-[var(--border)] bg-[var(--panel)] p-4">
          <input
            className="sinput mb-2"
            placeholder="片段名称"
            value={editing.name}
            onChange={(e) => setEditing({ ...editing, name: e.target.value })}
          />
          <textarea
            className="sinput h-24 resize-none font-mono"
            placeholder="命令内容（可多行）"
            value={editing.content}
            onChange={(e) => setEditing({ ...editing, content: e.target.value })}
          />
          <div className="mt-2 flex flex-wrap items-center gap-4 text-xs text-[var(--dim)]">
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={editing.sendEnter}
                onChange={(e) => setEditing({ ...editing, sendEnter: e.target.checked })}
              />
              发送并回车
            </label>
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={editing.autoRun}
                onChange={(e) => setEditing({ ...editing, autoRun: e.target.checked })}
              />
              自动执行
            </label>
            {editing.autoRun && (
              <select
                className="sinput w-40 py-1"
                value={editing.hostId}
                onChange={(e) => setEditing({ ...editing, hostId: e.target.value })}
              >
                <option value="">全部主机</option>
                {hosts.map((h) => (
                  <option key={h.id} value={h.id}>
                    仅 {h.name}
                  </option>
                ))}
              </select>
            )}
          </div>
          <div className="mt-2 flex justify-end gap-2">
            <button className="sbtn" onClick={() => setEditing(null)}>
              取消
            </button>
            <button className="sbtn sbtn-primary" onClick={save}>
              保存
            </button>
          </div>
        </div>
      )}

      {snippets.length === 0 && !editing && (
        <p className="text-sm text-[var(--faint)]">还没有片段。</p>
      )}
      <div className="flex flex-col gap-2">
        {snippets.map((s) => (
          <div
            key={s.id}
            className="rounded-xl border border-[var(--border)] bg-[var(--panel)] px-4 py-3"
          >
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium">{s.name}</span>
              <div className="flex-1" />
              <button
                className="rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--accent)] disabled:opacity-40"
                disabled={!hasSession}
                title="写入当前终端"
                onClick={() => onSendToTerminal(s.content, s.sendEnter)}
              >
                发送到终端
              </button>
              <button
                className="rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
                onClick={() =>
                  setEditing({
                    id: s.id,
                    name: s.name,
                    content: s.content,
                    sendEnter: s.sendEnter,
                    autoRun: s.autoRun,
                    hostId: s.hostId === null ? "" : String(s.hostId),
                  })
                }
              >
                编辑
              </button>
              <button
                className="rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-red-500"
                onClick={() => {
                  if (window.confirm(`删除片段「${s.name}」？`)) {
                    void snippetDelete(s.id).then(load);
                  }
                }}
              >
                删除
              </button>
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              {s.autoRun && (
                <span className="rounded bg-[var(--accent-soft)] px-1.5 py-0.5 text-xs text-[var(--accent)]">
                  自动执行
                </span>
              )}
              {s.hostId !== null && (
                <span className="rounded bg-[var(--hover)] px-1.5 py-0.5 text-xs text-[var(--dim)]">
                  仅 {hosts.find((h) => h.id === s.hostId)?.name ?? "未知主机"}
                </span>
              )}
              {!s.sendEnter && (
                <span className="rounded bg-[var(--hover)] px-1.5 py-0.5 text-xs text-[var(--dim)]">
                  不带回车
                </span>
              )}
            </div>
            <pre className="mt-1 whitespace-pre-wrap font-mono text-xs text-[var(--dim)]">
              {s.content}
            </pre>
          </div>
        ))}
      </div>

      <div className="mt-8 mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">触发器</h2>
      </div>
      <p className="mb-3 text-xs leading-4 text-[var(--dim)]">
        输出命中正则后自动回包（冷却 2 秒防连发）；对新增触发器，重开会话生效。
      </p>
      <div className="ssection">
        <div className="stitle">新建触发器</div>
        <div className="flex flex-wrap items-center gap-2">
          <select
            className="sinput w-32"
            value={newTrigger.hostId}
            onChange={(e) => setNewTrigger({ ...newTrigger, hostId: e.target.value })}
          >
            <option value="">全部主机</option>
            {hosts.map((h) => (
              <option key={h.id} value={h.id}>
                仅 {h.name}
              </option>
            ))}
          </select>
          <input
            className="sinput w-56 font-mono"
            placeholder="输出正则，如 补丁失败"
            value={newTrigger.pattern}
            onChange={(e) => setNewTrigger({ ...newTrigger, pattern: e.target.value })}
          />
          <input
            className="sinput w-56 font-mono"
            placeholder="自动回包内容"
            value={newTrigger.payload}
            onChange={(e) => setNewTrigger({ ...newTrigger, payload: e.target.value })}
          />
          <button className="sbtn sbtn-primary" onClick={addTrigger}>
            添加
          </button>
        </div>
      </div>
      {triggers.length > 0 && (
        <div className="flex flex-col gap-2">
          {triggers.map((t) => (
            <div
              key={t.id}
              className="flex items-center gap-3 rounded-xl border border-[var(--border)] bg-[var(--panel)] px-4 py-3"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-xs text-[var(--text)]">
                  命中 {t.pattern} → 回包「{t.payload}」
                </div>
                <div className="text-xs text-[var(--dim)]">
                  {t.hostId === null
                    ? "全部主机"
                    : `仅 ${hosts.find((h) => h.id === t.hostId)?.name ?? "未知主机"}`}
                </div>
              </div>
              <button
                className="shrink-0 rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-red-500"
                onClick={() => void triggerDelete(t.id).then(load)}
              >
                删除
              </button>
            </div>
          ))}
        </div>
      )}

      {snippets.length > 0 && hasSession && (
        <p className="mt-3 text-xs text-[var(--faint)]">
          点「发送到终端」写入当前会话；勾选「发送并回车」的片段会直接执行。
        </p>
      )}
    </div>
  );
}
