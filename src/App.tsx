import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { Channel, invoke } from "@tauri-apps/api/core";
import TerminalView from "./terminal/TerminalView";
import ReplayView from "./components/ReplayView";
import Sidebar from "./components/Sidebar";
import SettingsModal from "./components/SettingsModal";
import SftpPanel from "./components/SftpPanel";
import ForwardPanel from "./components/ForwardPanel";
import CommandPalette from "./components/CommandPalette";
import McpApprovals from "./components/McpApprovals";
import KnownHostsView from "./components/KnownHostsView";
import TunnelCenter from "./components/TunnelCenter";
import SnippetsView from "./components/SnippetsView";
import AiDrawer from "./components/AiDrawer";
import HostForm from "./components/HostForm";
import ImportDialog from "./components/ImportDialog";
import { getSelection } from "./lib/selections";
import { hostHue } from "./theme";
import {
  recordStart,
  recordStop,
  sessionLogStart,
  sessionLogStop,
  deleteHost,
  type Host,
} from "./lib/ipc";
import { useHosts } from "./stores/hosts";

interface Pane {
  key: string;
  sessionId: number | null;
}

interface Tab {
  key: string;
  kind: "terminal" | "replay" | "local";
  host?: Host;
  file?: string;
  panes: Pane[];
  splitDir: "horizontal" | "vertical";
  broadcast: boolean;
}

type View = "home" | "logs" | "term" | "known" | "snippets" | "tunnels";
type RightPanel = "ai" | "sftp" | null;

export default function App() {
  const { hosts, refresh, loaded } = useHosts();
  useEffect(() => {
    if (!loaded) {
      void refresh();
    }
  }, [loaded, refresh]);
  const [view, setView] = useState<View>("home");
  const [searchTerm, setSearchTerm] = useState("");
  const [hostFormOpen, setHostFormOpen] = useState(false);
  const [editingHost, setEditingHost] = useState<Host | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const filteredHosts = hosts.filter((h) =>
    (h.name + h.host + h.username).toLowerCase().includes(searchTerm.toLowerCase()),
  );
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [focusedPane, setFocusedPane] = useState<string | null>(null);
  const [rightPanel, setRightPanel] = useState<RightPanel>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(
    () => localStorage.getItem("sshell-sidebar") === "0",
  );
  useEffect(() => {
    localStorage.setItem("sshell-sidebar", sidebarCollapsed ? "0" : "1");
  }, [sidebarCollapsed]);
  const [forwardHost, setForwardHost] = useState<Host | null>(null);
  const [logLines, setLogLines] = useState<string[]>([]);
  const sessionIds = useRef<Record<string, number>>({});
  const [, setSessionTick] = useState(0);
  const [recording, setRecording] = useState<Record<string, boolean>>({});
  const [logging, setLogging] = useState<Record<string, string>>({});
  const [stealth, setStealth] = useState<Record<string, boolean>>({});
  const [notice, setNotice] = useState<string | null>(null);

  const activeTab = tabs.find((t) => t.key === activeKey) ?? null;
  const activePaneKey =
    activeTab && activeTab.panes.some((p) => p.key === focusedPane)
      ? focusedPane
      : (activeTab?.panes[0]?.key ?? null);
  const activeSessionId =
    activePaneKey !== null ? (sessionIds.current[activePaneKey] ?? null) : null;

  useEffect(() => {
    // 应用持久化主题（界面变量 + 终端配色）
    void (async () => {
      try {
        const { settingsGet } = await import("./lib/ipc");
        const { applyTheme } = await import("./theme");
        applyTheme((await settingsGet("ui_theme")) ?? "midnight");
      } catch {
        // 主题应用失败保持默认
      }
    })();
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setSidebarCollapsed((v) => !v);
      }
    };
    window.addEventListener("keydown", handler);
    // 顶栏菜单「设置…」（Cmd+,）
    let disposeMenu: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen("open-settings", () => setSettingsOpen(true)).then((un) => {
        disposeMenu = un;
      }),
    );
    return () => {
      window.removeEventListener("keydown", handler);
      disposeMenu?.();
    };
  }, []);

  // 启动自测：IPC 吞吐 + invoke 往返 + 通道突发回放，结果进日志
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const { perfStream } = await import("./lib/ipc");
          const totalMb = 8;
          const target = totalMb * 1024 * 1024;
          let received = 0;
          let doneResolve: (() => void) | null = null;
          const done = new Promise<void>((resolve) => {
            doneResolve = resolve;
          });
          const t0 = performance.now();
          await perfStream(totalMb, (chunk: Uint8Array) => {
            received += Math.max(0, chunk.length - 1);
            if (received >= target && doneResolve) {
              const resolve = doneResolve;
              doneResolve = null;
              resolve();
            }
          });
          await done;
          const secs = (performance.now() - t0) / 1000;
          const mibs = received / 1_048_576 / secs;
          const renderer = (window as unknown as Record<string, string>).__sshellRenderer ?? "未知";
          let min = Number.MAX_SAFE_INTEGER;
          let sum = 0;
          for (let i = 0; i < 30; i += 1) {
            const p = performance.now();
            await invoke("perf_ping");
            const rtt = performance.now() - p;
            sum += rtt;
            if (rtt < min) {
              min = rtt;
            }
          }
          await invoke("perf_report", {
            text: `IPC 自测：前端收到 ${(received / 1_048_576).toFixed(1)}MB 耗时 ${secs.toFixed(2)}s（${mibs.toFixed(0)} MiB/s），渲染器 ${renderer}；invoke 往返最小 ${min.toFixed(1)}ms 平均 ${(sum / 30).toFixed(1)}ms`,
          });
          const parseTimes: number[] = [];
          const arrivalAt: number[] = [];
          const burstDone = new Promise<void>((resolve) => {
            const channel = new Channel<unknown>();
            channel.onmessage = (raw) => {
              const arr = new Uint8Array(raw as ArrayBuffer);
              const seq = Number(
                new DataView(arr.buffer, arr.byteOffset + 1).getBigUint64(0),
              );
              arrivalAt.push(performance.now());
              const t = performance.now();
              const scratch = new Terminal({ scrollback: 1 });
              scratch.write(arr.subarray(17), () => {
                parseTimes.push(performance.now() - t);
                scratch.dispose();
              });
              if (seq >= 299) {
                resolve();
              }
            };
            void invoke("perf_replay_burst", { onData: channel });
          });
          const tBurst = performance.now();
          await burstDone;
          const burstSpan = performance.now() - tBurst;
          const gaps: number[] = [];
          for (let i = 1; i < arrivalAt.length; i += 1) {
            gaps.push(arrivalAt[i] - arrivalAt[i - 1]);
          }
          gaps.sort((a, b) => a - b);
          const gapMed = gaps.length ? gaps[Math.floor(gaps.length / 2)].toFixed(1) : "0";
          const gapMax = gaps.length ? gaps[gaps.length - 1].toFixed(1) : "0";
          const parseMax = parseTimes.length ? Math.max(...parseTimes).toFixed(1) : "0";
          await invoke("perf_report", {
            text: `突发回放：内核→前端接收跨度 ${burstSpan.toFixed(0)}ms（300 块 8KB），到达间隔中位 ${gapMed}ms 最大 ${gapMax}ms，xterm 解析最大 ${parseMax}ms`,
          });
        } catch {
          // 自测失败不影响正常使用
        }
      })();
    }, 1500);
    return () => window.clearTimeout(timer);
  }, []);

  const updateTab = (key: string, patch: (t: Tab) => Tab) => {
    setTabs((prev) => prev.map((t) => (t.key === key ? patch(t) : t)));
  };

  const connect = (host: Host) => {
    const key = `${host.id}-${Date.now()}`;
    const paneKey = `${key}-p0`;
    setTabs((prev) => [
      ...prev,
      {
        key,
        kind: "terminal",
        host,
        panes: [{ key: paneKey, sessionId: null }],
        splitDir: "horizontal",
        broadcast: false,
      },
    ]);
    setActiveKey(key);
    setFocusedPane(paneKey);
    setView("term");
    setRightPanel(null);
  };

  const closePaneSession = (paneKey: string) => {
    const sid = sessionIds.current[paneKey];
    if (sid !== undefined) {
      const owner = tabs.find((t) => t.panes.some((p) => p.key === paneKey));
      void import("./lib/ipc").then(({ sessionClose, localClose }) =>
        void (owner?.kind === "local" ? localClose(sid) : sessionClose(sid)),
      );
      delete sessionIds.current[paneKey];
    }
  };

  const closeTab = (key: string) => {
    const tab = tabs.find((t) => t.key === key);
    if (tab) {
      for (const pane of tab.panes) {
        closePaneSession(pane.key);
      }
    }
    setTabs((prev) => {
      const next = prev.filter((t) => t.key !== key);
      if (activeKey === key) {
        setActiveKey(next.length > 0 ? next[next.length - 1].key : null);
        setFocusedPane(null);
      }
      return next;
    });
    setRecording((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
  };

  const toggleSplit = () => {
    const tab = activeTab;
    if (!tab || tab.kind !== "terminal" || !tab.host) {
      return;
    }
    if (tab.panes.length === 1) {
      const paneKey = `${tab.key}-p${Date.now()}`;
      updateTab(tab.key, (t) => ({
        ...t,
        panes: [...t.panes, { key: paneKey, sessionId: null }],
      }));
      setFocusedPane(paneKey);
      setNotice(`已分屏：第二格将建立到 ${tab.host.name} 的新会话`);
    } else {
      const removed = tab.panes[tab.panes.length - 1];
      closePaneSession(removed.key);
      updateTab(tab.key, (t) => ({ ...t, panes: t.panes.slice(0, -1) }));
      setFocusedPane(tab.panes[0].key);
    }
  };

  const toggleSplitDir = () => {
    const tab = activeTab;
    if (!tab || tab.kind !== "terminal") {
      return;
    }
    updateTab(tab.key, (t) => ({
      ...t,
      splitDir: t.splitDir === "horizontal" ? "vertical" : "horizontal",
    }));
  };

  const toggleBroadcast = () => {
    const tab = activeTab;
    if (!tab || tab.kind !== "terminal") {
      return;
    }
    updateTab(tab.key, (t) => ({ ...t, broadcast: !t.broadcast }));
    setNotice(
      tab.broadcast ? "广播输入已关闭" : "广播输入已开启：键盘内容同步到本标签全部分屏",
    );
  };

  const broadcastInput = (
    tabKey: string,
    text: string,
    send: (payload: Uint8Array, sessionId: number) => void,
  ) => {
    const tab = tabs.find((t) => t.key === tabKey);
    if (!tab) {
      return;
    }
    const bytes = new TextEncoder().encode(text);
    for (const pane of tab.panes) {
      const sid = sessionIds.current[pane.key];
      if (sid !== undefined) {
        send(bytes, sid);
      }
    }
  };

  const openReplayTab = async () => {
    const { recordList } = await import("./lib/ipc");
    const files = await recordList();
    if (files.length === 0) {
      setNotice("还没有录制文件");
      return;
    }
    const pick = window.prompt(
      `输入要回放的文件序号（共 ${files.length} 个）：\n${files.map((f, i) => `${i + 1}. ${f}`).join("\n")}`,
      "1",
    );
    const idx = Number(pick);
    if (!Number.isInteger(idx) || idx < 1 || idx > files.length) {
      return;
    }
    const file = files[idx - 1];
    const key = `replay-${Date.now()}`;
    setTabs((prev) => [
      ...prev,
      { key, kind: "replay", file, panes: [], splitDir: "horizontal", broadcast: false },
    ]);
    setActiveKey(key);
  };

  const toggleRecord = () => {
    const tab = activeTab;
    const sid = activeSessionId;
    if (!tab || tab.kind !== "terminal" || sid === null) {
      return;
    }
    if (recording[tab.key]) {
      void recordStop(sid).then(() => {
        setRecording((prev) => ({ ...prev, [tab.key]: false }));
        setNotice("录制已保存到数据目录 recordings/ 下");
      });
    } else {
      void recordStart(sid).then((path) => {
        setRecording((prev) => ({ ...prev, [tab.key]: true }));
        setNotice(`录制中：${path}`);
      });
    }
  };

  const toggleSessionLog = () => {
    const tab = activeTab;
    const sid = activeSessionId;
    if (!tab || tab.kind !== "terminal" || sid === null) {
      return;
    }
    const current = logging[tab.key];
    if (current) {
      void sessionLogStop(sid).then(() => {
        setLogging((prev) => {
          const next = { ...prev };
          delete next[tab.key];
          return next;
        });
        setNotice(`会话日志已停止，文件：${current}`);
      });
    } else {
      void sessionLogStart(sid).then((path) => {
        setLogging((prev) => ({ ...prev, [tab.key]: path }));
        setNotice(`会话日志记录中：${path}`);
      });
    }
  };

  const loadLogs = () => {
    void import("./lib/ipc").then(({ logTail }) =>
      logTail(200).then(setLogLines).catch(() => setLogLines([])),
    );
  };

  useEffect(() => {
    if (view === "logs") {
      loadLogs();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  return (
    <div className="flex h-full pt-[28px]">
      <Sidebar
        view={view}
        onNav={(v) => {
          if (v === "term") {
            setView("term");
          } else if (v === "logs") {
            setView("logs");
            loadLogs();
          } else {
            setView(v);
          }
        }}
        onOpenSettings={() => setSettingsOpen(true)}
        termCount={tabs.filter((t) => t.kind === "terminal" || t.kind === "local").length}
        collapsed={sidebarCollapsed}
        onToggleCollapsed={() => setSidebarCollapsed((v) => !v)}
      />

      <div className="relative min-w-0 flex-1">
        {view === "home" && (
          <div className="absolute inset-0 overflow-y-auto p-6">
            <div className="mb-5 flex items-center justify-between">
              <h1 className="text-xl font-semibold">主机</h1>
              <div className="flex gap-2">
                <button className="sbtn" onClick={() => setImportOpen(true)}>
                  导入
                </button>
                <button className="sbtn sbtn-primary" onClick={() => { setEditingHost(null); setHostFormOpen(true); }}>
                  新建主机
                </button>
              </div>
            </div>
            <div className="mb-4 flex gap-2">
              <input
                className="sinput"
                placeholder="搜索主机或 ssh user@hostname…"
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    const first = filteredHosts[0];
                    if (first) {
                      connect(first);
                    }
                  }
                }}
              />
              <button
                className="sbtn sbtn-primary"
                disabled={filteredHosts.length === 0}
                onClick={() => {
                  const first = filteredHosts[0];
                  if (first) {
                    connect(first);
                  }
                }}
              >
                连接
              </button>
            </div>
            {filteredHosts.length === 0 && (
              <p className="text-sm text-[var(--faint)]">没有匹配的主机，点右上角「新建主机」添加。</p>
            )}
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {filteredHosts.map((host) => {
                const hue = hostHue(host.name);
                return (
                  <div
                    key={host.id}
                    className="group relative flex cursor-pointer items-center gap-3 rounded-xl border border-[var(--border)] bg-[var(--panel)] p-4 transition-shadow hover:shadow-md"
                    onClick={() => connect(host)}
                  >
                    <div
                      className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl text-lg font-bold text-white"
                      style={{
                        background: `linear-gradient(135deg, hsl(${hue},72%,55%), hsl(${(hue + 40) % 360},72%,45%))`,
                      }}
                    >
                      {host.name.slice(0, 1).toUpperCase()}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[14px] font-medium">{host.name}</div>
                      <div className="truncate text-xs text-[var(--dim)]">ssh, {host.username}</div>
                      <div className="mt-0.5 flex gap-2 text-[11px] text-[var(--faint)]">
                        {host.compatMode && <span className="text-amber-600">兼容</span>}
                        {host.compress && <span>压缩</span>}
                        {host.encoding !== "utf8" && <span>{host.encoding}</span>}
                      </div>
                    </div>
                    <div className="hidden gap-1 group-hover:flex">
                      <button
                        className="rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
                        onClick={(e) => {
                          e.stopPropagation();
                          setForwardHost(host);
                        }}
                      >
                        转发
                      </button>
                      <button
                        className="rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-[var(--text)]"
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingHost(host);
                          setHostFormOpen(true);
                        }}
                      >
                        编辑
                      </button>
                      <button
                        className="rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-red-500"
                        onClick={(e) => {
                          e.stopPropagation();
                          void deleteHost(host.id).then(() => refresh());
                        }}
                      >
                        删除
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {view === "known" && (
          <div className="absolute inset-0 flex flex-col">
            <KnownHostsView />
          </div>
        )}

        {view === "tunnels" && (
          <div className="absolute inset-0 flex flex-col">
            <TunnelCenter />
          </div>
        )}

        {view === "snippets" && (
          <div className="absolute inset-0 flex flex-col">
            <SnippetsView
              hasSession={activeSessionId !== null}
              onSendToTerminal={(content) => {
                const sid = activeSessionId;
                if (sid !== null) {
                  void import("./lib/ipc").then(({ sessionWrite }) =>
                    sessionWrite(sid, new TextEncoder().encode(content)),
                  );
                }
              }}
            />
          </div>
        )}

        {view === "logs" && (
          <div className="absolute inset-0 flex flex-col bg-[var(--panel)]">
            <div className="flex items-center justify-between border-b border-[var(--border)] px-5 py-3">
              <h1 className="text-base font-semibold">连接日志</h1>
              <button className="sbtn" onClick={loadLogs}>
                刷新
              </button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-4">
              {logLines.length === 0 && <p className="text-xs text-[var(--faint)]">暂无日志</p>}
              {logLines.map((line, i) => (
                <div key={i} className="whitespace-pre-wrap font-mono text-xs leading-5 text-[#3c4149]">
                  {line}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* 终端工作区常驻挂载：切到其他页面时仅隐藏，会话保持在线 */}
        <div
          className="absolute inset-0 flex flex-col bg-[var(--ws-bg)]"
          style={{ display: view === "term" ? "flex" : "none" }}
        >
            <div className="relative z-40 flex h-10 items-center gap-1 overflow-x-auto border-b border-[var(--ws-border)] bg-[var(--ws-chrome)] px-2">
              {tabs.map((tab) => (
                <div
                  key={tab.key}
                  className={`group flex max-w-52 shrink-0 items-center rounded-lg px-3 py-1.5 text-xs ${
                    tab.key === activeKey
                      ? "bg-[var(--ws-bg)] text-[var(--ws-text)] ring-1 ring-[var(--ws-border)]"
                      : "text-[var(--ws-text-dim)] hover:text-[var(--ws-text)]"
                  }`}
                >
                  <button className="min-w-0 flex-1 truncate" onClick={() => setActiveKey(tab.key)}>
                    {tab.kind === "replay" ? "回放 · " : ""}
                    {tab.kind === "local" ? "本地终端" : (tab.host?.name ?? tab.file?.split("/").pop())}
                    {recording[tab.key] && <span className="ml-1 text-red-400">●</span>}
                    {tab.panes.length > 1 && <span className="ml-1 text-sky-400">⧉</span>}
                  </button>
                  <button
                    className="ml-2 hidden text-[var(--ws-text-dim)] hover:text-red-400 group-hover:block"
                    onClick={() => closeTab(tab.key)}
                  >
                    ✕
                  </button>
                </div>
              ))}
              <div className="flex-1" />
              {activeTab?.kind === "terminal" && activeTab.host && (
                <>
                  <button
                    className="mr-1 shrink-0 rounded-md bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                    onClick={toggleSplitDir}
                  >
                    {activeTab.splitDir === "horizontal" ? "左右" : "上下"}
                  </button>
                  <button
                    className="mr-1 shrink-0 rounded-md bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                    onClick={toggleSplit}
                  >
                    {activeTab.panes.length === 1 ? "分屏" : "取消分屏"}
                  </button>
                  {activeTab.panes.length > 1 && (
                    <button
                      className={`mr-1 shrink-0 rounded-md px-2 py-1 text-xs ${
                        activeTab.broadcast ? "bg-emerald-700 text-white" : "bg-[var(--ws-btn)] text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                      }`}
                      onClick={toggleBroadcast}
                    >
                      广播
                    </button>
                  )}
                  <button
                    className={`mr-1 shrink-0 rounded-md px-2 py-1 text-xs ${
                      stealth[activeTab.key] ? "bg-purple-700 text-white" : "bg-[var(--ws-btn)] text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                    }`}
                    onClick={() => {
                      const next = !stealth[activeTab.key];
                      setStealth((prev) => ({ ...prev, [activeTab.key]: next }));
                      setNotice(next ? "隐身模式已开启" : "隐身模式已关闭");
                    }}
                  >
                    {stealth[activeTab.key] ? "退出隐身" : "隐身"}
                  </button>
                  <button
                    className={`mr-1 shrink-0 rounded-md px-2 py-1 text-xs ${
                      logging[activeTab.key]
                        ? "bg-amber-600 text-white"
                        : "bg-[var(--ws-btn)] text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                    }`}
                    title={logging[activeTab.key] ? `日志文件：${logging[activeTab.key]}` : "终端输出逐字落盘"}
                    onClick={toggleSessionLog}
                  >
                    {logging[activeTab.key] ? "日志●" : "日志"}
                  </button>
                  <button
                    className={`mr-1 shrink-0 rounded-md px-2 py-1 text-xs ${
                      recording[activeTab.key] ? "bg-red-700 text-white" : "bg-[var(--ws-btn)] text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                    }`}
                    onClick={toggleRecord}
                  >
                    {recording[activeTab.key] ? "停止录制" : "录制"}
                  </button>
                </>
              )}
              <button
                className="shrink-0 rounded-md bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                onClick={() => {
                  const key = `local-${Date.now()}`;
                  const paneKey = `${key}-p0`;
                  setTabs((prev) => [
                    ...prev,
                    { key, kind: "local", panes: [{ key: paneKey, sessionId: null }], splitDir: "horizontal", broadcast: false },
                  ]);
                  setActiveKey(key);
                  setFocusedPane(paneKey);
                }}
              >
                本地
              </button>
              <button
                className={`shrink-0 rounded-md px-2 py-1 text-xs ${
                  rightPanel === "sftp" ? "bg-[var(--accent)] text-white" : "bg-[var(--ws-btn)] text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                }`}
                onClick={() => setRightPanel(rightPanel === "sftp" ? null : "sftp")}
              >
                文件
              </button>
              <button
                className={`ml-1 shrink-0 rounded-md px-2 py-1 text-xs ${
                  rightPanel === "ai" ? "bg-[var(--accent)] text-white" : "bg-[var(--ws-btn)] text-[var(--ws-text)] hover:bg-[var(--ws-btn-hover)]"
                }`}
                onClick={() => setRightPanel(rightPanel === "ai" ? null : "ai")}
              >
                AI
              </button>
            </div>

            {notice && (
              <div className="flex items-center gap-2 border-b border-[var(--ws-border)] bg-[var(--ws-chrome)] px-3 py-1 text-xs text-[var(--ws-text-dim)]">
                <span className="min-w-0 flex-1 truncate">{notice}</span>
                <button className="text-[var(--ws-text-dim)] hover:text-zinc-200" onClick={() => setNotice(null)}>
                  ✕
                </button>
              </div>
            )}

            <div className="relative min-h-0 flex-1">
              {tabs.length === 0 && (
                <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-center">
                  <p className="text-sm text-[var(--ws-text-dim)]">还没有打开的终端</p>
                  <p className="text-xs text-[var(--faint)]">
                    回到「主机」点击卡片连接，或点右上角「本地」打开本机终端
                  </p>
                </div>
              )}
              {tabs.map((tab) => {
                const visible = tab.key === activeKey;
                if (tab.kind === "replay") {
                  return (
                    <div
                      key={tab.key}
                      className="absolute inset-0"
                      style={{ display: visible ? "block" : "none" }}
                    >
                      <ReplayView file={tab.file ?? ""} active={visible} />
                    </div>
                  );
                }
                const vertical = tab.splitDir === "vertical";
                return (
                  <div
                    key={tab.key}
                    className={`absolute inset-0 flex ${vertical ? "flex-col" : "flex-row"}`}
                    style={{ display: visible ? "flex" : "none" }}
                  >
                    {tab.panes.map((pane, i) => (
                      <div
                        key={pane.key}
                        className={`relative min-h-0 min-w-0 flex-1 ${i > 0 ? (vertical ? "border-t border-[var(--ws-border)]" : "border-l border-[var(--ws-border)]") : ""} ${pane.key === focusedPane && tab.panes.length > 1 ? "ring-1 ring-inset ring-sky-700" : ""}`}
                        onMouseDown={() => setFocusedPane(pane.key)}
                      >
                        {tab.panes.length > 1 && (
                          <span className="absolute right-2 top-1 z-10 rounded bg-black/50 px-1 text-xs text-[var(--ws-text-dim)]">
                            {tab.host?.name} {i + 1}
                            {tab.broadcast && " · 广播中"}
                          </span>
                        )}
                        {(tab.host || tab.kind === "local") && (
                          <TerminalView
                            host={tab.host ?? null}
                            local={tab.kind === "local"}
                            tabKey={pane.key}
                            active={visible && pane.key === focusedPane}
                            stealth={stealth[tab.key] ?? false}
                            onSessionReady={(sid) => {
                              sessionIds.current[pane.key] = sid;
                              if (pane.key === activePaneKey) {
                                setSessionTick((v) => v + 1);
                              }
                            }}
                            broadcast={tab.broadcast}
                            onInput={(text, send) => broadcastInput(tab.key, text, send)}
                          />
                        )}
                      </div>
                    ))}
                  </div>
                );
              })}
            </div>
          </div>
      </div>

      <div
        className="absolute right-0 top-10 bottom-0 z-30 flex-col"
        style={{ display: view === "term" && rightPanel === "ai" ? "flex" : "none" }}
      >
        <AiDrawer
          sessionId={activeSessionId}
          host={activeTab?.host ?? null}
          getSelection={() => (activeTab && activePaneKey ? getSelection(activePaneKey) : "")}
          onNotice={setNotice}
        />
      </div>

      <div
        className="absolute right-0 top-10 bottom-0 z-30 flex-col"
        style={{ display: view === "term" && rightPanel === "sftp" ? "flex" : "none" }}
      >
        <SftpPanel sessionId={activeSessionId} />
      </div>

      {paletteOpen && (
        <CommandPalette
          hosts={hosts}
          onClose={() => setPaletteOpen(false)}
          onConnect={connect}
          onOpenAi={() => {
            setView("term");
            setRightPanel("ai");
          }}
          onOpenSettings={() => setSettingsOpen(true)}
          onOpenSftp={() => {
            setView("term");
            setRightPanel("sftp");
          }}
          onOpenReplay={() => void openReplayTab()}
        />
      )}

      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
      {forwardHost && <ForwardPanel host={forwardHost} onClose={() => setForwardHost(null)} />}
      {hostFormOpen && (
        <HostForm
          editing={editingHost}
          onClose={() => setHostFormOpen(false)}
          onSaved={() => {
            setHostFormOpen(false);
            refresh();
          }}
        />
      )}
      {importOpen && (
        <ImportDialog
          onClose={() => setImportOpen(false)}
          onImported={() => refresh()}
        />
      )}
      <McpApprovals />
    </div>
  );
}
