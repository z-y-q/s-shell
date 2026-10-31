import { useEffect, useState } from "react";
import {
  forwardApply,
  forwardDelete,
  forwardList,
  forwardSave,
  type Forward,
  type ForwardKind,
  type Host,
} from "../lib/ipc";

interface Props {
  host: Host;
  onClose: () => void;
}

export default function ForwardPanel({ host, onClose }: Props) {
  const [rules, setRules] = useState<Forward[]>([]);
  const [kind, setKind] = useState<ForwardKind>("local");
  const [listenPort, setListenPort] = useState("");
  const [targetHost, setTargetHost] = useState("127.0.0.1");
  const [targetPort, setTargetPort] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    void forwardList(host.id).then(setRules);
  };

  useEffect(refresh, [host.id]);

  const inputCls =
    "w-full rounded border border-[var(--border)] bg-[var(--ws-btn)] px-2 py-1 text-[13px] text-[var(--text)] outline-none focus:border-sky-600";

  const add = () => {
    const port = Number(listenPort);
    const tPort = Number(targetPort);
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
      setError("监听端口必须是 1 到 65535 的整数");
      return;
    }
    if (kind !== "dynamic") {
      if (!targetHost.trim()) {
        setError("目标地址不能为空");
        return;
      }
      if (!Number.isInteger(tPort) || tPort <= 0 || tPort > 65535) {
        setError("目标端口必须是 1 到 65535 的整数");
        return;
      }
    }
    setError(null);
    void forwardSave(
      {
        hostId: host.id,
        kind,
        listenPort: port,
        targetHost: kind === "dynamic" ? "" : targetHost.trim(),
        targetPort: kind === "dynamic" ? 0 : tPort,
        enabled: true,
      },
      null,
    )
      .then(() => {
        setListenPort("");
        setTargetPort("");
        refresh();
      })
      .catch((err) => setError(String(err)));
  };

  const toggle = (rule: Forward) => {
    void forwardSave(
      {
        hostId: rule.hostId,
        kind: rule.kind,
        listenPort: rule.listenPort,
        targetHost: rule.targetHost,
        targetPort: rule.targetPort,
        enabled: !rule.enabled,
      },
      rule.id,
    )
      .then(refresh)
      .catch((err) => setError(String(err)));
  };

  const apply = () => {
    void forwardApply(host.id)
      .then((n) =>
        setMessage(n > 0 ? `已按当前规则重启 ${n} 条转发` : "当前没有在线会话，规则将在下次连接时生效"),
      )
      .catch((err) => setError(String(err)));
  };

  return (
    <div className="absolute inset-0 z-30 flex items-center justify-center bg-black/60">
      <div className="max-h-[90%] w-[520px] overflow-y-auto rounded-lg border border-[var(--border)] bg-[var(--ws-bg)] p-5">
        <h2 className="mb-1 text-base font-semibold">转发规则：{host.name}</h2>
        <p className="mb-4 text-xs text-[var(--dim)]">
          勾选的规则会在每次连接时自动启动；「应用到在线会话」立即重启。
        </p>

        {rules.length === 0 && <p className="mb-4 text-xs text-[var(--dim)]">还没有规则。</p>}
        {rules.map((rule) => (
          <div key={rule.id} className="mb-2 flex items-center gap-2 rounded border border-[var(--ws-border)] px-3 py-2">
            <input type="checkbox" checked={rule.enabled} onChange={() => toggle(rule)} />
            <span className="text-xs text-[var(--text)]">
              {rule.kind === "local" && `本地 ${rule.listenPort} → ${rule.targetHost}:${rule.targetPort}`}
              {rule.kind === "remote" && `远端 ${rule.listenPort} → 本侧 ${rule.targetHost}:${rule.targetPort}`}
              {rule.kind === "dynamic" && `动态 SOCKS5 ${rule.listenPort}`}
            </span>
            <div className="flex-1" />
            <button
              className="text-xs text-[var(--dim)] hover:text-red-400"
              onClick={() => void forwardDelete(rule.id).then(refresh)}
            >
              删除
            </button>
          </div>
        ))}

        <h3 className="mb-2 mt-4 text-sm font-semibold text-[var(--text)]">新增规则</h3>
        <div className="mb-2 flex gap-2">
          <select
            className={inputCls}
            value={kind}
            onChange={(e) => setKind(e.target.value as ForwardKind)}
          >
            <option value="local">本地转发（本机端口 → 远端目标）</option>
            <option value="remote">远程转发（服务器端口 → 本侧目标）</option>
            <option value="dynamic">动态 SOCKS5</option>
          </select>
          <input
            className={`${inputCls} w-28`}
            placeholder="监听端口"
            value={listenPort}
            onChange={(e) => setListenPort(e.target.value)}
          />
        </div>
        {kind !== "dynamic" && (
          <div className="mb-2 flex gap-2">
            <input
              className={inputCls}
              placeholder="目标地址"
              value={targetHost}
              onChange={(e) => setTargetHost(e.target.value)}
            />
            <input
              className={`${inputCls} w-28`}
              placeholder="目标端口"
              value={targetPort}
              onChange={(e) => setTargetPort(e.target.value)}
            />
          </div>
        )}
        <button
          className="rounded bg-[var(--ws-btn)] px-3 py-1.5 text-xs text-[var(--text)] hover:bg-[var(--ws-btn-hover)]"
          onClick={add}
        >
          添加规则
        </button>

        {(error || message) && (
          <p className={`mt-3 text-xs ${error ? "text-red-400" : "text-emerald-400"}`}>
            {error ?? message}
          </p>
        )}

        <div className="mt-4 flex justify-end gap-2">
          <button
            className="rounded border border-[var(--border)] px-3 py-1.5 text-sm text-[var(--text)] hover:bg-[var(--ws-btn)]"
            onClick={apply}
          >
            应用到在线会话
          </button>
          <button
            className="rounded bg-sky-700 px-3 py-1.5 text-sm text-white hover:bg-sky-600"
            onClick={onClose}
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
}
