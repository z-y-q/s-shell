import type { ReactNode } from "react";

interface Props {
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
}

export default function SidePanel({ title, subtitle, onClose, children, footer, width }: Props) {
  return (
    <aside
      className="sside"
      style={width ? { width } : undefined}
    >
      <div className="flex items-start justify-between px-5 pb-3 pt-4">
        <div className="min-w-0">
          <h2 className="text-[15px] font-semibold">{title}</h2>
          {subtitle && <p className="mt-0.5 text-xs text-[var(--dim)]">{subtitle}</p>}
        </div>
        <button
          className="ml-2 rounded-md p-1 text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
          onClick={onClose}
        >
          ✕
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4">{children}</div>
      {footer && <div className="border-t border-[var(--border)] p-4">{footer}</div>}
    </aside>
  );
}
