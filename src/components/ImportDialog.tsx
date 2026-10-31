import { useState } from "react";
import SidePanel from "./SidePanel";
import { importHosts, type ImportKind } from "../lib/ipc";

interface Props {
  onClose: () => void;
  onImported: () => void;
}

export default function ImportDialog({ onClose, onImported }: Props) {
  const [kind, setKind] = useState<ImportKind>("ssh_config");
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);

  const hint =
    kind === "ssh_config"
      ? "OpenSSH 配置文件路径，例如 ~/.ssh/config（自动跳过通配条目）"
      : kind === "xshell"
        ? "Xshell 会话目录路径，导入其中全部 .xsh 文件"
        : "Termius 导出的 JSON 文件路径（设置 → 导出）";

  const run = () => {
    if (!path.trim() || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    importHosts(kind, path.trim())
      .then((count) => {
        setResult(`导入完成：${count} 台主机`);
        setBusy(false);
        onImported();
      })
      .catch((err) => {
        setError(String(err));
        setBusy(false);
      });
  };

  return (
    <SidePanel
      title="导入主机"
      subtitle="支持 OpenSSH config、Xshell、Termius 三种来源"
      onClose={onClose}
      footer={
        <div className="flex gap-2">
          <button className="sbtn flex-1" onClick={onClose}>
            关闭
          </button>
          <button
            className="sbtn sbtn-primary flex-1"
            disabled={busy || !path.trim()}
            onClick={run}
          >
            {busy ? "导入中" : "导入"}
          </button>
        </div>
      }
    >
      <div className="ssection mb-3">
        <div className="stitle">来源格式</div>
        <select
          className="sinput"
          value={kind}
          onChange={(e) => setKind(e.target.value as ImportKind)}
        >
          <option value="ssh_config">OpenSSH config</option>
          <option value="xshell">Xshell 会话目录</option>
          <option value="termius">Termius 导出 JSON</option>
        </select>
        <p className="mt-2 text-xs leading-4 text-[var(--dim)]">{hint}</p>
      </div>
      <div className="ssection">
        <div className="stitle">路径</div>
        <input
          className="sinput"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder={kind === "ssh_config" ? "~/.ssh/config" : "/path/to/folder"}
        />
      </div>

      {error && <p className="mt-3 text-xs text-red-500">{error}</p>}
      {result && <p className="mt-3 text-xs text-[var(--ok)]">{result}</p>}
    </SidePanel>
  );
}
