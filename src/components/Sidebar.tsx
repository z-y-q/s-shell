import type { Host } from "../lib/ipc";

interface Props {
  view: "home" | "logs" | "term" | "known" | "snippets" | "tunnels";
  onNav: (view: "home" | "logs" | "term" | "known" | "snippets" | "tunnels") => void;
  onOpenSettings: () => void;
  termCount: number;
  collapsed: boolean;
  onToggleCollapsed: () => void;
}

function NavIcon({
  kind,
}: {
  kind: "hosts" | "logs" | "settings" | "term" | "collapse" | "expand" | "tunnel";
}) {
  const common = {
    width: 16,
    height: 16,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
  if (kind === "term") {
    return (
      <svg {...common}>
        <rect x="2" y="4" width="20" height="16" rx="2" />
        <path d="M6 8l4 4-4 4M12 16h6" />
      </svg>
    );
  }
  if (kind === "hosts") {
    return (
      <svg {...common}>
        <rect x="3" y="3" width="7" height="7" rx="1.5" />
        <rect x="14" y="3" width="7" height="7" rx="1.5" />
        <rect x="3" y="14" width="7" height="7" rx="1.5" />
        <rect x="14" y="14" width="7" height="7" rx="1.5" />
      </svg>
    );
  }
  if (kind === "logs") {
    return (
      <svg {...common}>
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
      </svg>
    );
  }
  if (kind === "tunnel") {
    return (
      <svg {...common}>
        <path d="M4 8h6M14 8h6M4 16h6M14 16h6" />
        <circle cx="12" cy="8" r="2.2" />
        <circle cx="12" cy="16" r="2.2" />
      </svg>
    );
  }
  if (kind === "collapse") {
    return (
      <svg {...common}>
        <path d="M15 6l-6 6 6 6" />
      </svg>
    );
  }
  if (kind === "expand") {
    return (
      <svg {...common}>
        <path d="M9 6l6 6-6 6" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

export default function Sidebar({
  view,
  onNav,
  onOpenSettings,
  termCount,
  collapsed,
  onToggleCollapsed,
}: Props) {
  const items: Array<{
    id: "home" | "logs" | "term" | "known" | "snippets" | "tunnels";
    label: string;
    kind: "hosts" | "logs" | "term" | "tunnel";
  }> = [
    { id: "home", label: "主机", kind: "hosts" },
    { id: "tunnels", label: "隧道", kind: "tunnel" },
    { id: "snippets", label: "片段", kind: "hosts" },
    { id: "known", label: "已知主机", kind: "hosts" },
  ];
  items.push({ id: "term", label: termCount > 0 ? `终端（${termCount}）` : "终端", kind: "term" });
  items.push({ id: "logs", label: "日志", kind: "logs" });
  return (
    <aside
      className={`flex shrink-0 flex-col overflow-hidden border-r border-[var(--border)] bg-[var(--panel)] transition-[width] duration-200 ${
        collapsed ? "w-14" : "w-56"
      }`}
    >
      {collapsed ? (
        <div className="flex flex-col items-center pb-2 pt-4">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-sky-500 to-blue-700 text-sm font-bold text-white">
            S
          </div>
          <button
            className="mt-2 rounded-md p-1 text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
            title="展开侧栏（⌘B）"
            onClick={onToggleCollapsed}
          >
            <NavIcon kind="expand" />
          </button>
        </div>
      ) : (
        <div className="flex items-center gap-2 px-4 pb-2 pt-4">
          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-sky-500 to-blue-700 text-sm font-bold text-white">
            S
          </div>
          <span className="min-w-0 flex-1 truncate text-[15px] font-semibold">s-shell</span>
          <button
            className="rounded-md p-1 text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
            title="收起侧栏（⌘B）"
            onClick={onToggleCollapsed}
          >
            <NavIcon kind="collapse" />
          </button>
        </div>
      )}
      <nav className={`mt-2 flex flex-col gap-0.5 ${collapsed ? "px-1.5" : "px-2"}`}>
        {items.map((item) => (
          <button
            key={item.id}
            title={collapsed ? item.label : undefined}
            className={`flex items-center text-[13px] ${
              collapsed ? "justify-center rounded-lg py-2" : "gap-2.5 rounded-lg px-3 py-2"
            } ${
              view === item.id
                ? "bg-[var(--accent-soft)] font-medium text-[var(--accent)]"
                : "text-[var(--text)] hover:bg-[var(--hover)]"
            }`}
            onClick={() => onNav(item.id)}
          >
            <NavIcon kind={item.kind} />
            {!collapsed && <span className="min-w-0 truncate">{item.label}</span>}
          </button>
        ))}
        <button
          title={collapsed ? "设置" : undefined}
          className={`flex items-center text-[13px] text-[var(--text)] hover:bg-[var(--hover)] ${
            collapsed ? "justify-center rounded-lg py-2" : "gap-2.5 rounded-lg px-3 py-2 text-left"
          }`}
          onClick={onOpenSettings}
        >
          <NavIcon kind="settings" />
          {!collapsed && <span>设置</span>}
        </button>
      </nav>
      <div className="flex-1" />
      {!collapsed && (
        <p className="px-4 pb-4 text-[11px] text-[var(--faint)]">SSH · SFTP · AI · MCP</p>
      )}
    </aside>
  );
}

export type { Host };
