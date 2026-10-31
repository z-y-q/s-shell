import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { ImageAddon } from "@xterm/addon-image";
import Zmodem from "zmodem.js";
import { getTheme, hostHue } from "../theme";
import {
  localClose,
  localOpen,
  localResize,
  localWrite,
  onSessionClosed,
  openSession,
  sessionClose,
  sessionResize,
  sessionWrite,
  settingsGet,
  zmodemSave,
  type Host,
} from "../lib/ipc";
import { dropSelection, setSelection } from "../lib/selections";

interface Props {
  host?: Host | null;
  local?: boolean;
  tabKey: string;
  active: boolean;
  stealth?: boolean;
  broadcast?: boolean;
  onSessionReady?: (sessionId: number) => void;
  onInput?: (text: string, send: (payload: Uint8Array, sessionId: number) => void) => void;
}

export default function TerminalView({
  host,
  local,
  tabKey,
  active,
  stealth,
  broadcast,
  onSessionReady,
  onInput,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef<number | null>(null);
  const zsessionRef = useRef<{ close: () => void } | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const stealthRef = useRef(stealth ?? false);
  stealthRef.current = stealth ?? false;
  const [initError, setInitError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(true);

  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;

    (async () => {
      try {
      // 界面主题捆绑终端配色：读取 ui_theme 并跟随主题切换事件实时更新
      const [uiThemeName, ligatures, scrollbackRaw] = await Promise.all([
        settingsGet("ui_theme").catch(() => null),
        settingsGet("terminal_ligatures").catch(() => null),
        settingsGet("terminal_scrollback").catch(() => null),
      ]);
      if (disposed) {
        return;
      }
      const theme = getTheme(uiThemeName).term;
      const term = new Terminal({
        fontFamily: '"SF Mono", Menlo, Monaco, "Courier New", monospace',
        fontSize: 13,
        cursorBlink: true,
        scrollback: Number(scrollbackRaw ?? "5000") || 5000,
        allowProposedApi: true,
        theme,
      });
      // 连字选项在类型里挂在 proposed 区，运行期设置
      (term.options as unknown as { fontLigatures: boolean }).fontLigatures = ligatures === "1";
      // 主题切换即时生效（不重开标签）
      const onThemeChange = (e: Event) => {
        const pal = (e as CustomEvent<{ term: unknown }>).detail.term;
        term.options.theme = pal as Record<string, string>;
      };
      window.addEventListener("sshell-theme", onThemeChange);
      termRef.current = term;

      const fit = new FitAddon();
      fitRef.current = fit;
      term.loadAddon(fit);
      term.loadAddon(new SearchAddon());
      term.loadAddon(new WebLinksAddon());
      // sixel 与内联图片显示（远端程序输出 sixel 序列时渲染，如 img2sixel）
      term.loadAddon(new ImageAddon());
      try {
        const webgl = new WebglAddon();
        webgl.onContextLoss(() => webgl.dispose());
        term.loadAddon(webgl);
        (window as unknown as Record<string, string>).__sshellRenderer = "webgl";
      } catch {
        // WebGL 不可用时自动落到 Canvas 渲染器，方案规定的降级路径
        (window as unknown as Record<string, string>).__sshellRenderer = "canvas";
      }

      const container = containerRef.current;
      if (!container) {
        throw new Error("终端容器元素不存在");
      }
      term.open(container);
      if (container.clientWidth > 0 && container.clientHeight > 0) {
        fit.fit();
      }

      // zmodem 哨兵只在内核标记为传输数据时才参与；普通输出走 xterm 快路径直写
      let zActive = false;
      const handleChunk = (raw: Uint8Array) => {
        const tag = raw[0];
        const payload = raw.subarray(1);
        if (tag === 1) {
          // zmodem 传输中：键盘输入交给协议端点，由哨兵经 sender 回调写回远端
          zActive = true;
          zsentry.consume(payload);
        } else {
          // 回显到达：与最近一次击键配对，分解三段耗时
          const arrival = performance.now();
          const pending = pendingEcho;
          pendingEcho = null;
          const parseStart = arrival;
          term.write(payload, () => {
            if (pending) {
              const total = performance.now() - pending.t0;
              const segInvoke = pending.invokeDone - pending.t0;
              const segKernel = arrival - (pending.invokeDone || pending.t0);
              const segRender = performance.now() - parseStart;
              chainCount += 1;
              chainTotal += total;
              if (total > chainMax) {
                chainMax = total;
              }
              if (segInvoke > segInvokeMax) {
                segInvokeMax = segInvoke;
              }
              if (segKernel > segKernelMax) {
                segKernelMax = segKernel;
              }
              if (segRender > segRenderMax) {
                segRenderMax = segRender;
              }
            }
          });
        }
      };

      const encoder = new TextEncoder();
      // 全链路回显采样：击键 → invoke 完成 → 回显上屏完成，三段分解定位延迟层
      let pendingEcho: {
        char: string;
        t0: number;
        invokeDone: number;
      } | null = null;
      let chainCount = 0;
      let chainTotal = 0;
      let chainMax = 0;
      let segInvokeMax = 0;
      let segKernelMax = 0;
      let segRenderMax = 0;
      const chainProbe = window.setInterval(() => {
        if (chainCount > 0) {
          const avg = (chainTotal / chainCount).toFixed(0);
          void import("@tauri-apps/api/core").then(({ invoke }) =>
            invoke("perf_report", {
              text: `全链路回显 ${chainCount} 笔 平均 ${avg}ms 最大 ${chainMax.toFixed(0)}ms ｜ 分段最大：invoke ${segInvokeMax.toFixed(0)}ms 内核+网络 ${segKernelMax.toFixed(0)}ms 上屏 ${segRenderMax.toFixed(0)}ms`,
            }),
          );
          chainCount = 0;
          chainTotal = 0;
          chainMax = 0;
          segInvokeMax = 0;
          segKernelMax = 0;
          segRenderMax = 0;
        }
      }, 10_000);
      term.onData((data) => {
        if (zActive) {
          // zmodem 传输进行中，键盘交给协议端点处理，不直写远端
          return;
        }
        pendingEcho = { char: data, t0: performance.now(), invokeDone: 0 };
        const send = (payload: Uint8Array, sid: number) => {
          const write = local ? localWrite : sessionWrite;
          void write(sid, payload)
            .then(() => {
              if (pendingEcho) {
                pendingEcho.invokeDone = performance.now();
              }
            })
            .catch(() => undefined);
        };
        if (broadcast && onInput) {
          // 广播输入：由父层把内容同步写入本分屏组的全部会话
          onInput(data, send);
          return;
        }
        const sessionId = sessionRef.current;
        if (sessionId !== null) {
          send(encoder.encode(data), sessionId);
        }
      });
      term.onSelectionChange(() => {
        // 隐身模式下选区即刻清除且不进入共享注册表
        if (stealthRef.current) {
          dropSelection(tabKey);
          return;
        }
        setSelection(tabKey, term.getSelection());
      });

      // zmodem 端点：所有远端字节先进哨兵，普通数据原样上屏，ZRQINIT 触发协议接管
      // 注意回调键名是 zmodem.js 规定的 sender（不是 to_sender）
      const zsentry = new Zmodem.Sentry({
        to_terminal: (octets: Uint8Array) => term.write(octets),
        sender: (octets: Uint8Array) => {
          const sessionId = sessionRef.current;
          if (sessionId !== null) {
            void (local ? localWrite : sessionWrite)(sessionId, octets);
          }
        },
        on_detect: (detection) => {
          const zsession = detection.confirm();
          zsessionRef.current = zsession;
          if (zsession.get_session_role() === "receive") {
            // rz：远端推送文件到本机，按官方 input/complete 事件收集后落盘
            zsession.on("offer", (offer: { get: (k: string) => unknown; accept: () => Promise<void>; on: (ev: string, cb: (data?: Uint8Array) => void) => void }) => {
              const name = String(offer.get("name") ?? "file");
              const parts: Uint8Array[] = [];
              offer.on("input", (data?: Uint8Array) => {
                if (data) {
                  parts.push(data);
                }
              });
              offer.accept().then(() => {
                const size = parts.reduce((acc, p) => acc + p.length, 0);
                const merged = new Uint8Array(size);
                let offset = 0;
                for (const part of parts) {
                  merged.set(part, offset);
                  offset += part.length;
                }
                void zmodemSave(name, bytesToBase64(merged)).then((path) => {
                  term.write(`\r\n\u001b[36m[s-shell] 已接收 ${name}（${size} 字节）→ ${path}\u001b[0m\r\n`);
                });
              });
            });
            zsession.on("session_end", () => {
              zsessionRef.current = null;
            });
            zsession.start();
          } else {
            term.write("\r\n\u001b[36m[s-shell] 远端请求发送文件（sz），请在弹出的选择框中选取\u001b[0m\r\n");
            fileInputRef.current?.click();
          }
        },
        on_retract: () => {
          zsessionRef.current = null;
        },
      });

      const onFileChosen = () => {
        const input = fileInputRef.current;
        const zsession = zsessionRef.current;
        if (!input || !zsession || input.files?.length !== 1) {
          return;
        }
        const file = input.files[0];
        const send = (Zmodem as unknown as { Browser?: { send_files?: (s: unknown, f: File[]) => Promise<void> } })
          .Browser?.send_files;
        const done = () => {
          term.write(`\r\n\u001b[36m[s-shell] ${file.name} 发送完成\u001b[0m\r\n`);
          zsessionRef.current = null;
          input.value = "";
        };
        if (send) {
          void send(zsession, [file]).then(done);
        } else {
          void (zsentry as unknown as { send_files: (s: unknown, f: File[]) => Promise<void> })
            .send_files(zsession, [file])
            .then(done);
        }
      };
      fileInputRef.current?.addEventListener("change", onFileChosen);

      let sessionCancelled = false;
      let disposeClosed: (() => void) | undefined;

      const openPty = (c: number, r: number, cb: (chunk: Uint8Array) => void) =>
        local ? localOpen(c, r, cb) : openSession(host!.id, c, r, cb);

      openPty(term.cols, term.rows, (chunk) => handleChunk(chunk))
        .then(async (sessionId) => {
          setConnecting(false);
          if (disposed || sessionCancelled) {
            void (local ? localClose : sessionClose)(sessionId);
            return;
          }
          sessionRef.current = sessionId;
          onSessionReady?.(sessionId);
          term.onResize(({ cols, rows }) => {
            if (sessionRef.current !== null) {
              void (local ? localResize : sessionResize)(sessionId, cols, rows);
            }
          });
          const unlisten = await onSessionClosed((sid) => {
            if (sid === sessionId) {
              term.write(
                local
                  ? "\r\n\u001b[33m[本地终端已退出]\u001b[0m\r\n"
                  : "\r\n\u001b[33m[连接已断开]\u001b[0m\r\n",
              );
            }
          });
          if (disposed) {
            unlisten();
            return;
          }
          disposeClosed = unlisten;
        })
        .catch((err) => {
          setConnecting(false);
          term.write(
            `\r\n\u001b[31m[${local ? "本地终端启动失败" : "连接失败"}] ${String(err)}\u001b[0m\r\n`,
          );
        });

      const observer = new ResizeObserver(() => {
        if (fitRaf !== null) {
          return;
        }
        // rAF 去抖 + 仅在列数真正变化时 fit，杜绝滚动条振荡引发的重排风暴
        fitRaf = window.requestAnimationFrame(() => {
          fitRaf = null;
          const c = containerRef.current;
          if (!c || c.clientWidth === 0 || c.clientHeight === 0) {
            return;
          }
          const proposed = fit.proposeDimensions();
          if (
            proposed &&
            (proposed.cols !== term.cols || proposed.rows !== term.rows)
          ) {
            fitCount += 1;
            fit.fit();
          }
        });
      });
      let fitCount = 0;
      let fitRaf: number | null = null;
      observer.observe(container);
      // 每 10 秒上报一次 fit 调用次数，用于取证渲染风暴
      const fitProbe = window.setInterval(() => {
        if (fitCount > 0) {
          void import("@tauri-apps/api/core").then(({ invoke }) =>
            invoke("perf_report", {
              text: `fit 调用 ${fitCount} 次/10s（正常应接近 0）`,
            }),
          );
          fitCount = 0;
        }
      }, 10_000);

      const onCopyBlock = (e: ClipboardEvent) => {
        if (stealthRef.current) {
          e.preventDefault();
          e.stopPropagation();
        }
      };
      container.addEventListener("copy", onCopyBlock, true);

      cleanup = () => {
        sessionCancelled = true;
        window.clearInterval(fitProbe);
        if (fitRaf !== null) {
          window.cancelAnimationFrame(fitRaf);
        }
        window.clearInterval(chainProbe);
        observer.disconnect();
        disposeClosed?.();
        fileInputRef.current?.removeEventListener("change", onFileChosen);
        window.removeEventListener("sshell-theme", onThemeChange);
        container.removeEventListener("copy", onCopyBlock, true);
        dropSelection(tabKey);
        const sessionId = sessionRef.current;
        if (sessionId !== null) {
          void (local ? localClose : sessionClose)(sessionId);
        }
        term.dispose();
      };
      } catch (err) {
        // 兜底：任何初始化失败都必须可见，不允许静默白屏
        console.error("[s-shell] 终端初始化失败", err);
        setInitError(String(err));
      }
    })();

    return () => {
      disposed = true;
      cleanup?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host?.id, local, tabKey]);

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

  return (
    <div
      className={`h-full w-full p-1 transition-[filter] ${stealth ? "blur-sm select-none" : ""}`}
      onCopyCapture={(e) => {
        if (stealth) {
          e.preventDefault();
          e.stopPropagation();
        }
      }}
    >
      {initError && (
        <div className="m-2 rounded border border-red-800 bg-red-900/30 p-2 text-xs text-red-300">
          终端初始化失败：{initError}
        </div>
      )}
      {connecting && !initError && (
        <div
          className="absolute inset-0 z-20 flex flex-col items-center justify-center gap-4"
          style={{ background: "var(--ws-bg)" }}
        >
          <div className="flex items-center gap-3">
            <div
              className="flex h-12 w-12 items-center justify-center rounded-xl text-lg font-bold text-white"
              style={{
                background: `linear-gradient(135deg, hsl(${hostHue(host?.name ?? "本地")},72%,55%), hsl(${(hostHue(host?.name ?? "本地") + 40) % 360},72%,45%))`,
              }}
            >
              {(host?.name ?? "本").slice(0, 1).toUpperCase()}
            </div>
            <div>
              <div className="text-sm font-medium text-[var(--ws-text)]">
                {host?.name ?? "本地终端"}
              </div>
              <div className="text-xs text-[var(--ws-text-dim)]">
                {host ? `SSH ${host.host}:${host.port} · 正在建立连接…` : "正在启动本机 shell…"}
              </div>
            </div>
          </div>
          <div className="h-1 w-56 overflow-hidden rounded-full bg-[var(--ws-btn)]">
            <div className="sshell-progress h-full w-1/3 rounded-full bg-[var(--accent)]" />
          </div>
        </div>
      )}
      <div ref={containerRef} className="h-full w-full" />
      <input ref={fileInputRef} type="file" className="hidden" />
    </div>
  );
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    binary += String.fromCharCode(...bytes.subarray(i, i + step));
  }
  return btoa(binary);
}
