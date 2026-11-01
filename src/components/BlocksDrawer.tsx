import { useEffect, useState } from "react";
import { getEntry, type CommandBlock } from "../lib/blocks";

interface Props {
  paneKey: string;
  onClose: () => void;
}

export default function BlocksDrawer({ paneKey, onClose }: Props) {
  const [blocks, setBlocks] = useState<CommandBlock[]>([]);
  const [copied, setCopied] = useState<number | null>(null);

  useEffect(() => {
    const refresh = () => setBlocks([...(getEntry(paneKey)?.blocks ?? [])]);
    refresh();
    window.addEventListener("sshell-blocks", refresh);
    return () => window.removeEventListener("sshell-blocks", refresh);
  }, [paneKey]);

  const copyOutput = (block: CommandBlock, index: number) => {
    const text = getEntry(paneKey)?.copyAt?.(block) ?? "";
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(index);
      window.setTimeout(() => setCopied(null), 1200);
    });
  };

  return (
    <aside className="flex h-full w-[360px] flex-col border-l border-[var(--ws-border)] bg-[var(--ws-bg)]">
      <div className="flex items-center justify-between px-4 pb-2 pt-3">
        <h2 className="text-sm font-semibold text-[var(--text)]">命令块（本标签）</h2>
        <button
          className="rounded-md p-1 text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
          onClick={onClose}
        >
          ✕
        </button>
      </div>
      <p className="px-4 pb-2 text-xs leading-4 text-[var(--dim)]">
        每次回车划分一个命令块；跳转回看、复制该命令的完整输出。
      </p>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
        {blocks.length === 0 && (
          <p className="px-1 text-xs text-[var(--faint)]">
            还没有命令块。在本标签执行命令后出现在这里。
          </p>
        )}
        {[...blocks].reverse().map((block, ri) => {
          const index = blocks.length - 1 - ri;
          return (
            <div
              key={`${block.start}-${index}`}
              className="mb-1.5 rounded-lg border border-[var(--ws-border)] px-3 py-2"
            >
              <div className="truncate font-mono text-xs text-[var(--text)]" title={block.cmd}>
                {block.cmd || "(空命令)"}
              </div>
              <div className="mt-1 flex items-center gap-2">
                <span className="text-[11px] text-[var(--faint)]">
                  第 {block.start + 1}–{block.end + 1} 行
                </span>
                <button
                  className="rounded px-1.5 text-xs text-[var(--dim)] hover:text-[var(--accent)]"
                  onClick={() => getEntry(paneKey)?.scroll?.(block.start)}
                >
                  跳转
                </button>
                <button
                  className="rounded px-1.5 text-xs text-[var(--dim)] hover:text-[var(--accent)]"
                  onClick={() => copyOutput(block, index)}
                >
                  {copied === index ? "已复制 ✓" : "复制输出"}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </aside>
  );
}
