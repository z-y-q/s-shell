import { useEffect, useRef, useState } from "react";
import type { Host } from "../lib/ipc";

export interface PaletteAction {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

interface Props {
  hosts: Host[];
  onClose: () => void;
  onConnect: (host: Host) => void;
  onOpenAi: () => void;
  onOpenSettings: () => void;
  onOpenSftp: () => void;
  onOpenReplay: () => void;
}

export default function CommandPalette({
  hosts,
  onClose,
  onConnect,
  onOpenAi,
  onOpenSettings,
  onOpenSftp,
  onOpenReplay,
}: Props) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const actions: PaletteAction[] = [
    { id: "ai", label: "AI 助手", run: onOpenAi },
    { id: "sftp", label: "文件管理（当前会话）", run: onOpenSftp },
    { id: "replay", label: "回放会话录制", run: onOpenReplay },
    { id: "settings", label: "设置", run: onOpenSettings },
    ...hosts.map((host) => ({
      id: `host-${host.id}`,
      label: `连接：${host.name}`,
      hint: `${host.username}@${host.host}`,
      run: () => onConnect(host),
    })),
  ];

  const filtered = actions.filter((a) =>
    (a.label + (a.hint ?? "")).toLowerCase().includes(query.toLowerCase()),
  );

  const keydown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      const action = filtered[index];
      if (action) {
        onClose();
        action.run();
      }
    }
  };

  return (
    <div className="absolute inset-0 z-40 flex items-start justify-center bg-black/50 pt-24" onClick={onClose}>
      <div
        className="w-[520px] rounded-xl border border-[var(--border)] bg-[var(--ws-bg)] shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          className="w-full rounded-t-lg border-b border-[var(--border)] bg-transparent px-4 py-3 text-sm text-[var(--text)] outline-none"
          placeholder="搜索主机或命令（上下选择，回车执行）"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
          onKeyDown={keydown}
        />
        <div className="max-h-80 overflow-y-auto py-1">
          {filtered.length === 0 && (
            <p className="px-4 py-3 text-xs text-[var(--dim)]">没有匹配项</p>
          )}
          {filtered.map((action, i) => (
            <button
              key={action.id}
              className={`flex w-full items-center justify-between px-4 py-2 text-left text-sm ${
                i === index ? "bg-sky-900/60 text-white" : "text-[var(--text)] hover:bg-[var(--ws-btn)]"
              }`}
              onMouseEnter={() => setIndex(i)}
              onClick={() => {
                onClose();
                action.run();
              }}
            >
              <span>{action.label}</span>
              {action.hint && <span className="text-xs text-[var(--dim)]">{action.hint}</span>}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
