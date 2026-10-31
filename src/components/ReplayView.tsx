import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { replayOpen } from "../lib/ipc";

interface Props {
  file: string;
  active: boolean;
}

export default function ReplayView({ file, active }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);

  useEffect(() => {
    const term = new Terminal({
      fontFamily: '"SF Mono", Menlo, Monaco, "Courier New", monospace',
      fontSize: 13,
      scrollback: 5000,
      theme: { background: "#0d1117", foreground: "#e6edf3" },
      disableStdin: true,
    });
    const fit = new FitAddon();
    fitRef.current = fit;
    term.loadAddon(fit);

    const container = containerRef.current;
    if (!container) {
      throw new Error("回放容器元素不存在");
    }
    term.open(container);
    if (container.clientWidth > 0 && container.clientHeight > 0) {
      fit.fit();
    }
    term.writeln(`\u{1b}[36m回放：${file}\u{1b}[0m`);

    const observer = new ResizeObserver(() => {
      if (container.clientWidth > 0 && container.clientHeight > 0) {
        fit.fit();
      }
    });
    observer.observe(container);

    const controller = new AbortController();
    void replayOpen(file, (chunk) => {
      if (!controller.signal.aborted) {
        term.write(chunk);
      }
    }).catch((err) => term.write(`\r\n\u{1b}[31m回放失败：${String(err)}\u{1b}[0m\r\n`));

    return () => {
      controller.abort();
      observer.disconnect();
      term.dispose();
    };
  }, [file]);

  useEffect(() => {
    if (active) {
      requestAnimationFrame(() => {
        const container = containerRef.current;
        const fit = fitRef.current;
        if (container && container.clientWidth > 0 && container.clientHeight > 0 && fit) {
          fit.fit();
        }
      });
    }
  }, [active]);

  return <div ref={containerRef} className="h-full w-full p-1" />;
}
