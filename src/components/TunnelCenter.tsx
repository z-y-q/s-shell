import { useCallback, useEffect, useState } from "react";
import {
  checkPortFree,
  forwardActiveRules,
  forwardApply,
  forwardDelete,
  forwardListAll,
  forwardSave,
  forwardToggle,
  type ForwardRow,
  type Host,
} from "../lib/ipc";
import { useHosts } from "../stores/hosts";

const KIND_LABEL: Record<string, string> = {
  local: "本地 → 远端",
  remote: "远端 → 本地",
  dynamic: "动态 SOCKS5",
};

export default function TunnelCenter() {
  const { hosts } = useHosts();
  const [rules, setRules] = useState<ForwardRow[]>([]);
  const [active, setActive] = useState<number[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState({
    hostId: "",
    kind: "local",
    listenPort: "",
    targetHost: "127.0.0.1",
    targetPort: "",
  });
  const [portFree, setPortFree] = useState<boolean | null>(null);

  const load = useCallback(() => {
    void forwardListAll()
      .then((rows) => {
        setRules(rows);
        setError(null);
      })
      .catch((err) => setError(String(err)));
    void forwardActiveRules().then(setActive);
  }, []);

  useEffect(load, []);
  useEffect(() => {
    const timer = window.setInterval(() => void forwardActiveRules().then(setActive), 3000);
    return () => window.clearInterval(timer);
  }, []);

  const toggle = (rule: ForwardRow) => {
    void forwardToggle(rule.id, !rule.enabled)
      .then(load)
      .catch((err) => setError(String(err)));
  };

  const add = () => {
    const hostId = Number(form.hostId);
    const listenPort = Number(form.listenPort);
    if (!hostId || !listenPort) {
      setError("请选择主机并填写监听端口");
      return;
    }
    if (portFree === false) {
      setError(`本机端口 ${listenPort} 已被占用`);
      return;
    }
    const targetHost = form.kind === "local" ? form.targetHost || "127.0.0.1" : "";
    const targetPort = form.kind === "local" ? Number(form.targetPort) || 0 : 0;
    if (form.kind === "local" && !targetPort) {
      setError("本地转发需要填写目标端口");
      return;
    }
    void forwardSave(
      { hostId, kind: form.kind, listenPort, targetHost, targetPort, enabled: true },
      null,
    )
      .then(() => void forwardApply(hostId))
      .then(() => {
        setForm({ ...form, listenPort: "", targetPort: "" });
        setPortFree(null);
        load();
      })
      .catch((err) => setError(String(err)));
  };

  const onPortChange = (value: string) => {
    setForm({ ...form, listenPort: value });
    const port = Number(value);
    if (Number.isInteger(port) && port > 0 && port < 65536) {
      void checkPortFree(port).then(setPortFree);
    } else {
      setPortFree(null);
    }
  };

  const selectedHost: Host | undefined = hosts.find((h) => String(h.id) === form.hostId);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-6">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-xl font-semibold">隧道中心</h1>
        <button className="sbtn" onClick={load}>
          刷新
        </button>
      </div>
      <p className="mb-4 text-xs leading-4 text-[var(--dim)]">
        端口映射规则集中管理：会话在线时启停立即生效，离线时在下次连接自动应用。绿点 = 正在监听。
      </p>

      {error && <p className="mb-3 text-xs text-red-500">{error}</p>}

      <div className="ssection">
        <div className="stitle">新建规则</div>
        <div className="flex flex-wrap items-end gap-2">
          <select
            className="sinput w-40"
            value={form.hostId}
            onChange={(e) => setForm({ ...form, hostId: e.target.value })}
          >
            <option value="">选择主机…</option>
            {hosts.map((h) => (
              <option key={h.id} value={h.id}>
                {h.name}
              </option>
            ))}
          </select>
          <select
            className="sinput w-36"
            value={form.kind}
            onChange={(e) => setForm({ ...form, kind: e.target.value })}
          >
            <option value="local">本地 → 远端</option>
            <option value="remote">远端 → 本地</option>
            <option value="dynamic">动态 SOCKS5</option>
          </select>
          <div>
            <input
              className="sinput w-28"
              placeholder="监听端口"
              value={form.listenPort}
              onChange={(e) => onPortChange(e.target.value)}
            />
            {portFree === false && (
              <p className="mt-0.5 text-xs text-red-500">端口已被占用</p>
            )}
            {portFree === true && <p className="mt-0.5 text-xs text-[var(--ok)]">端口可用</p>}
          </div>
          {form.kind === "local" && (
            <>
              <input
                className="sinput w-36"
                placeholder="目标主机"
                value={form.targetHost}
                onChange={(e) => setForm({ ...form, targetHost: e.target.value })}
              />
              <input
                className="sinput w-28"
                placeholder="目标端口"
                value={form.targetPort}
                onChange={(e) => setForm({ ...form, targetPort: e.target.value })}
              />
            </>
          )}
          <button
            className="sbtn sbtn-primary"
            disabled={!form.hostId || !form.listenPort}
            onClick={add}
          >
            添加规则
          </button>
        </div>
        {selectedHost && form.kind !== "dynamic" && (
          <p className="mt-2 text-xs leading-4 text-[var(--dim)]">
            {form.kind === "local"
              ? `连接 ${selectedHost.name} 后，本机 :${form.listenPort || "…"} 的流量经服务器转发到 ${form.targetHost}:${form.targetPort || "…"}`
              : `服务器上的 :${form.listenPort || "…"} 会反向映射回本机`}
          </p>
        )}
      </div>

      {rules.length === 0 ? (
        <p className="text-sm text-[var(--faint)]">还没有规则。用上面的表单添加第一条。</p>
      ) : (
        <div className="flex flex-col gap-2">
          {rules.map((r) => {
            const isActive = r.enabled && active.includes(r.id);
            return (
              <div
                key={r.id}
                className="flex items-center gap-3 rounded-xl border border-[var(--border)] bg-[var(--panel)] px-4 py-3"
              >
                <span
                  className={`h-2.5 w-2.5 shrink-0 rounded-full ${
                    isActive ? "bg-[var(--ok)]" : r.enabled ? "bg-amber-500" : "bg-[var(--faint)]"
                  }`}
                  title={isActive ? "正在监听" : r.enabled ? "主机离线" : "已停用"}
                />
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium">
                    {r.hostName} · {KIND_LABEL[r.kind] ?? r.kind} :{r.listenPort}
                    {r.kind !== "dynamic" && ` → ${r.targetHost || "本机"}:${r.targetPort}`}
                  </div>
                  <div className="text-xs text-[var(--dim)]">
                    {isActive ? "转发中" : r.enabled ? "等待主机上线" : "已停用"}
                  </div>
                </div>
                <label className="flex shrink-0 items-center gap-1.5 text-xs text-[var(--dim)]">
                  <input
                    type="checkbox"
                    checked={r.enabled}
                    onChange={() => toggle(r)}
                  />
                  启用
                </label>
                <button
                  className="shrink-0 rounded px-2 py-1 text-xs text-[var(--dim)] hover:bg-[var(--hover)] hover:text-red-500"
                  onClick={() => {
                    if (window.confirm(`删除规则（${r.hostName} :${r.listenPort}）？`)) {
                      void forwardDelete(r.id).then(load);
                    }
                  }}
                >
                  删除
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
