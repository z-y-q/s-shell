import { useEffect, useState } from "react";
import { snippetDelete, snippetList, snippetSave, type Snippet } from "../lib/ipc";

interface Props {
  onSendToTerminal: (content: string) => void;
  hasSession: boolean;
}

export default function SnippetsView({ onSendToTerminal, hasSession }: Props) {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [editing, setEditing] = useState<{ id: number | null; name: string; content: string } | null>(
    null,
  );

  const load = () => {
    void snippetList().then(setSnippets);
  };

  useEffect(load, []);

  const save = () => {
    if (!editing || !editing.name.trim() || !editing.content.trim()) {
      return;
    }
    void snippetSave({ name: editing.name.trim(), content: editing.content }, editing.id).then(() => {
      setEditing(null);
      load();
    });
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-6">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-xl font-semibold">片段</h1>
        <button
          className="sbtn sbtn-primary"
          onClick={() => setEditing({ id: null, name: "", content: "" })}
        >
          新建片段
        </button>
      </div>
      <p className="mb-4 text-xs leading-4 text-[var(--dim)]">
        常用命令存成片段，一键发送到当前终端（不会自动执行，回车由你决定）。
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
                title="写入当前终端（不自动回车）"
                onClick={() => onSendToTerminal(s.content)}
              >
                发送到终端
              </button>
              <button
                className="rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
                onClick={() => setEditing({ id: s.id, name: s.name, content: s.content })}
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
            <pre className="mt-1 whitespace-pre-wrap font-mono text-xs text-[var(--dim)]">
              {s.content}
            </pre>
          </div>
        ))}
      </div>

      {snippets.length > 0 && hasSession && (
        <p className="mt-3 text-xs text-[var(--faint)]">点「发送到终端」写入当前会话，回车由你决定。</p>
      )}
    </div>
  );
}
