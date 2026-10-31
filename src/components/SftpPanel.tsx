import { useEffect, useState } from "react";
import {
  localList,
  localMkdir,
  localRemove,
  onSftpDone,
  onSftpProgress,
  sftpDownload,
  sftpList,
  sftpMkdir,
  sftpRemove,
  sftpRename,
  sftpUpload,
  type FsEntry,
  type SftpProgress,
} from "../lib/ipc";

interface Props {
  sessionId: number | null;
}

function humanSize(n: number): string {
  if (n < 1024) {
    return `${n} B`;
  }
  const units = ["KB", "MB", "GB", "TB"];
  let v = n;
  let i = -1;
  do {
    v /= 1024;
    i += 1;
  } while (v >= 1024 && i < units.length - 1);
  return `${v.toFixed(1)} ${units[i]}`;
}

const inputCls =
  "w-full rounded border border-[var(--border)] bg-[var(--ws-btn)] px-2 py-1 text-[13px] text-[var(--text)] outline-none focus:border-sky-600";

export default function SftpPanel({ sessionId }: Props) {
  const [localPath, setLocalPath] = useState(
    () => window.localStorage.getItem("s-shell-local-base") ?? "/tmp",
  );
  const [localEntries, setLocalEntries] = useState<FsEntry[]>([]);
  const [selectedLocal, setSelectedLocal] = useState<FsEntry | null>(null);
  const [remotePath, setRemotePath] = useState("/");
  const [remoteEntries, setRemoteEntries] = useState<FsEntry[]>([]);
  const [selectedRemote, setSelectedRemote] = useState<FsEntry | null>(null);
  const [resume, setResume] = useState(true);
  const [limitKbps, setLimitKbps] = useState("");
  const [renamingRemote, setRenamingRemote] = useState<{ from: string; to: string } | null>(null);
  const [mkdirLocalName, setMkdirLocalName] = useState("");
  const [mkdirRemoteName, setMkdirRemoteName] = useState("");
  const [progress, setProgress] = useState<SftpProgress[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const loadLocal = (target: string) => {
    void localList(target)
      .then((list) => {
        setLocalEntries(list);
        setLocalPath(target);
        setSelectedLocal(null);
        window.localStorage.setItem("s-shell-local-base", target);
      })
      .catch((err) => setError(String(err)));
  };

  const loadRemote = (target: string) => {
    if (sessionId === null) {
      return;
    }
    setError(null);
    void sftpList(sessionId, target)
      .then((list) => {
        setRemoteEntries(list);
        setRemotePath(target);
        setSelectedRemote(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    loadLocal(localPath);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    loadRemote(remotePath);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  useEffect(() => {
    let alive = true;
    const unlistenP = onSftpProgress((p) => {
      if (alive) {
        setProgress((prev) => [...prev.filter((x) => x.taskId !== p.taskId), p]);
      }
    });
    const unlistenD = onSftpDone((r) => {
      if (!alive) {
        return;
      }
      setProgress((prev) => prev.filter((x) => x.taskId !== r.taskId));
      if (r.ok) {
        setMessage("传输完成");
        loadRemote(remotePath);
        loadLocal(localPath);
      } else {
        setError(r.error ?? "传输失败");
      }
    });
    return () => {
      alive = false;
      void unlistenP.then((fn) => fn());
      void unlistenD.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, remotePath, localPath]);

  if (sessionId === null) {
    return (
      <aside className="ssh-drawer flex h-full w-[640px] flex-col border-l border-[var(--ws-border)] bg-[var(--ws-bg)] p-4">
        <h2 className="text-sm font-semibold text-[var(--text)]">文件管理</h2>
        <p className="mt-4 text-xs leading-5 text-[var(--dim)]">先连接一台主机，再打开文件管理。</p>
      </aside>
    );
  }

  const parentOf = (p: string) => {
    const trimmed = p.replace(/\/+$/, "");
    const idx = trimmed.lastIndexOf("/");
    return idx <= 0 ? "/" : trimmed.slice(0, idx);
  };
  const join = (dir: string, name: string) => `${dir.replace(/\/+$/, "")}/${name}`;

  const doUpload = (localFile: string) => {
    void sftpUpload(sessionId, localFile, remotePath, resume, Number(limitKbps) || 0).then(
      (taskId) => setMessage(`上传任务 ${taskId} 已开始`),
    );
  };
  const doDownload = (remoteFile: string) => {
    const base = localPath;
    const name = remoteFile.split("/").pop() ?? "file";
    void sftpDownload(
      sessionId,
      remoteFile,
      join(base, name),
      resume,
      Number(limitKbps) || 0,
    ).then((taskId) => setMessage(`下载任务 ${taskId} 已开始 → ${join(base, name)}`));
  };

  const dirProps = (side: "local" | "remote") => ({
    onDragOver: (e: React.DragEvent) => e.preventDefault(),
    onDrop: (e: React.DragEvent) => {
      e.preventDefault();
      const localFile = e.dataTransfer.getData("application/x-sshell-local");
      const remoteFile = e.dataTransfer.getData("application/x-sshell-remote");
      if (side === "remote" && localFile) {
        doUpload(localFile);
      } else if (side === "local" && remoteFile) {
        doDownload(remoteFile);
      }
    },
  });

  const fileRows = (entries: FsEntry[], side: "local" | "remote") =>
    entries.map((entry, idx) => {
      const selected = side === "local" ? selectedLocal : selectedRemote;
      const isSelected = selected?.name === entry.name;
      const fullPath = side === "local" ? join(localPath, entry.name) : join(remotePath, entry.name);
      return (
        <div
          key={`${entry.name}-${idx}`}
          draggable
          onDragStart={(e) => {
            e.dataTransfer.setData(
              side === "local" ? "application/x-sshell-local" : "application/x-sshell-remote",
              fullPath,
            );
          }}
          onClick={() => {
            if (side === "local") {
              setSelectedLocal(entry);
            } else {
              setSelectedRemote(entry);
            }
          }}
          onDoubleClick={() => {
            if (entry.isDir) {
              if (side === "local") {
                loadLocal(fullPath);
              } else {
                loadRemote(fullPath);
              }
              return;
            }
            if (side === "remote") {
              doDownload(fullPath);
            }
          }}
          className={`flex cursor-pointer items-center rounded px-2 py-1 text-xs ${
            isSelected ? "bg-sky-900/50 text-white" : "text-[var(--text)] hover:bg-[var(--ws-btn)]"
          }`}
        >
          <span className={entry.isDir ? "text-[var(--accent)]" : "text-[var(--dim)]"}>
            {entry.isDir ? "目录" : "文件"}
          </span>
          <span className="ml-2 min-w-0 flex-1 truncate">{entry.name}</span>
          <span className="ml-2 shrink-0 text-[var(--faint)]">{humanSize(entry.size)}</span>
        </div>
      );
    });

  return (
    <aside className="ssh-drawer flex h-full w-[720px] flex-col border-l border-[var(--ws-border)] bg-[var(--ws-bg)]">
      <div className="flex items-center justify-between px-3 py-2">
        <h2 className="text-sm font-semibold text-[var(--text)]">文件管理（SFTP 双栏）</h2>
        <label className="flex items-center gap-1 text-xs text-[var(--dim)]">
          <input type="checkbox" checked={resume} onChange={(e) => setResume(e.target.checked)} />
          断点续传
        </label>
        <input
          className="w-24 rounded border border-[var(--border)] bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)]"
          placeholder="限速 KB/s，0 不限"
          value={limitKbps}
          onChange={(e) => setLimitKbps(e.target.value)}
        />
      </div>

      <div className="flex min-h-0 flex-1">
        {/* 本地栏 */}
        <div className="flex min-w-0 flex-1 flex-col border-r border-[var(--ws-border)]" {...dirProps("local")}>
          <div className="flex flex-wrap gap-1 p-2">
            <button
              className="rounded bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)] hover:bg-[var(--ws-btn-hover)]"
              onClick={() => loadLocal(parentOf(localPath))}
            >
              上一级
            </button>
            <input
              className={inputCls}
              value={localPath}
              onChange={(e) => setLocalPath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  loadLocal(localPath);
                }
              }}
            />
          </div>
          <div className="flex flex-wrap gap-1 px-2 pb-2">
            <input
              className={inputCls}
              placeholder="新建目录名"
              value={mkdirLocalName}
              onChange={(e) => setMkdirLocalName(e.target.value)}
            />
            <button
              className="shrink-0 rounded bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)] hover:bg-[var(--ws-btn-hover)] disabled:opacity-50"
              disabled={!mkdirLocalName.trim()}
              onClick={() => {
                void localMkdir(join(localPath, mkdirLocalName.trim())).then(() => {
                  setMkdirLocalName("");
                  loadLocal(localPath);
                });
              }}
            >
              新建
            </button>
            <button
              className="shrink-0 rounded bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)] hover:text-red-300 disabled:opacity-50"
              disabled={!selectedLocal}
              onClick={() => {
                if (selectedLocal && window.confirm(`删除本地 ${selectedLocal.name}？`)) {
                  void localRemove(join(localPath, selectedLocal.name), selectedLocal.isDir).then(() =>
                    loadLocal(localPath),
                  );
                }
              }}
            >
              删除
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            {fileRows(localEntries, "local")}
          </div>
          <div className="border-t border-[var(--ws-border)] px-2 py-1 text-xs text-[var(--dim)]">
            单击选中；双击目录进入；拖拽本地文件到右侧上传
          </div>
        </div>

        {/* 传输按钮列 */}
        <div className="flex w-14 shrink-0 flex-col items-center justify-center gap-2 border-r border-[var(--ws-border)]">
          <button
            className="rounded bg-sky-800 px-1.5 py-1 text-xs text-white hover:bg-sky-700 disabled:opacity-50"
            disabled={!selectedLocal || selectedLocal.isDir}
            title="上传选中文件到远端当前目录"
            onClick={() => doUpload(join(localPath, selectedLocal!.name))}
          >
            →
          </button>
          <button
            className="rounded bg-sky-800 px-1.5 py-1 text-xs text-white hover:bg-sky-700 disabled:opacity-50"
            disabled={!selectedRemote || selectedRemote.isDir}
            title="下载选中文件到本地当前目录"
            onClick={() => doDownload(join(remotePath, selectedRemote!.name))}
          >
            ←
          </button>
        </div>

        {/* 远端栏 */}
        <div className="flex min-w-0 flex-1 flex-col" {...dirProps("remote")}>
          <div className="flex flex-wrap gap-1 p-2">
            <button
              className="rounded bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)] hover:bg-[var(--ws-btn-hover)]"
              onClick={() => loadRemote(parentOf(remotePath))}
            >
              上一级
            </button>
            <input
              className={inputCls}
              value={remotePath}
              onChange={(e) => setRemotePath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  loadRemote(remotePath);
                }
              }}
            />
            <button
              className="shrink-0 rounded bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)] hover:bg-[var(--ws-btn-hover)]"
              onClick={() => loadRemote(remotePath)}
            >
              刷新
            </button>
          </div>
          <div className="flex flex-wrap gap-1 px-2 pb-2">
            <input
              className={inputCls}
              placeholder="新建目录名"
              value={mkdirRemoteName}
              onChange={(e) => setMkdirRemoteName(e.target.value)}
            />
            <button
              className="shrink-0 rounded bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)] hover:bg-[var(--ws-btn-hover)] disabled:opacity-50"
              disabled={!mkdirRemoteName.trim()}
              onClick={() => {
                void sftpMkdir(sessionId, join(remotePath, mkdirRemoteName.trim())).then(() => {
                  setMkdirRemoteName("");
                  loadRemote(remotePath);
                });
              }}
            >
              新建
            </button>
            <button
              className="shrink-0 rounded bg-[var(--ws-btn)] px-2 py-1 text-xs text-[var(--text)] hover:text-red-300 disabled:opacity-50"
              disabled={!selectedRemote}
              onClick={() => {
                if (selectedRemote && window.confirm(`删除远端 ${selectedRemote.name}？`)) {
                  void sftpRemove(sessionId, join(remotePath, selectedRemote.name), selectedRemote.isDir).then(
                    () => loadRemote(remotePath),
                  );
                }
              }}
            >
              删除
            </button>
          </div>
          {renamingRemote && (
            <div className="flex flex-wrap gap-1 px-2 pb-2">
              <input
                className={inputCls}
                value={renamingRemote.to}
                onChange={(e) => setRenamingRemote({ ...renamingRemote, to: e.target.value })}
              />
              <button
                className="shrink-0 rounded bg-sky-800 px-2 text-xs text-white"
                onClick={() => {
                  void sftpRename(
                    sessionId,
                    join(remotePath, renamingRemote.from),
                    join(remotePath, renamingRemote.to),
                  ).then(() => {
                    setRenamingRemote(null);
                    loadRemote(remotePath);
                  });
                }}
              >
                确定
              </button>
              <button
                className="shrink-0 rounded bg-[var(--ws-btn)] px-2 text-xs text-[var(--text)]"
                onClick={() => setRenamingRemote(null)}
              >
                取消
              </button>
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            {fileRows(remoteEntries, "remote")}
            {selectedRemote && !renamingRemote && (
              <button
                className="mt-1 w-full rounded px-2 py-1 text-left text-xs text-[var(--dim)] hover:bg-[var(--ws-btn)] hover:text-[var(--text)]"
                onClick={() => setRenamingRemote({ from: selectedRemote.name, to: selectedRemote.name })}
              >
                重命名「{selectedRemote.name}」…
              </button>
            )}
          </div>
          <div className="border-t border-[var(--ws-border)] px-2 py-1 text-xs text-[var(--dim)]">
            双击目录进入；双击文件下载到本地当前目录
          </div>
        </div>
      </div>

      {(error || message) && (
        <p className={`px-3 py-1 text-xs ${error ? "text-red-400" : "text-emerald-400"}`}>
          {error ?? message}
        </p>
      )}
      {progress.map((p) => (
        <div key={p.taskId} className="px-3 pb-2">
          <div className="mb-1 text-xs text-[var(--dim)]">
            任务 {p.taskId}：{humanSize(p.done)} / {humanSize(p.total)}
          </div>
          <div className="h-1 rounded bg-[var(--ws-btn)]">
            <div
              className="h-1 rounded bg-sky-600"
              style={{ width: `${p.total > 0 ? Math.min(100, (p.done / p.total) * 100) : 100}%` }}
            />
          </div>
        </div>
      ))}
    </aside>
  );
}

