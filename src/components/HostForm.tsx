import { useState } from "react";
import SidePanel from "./SidePanel";
import {
  saveHost,
  vaultSet,
  type AuthMethod,
  type Host,
  type HostInput,
  type ProxyKind,
  type SessionEncodingName,
  type TmuxPolicyName,
} from "../lib/ipc";
import { useHosts } from "../stores/hosts";

interface Props {
  editing: Host | null;
  onClose: () => void;
  onSaved: () => void;
}

export default function HostForm({ editing, onClose, onSaved }: Props) {
  const { hosts } = useHosts();
  const [name, setName] = useState(editing?.name ?? "");
  const [groupName, setGroupName] = useState(editing?.groupName ?? "");
  const [host, setHost] = useState(editing?.host ?? "");
  const [port, setPort] = useState(String(editing?.port ?? 22));
  const [username, setUsername] = useState(editing?.username ?? "root");
  const [authMethod, setAuthMethod] = useState<AuthMethod>(editing?.authMethod ?? "password");
  const [password, setPassword] = useState("");
  const [keyPath, setKeyPath] = useState(editing?.keyPath ?? "");
  const [certPath, setCertPath] = useState(editing?.certPath ?? "");
  const [keyPassphrase, setKeyPassphrase] = useState("");
  const [encoding, setEncoding] = useState<SessionEncodingName>(editing?.encoding ?? "utf8");
  const [compatMode, setCompatMode] = useState(editing?.compatMode ?? false);
  const [tmuxPolicy, setTmuxPolicy] = useState<TmuxPolicyName>(editing?.tmuxPolicy ?? "none");
  const [jumpHostId, setJumpHostId] = useState<number | null>(editing?.jumpHostId ?? null);
  const [shellIntegration, setShellIntegration] = useState(editing?.shellIntegration ?? false);
  const [compress, setCompress] = useState(editing?.compress ?? false);
  const [proxyKind, setProxyKind] = useState<ProxyKind>(editing?.proxyKind ?? "none");
  const [proxyHost, setProxyHost] = useState(editing?.proxyHost ?? "");
  const [proxyPort, setProxyPort] = useState(editing?.proxyPort != null ? String(editing.proxyPort) : "");
  const [proxyCommand, setProxyCommand] = useState(editing?.proxyCommand ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = () => {
    if (!name.trim() || !host.trim() || !username.trim()) {
      setError("名称、主机地址、用户名都不能为空");
      return;
    }
    const portNumber = Number(port);
    if (!Number.isInteger(portNumber) || portNumber <= 0 || portNumber > 65535) {
      setError("端口必须是 1 到 65535 的整数");
      return;
    }
    if (authMethod === "key" && !keyPath.trim()) {
      setError("密钥认证必须填写私钥路径");
      return;
    }
    if (authMethod === "cert" && (!keyPath.trim() || !certPath.trim())) {
      setError("证书认证必须填写私钥路径与证书路径");
      return;
    }
    setBusy(true);
    const input: HostInput = {
      name: name.trim(),
      host: host.trim(),
      port: portNumber,
      groupName: groupName.trim(),
      username: username.trim(),
      authMethod,
      keyPath: authMethod === "password" || authMethod === "agent" ? null : keyPath.trim(),
      encoding,
      compatMode,
      tmuxPolicy,
      jumpHostId,
      certPath: authMethod === "cert" ? certPath.trim() : null,
      shellIntegration,
      compress,
      proxyKind,
      proxyHost: proxyKind === "socks5" || proxyKind === "http" ? proxyHost.trim() : null,
      proxyPort: proxyKind === "socks5" || proxyKind === "http" ? Number(proxyPort) || null : null,
      proxyCommand: proxyKind === "command" ? proxyCommand.trim() : null,
    };
    saveHost(input, editing?.id ?? null)
      .then((id) => {
        const tasks: Array<Promise<void>> = [];
        if (authMethod === "password" && password) {
          tasks.push(vaultSet(id, "password", password));
        }
        if (authMethod === "key" && keyPassphrase) {
          tasks.push(vaultSet(id, "keyPassphrase", keyPassphrase));
        }
        return Promise.all(tasks)
          .then(() => undefined)
          .catch((err) => {
            onSaved();
            setError(`主机已保存，但凭据写入失败：${String(err)}`);
            setBusy(false);
            throw new Error("__handled__");
          });
      })
      .then(onSaved)
      .catch((err) => {
        if (String(err?.message ?? err) === "__handled__") {
          return;
        }
        setError(String(err));
        setBusy(false);
      });
  };

  const label = "slabel";
  const input = "sinput";
  const section = "ssection mb-3";

  return (
    <SidePanel
      title={editing ? "编辑主机" : "新建主机"}
      subtitle="保存后即可从主机列表一键连接"
      onClose={onClose}
      footer={
        <div className="flex gap-2">
          <button className="sbtn flex-1" onClick={onClose}>
            取消
          </button>
          <button
            className="sbtn sbtn-primary flex-1"
            disabled={busy}
            onClick={submit}
          >
            {busy ? "保存中" : "保存"}
          </button>
        </div>
      }
    >
      {error && <p className="mb-3 text-xs text-red-500">{error}</p>}

      <div className={section}>
        <div className="stitle">地址与账号</div>
        <div className="mb-3">
          <label className={label}>名称</label>
          <input className={input} value={name} onChange={(e) => setName(e.target.value)} placeholder="生产-Web-01" />
        </div>
        <div className="mb-3">
          <label className={label}>分组（可选，用于主机页归类）</label>
          <input
            className={input}
            list="sshell-groups"
            value={groupName}
            onChange={(e) => setGroupName(e.target.value)}
            placeholder="例如：生产环境"
          />
          <datalist id="sshell-groups">
            {hosts
              .map((h) => h.groupName)
              .filter((g, i, arr) => g && arr.indexOf(g) === i)
              .map((g) => (
                <option key={g} value={g} />
              ))}
          </datalist>
        </div>
        <div className="mb-3 flex gap-3">
          <div className="flex-1">
            <label className={label}>主机地址</label>
            <input className={input} value={host} onChange={(e) => setHost(e.target.value)} placeholder="192.168.1.10" />
          </div>
          <div className="w-24">
            <label className={label}>端口</label>
            <input className={input} value={port} onChange={(e) => setPort(e.target.value)} />
          </div>
        </div>
        <div>
          <label className={label}>用户名</label>
          <input className={input} value={username} onChange={(e) => setUsername(e.target.value)} />
        </div>
      </div>

      <div className={section}>
        <div className="stitle">认证</div>
        <div className="mb-3">
          <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-[var(--text)]">
            <label className="flex items-center gap-1">
              <input type="radio" checked={authMethod === "password"} onChange={() => setAuthMethod("password")} />
              密码
            </label>
            <label className="flex items-center gap-1">
              <input type="radio" checked={authMethod === "key"} onChange={() => setAuthMethod("key")} />
              私钥
            </label>
            <label className="flex items-center gap-1">
              <input type="radio" checked={authMethod === "agent"} onChange={() => setAuthMethod("agent")} />
              ssh-agent
            </label>
            <label className="flex items-center gap-1">
              <input type="radio" checked={authMethod === "cert"} onChange={() => setAuthMethod("cert")} />
              证书
            </label>
          </div>
        </div>

        {authMethod === "password" ? (
          <div>
            <label className={label}>密码（存入系统钥匙串）</label>
            <input
              className={input}
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={editing ? "留空表示不修改" : ""}
            />
          </div>
        ) : authMethod === "agent" ? (
          <p className="text-xs leading-4 text-[var(--dim)]">
            连接时遍历本机 agent 里的全部密钥逐一尝试；1Password 与 FIDO 硬件密钥经 agent 完整支持。
          </p>
        ) : (
          <>
            <div className="mb-3">
              <label className={label}>私钥路径{authMethod === "cert" ? "（证书对应的私钥）" : ""}</label>
              <input className={input} value={keyPath} onChange={(e) => setKeyPath(e.target.value)} placeholder="~/.ssh/id_ed25519" />
            </div>
            {authMethod === "cert" && (
              <div className="mb-3">
                <label className={label}>证书路径（*-cert.pub）</label>
                <input
                  className={input}
                  value={certPath}
                  onChange={(e) => setCertPath(e.target.value)}
                  placeholder="~/.ssh/id_ed25519-cert.pub"
                />
              </div>
            )}
            <div>
              <label className={label}>私钥口令（可选，留空表示不修改）</label>
              <input
                className={input}
                type="password"
                value={keyPassphrase}
                onChange={(e) => setKeyPassphrase(e.target.value)}
              />
            </div>
          </>
        )}
      </div>

      <div className={section}>
        <div className="stitle">会话</div>
        <div className="mb-3 flex gap-3">
          <div className="flex-1">
            <label className={label}>编码</label>
            <select className={input} value={encoding} onChange={(e) => setEncoding(e.target.value as SessionEncodingName)}>
              <option value="utf8">UTF-8</option>
              <option value="gbk">GBK</option>
              <option value="big5">Big5</option>
              <option value="windows1252">Windows-1252</option>
            </select>
          </div>
          <div className="flex-1">
            <label className={label}>tmux 策略</label>
            <select className={input} value={tmuxPolicy} onChange={(e) => setTmuxPolicy(e.target.value as TmuxPolicyName)}>
              <option value="none">不使用</option>
              <option value="attach">自动附加</option>
              <option value="wrap">自动包裹</option>
            </select>
          </div>
        </div>
        <div className="mb-3">
          <label className={label}>跳板机（经它中转连接，可多级串联）</label>
          <select
            className={input}
            value={jumpHostId ?? ""}
            onChange={(e) => setJumpHostId(e.target.value === "" ? null : Number(e.target.value))}
          >
            <option value="">不使用</option>
            {hosts
              .filter((h) => h.id !== editing?.id)
              .map((h) => (
                <option key={h.id} value={h.id}>
                  {h.name}（{h.host}）
                </option>
              ))}
          </select>
        </div>
        <label className="flex items-center gap-2 text-sm text-[var(--text)]">
          <input type="checkbox" checked={compatMode} onChange={(e) => setCompatMode(e.target.checked)} />
          兼容模式（老设备连不上时开启）
        </label>
      </div>

      <div className={section}>
        <div className="stitle">传输与集成</div>
        <label className="mb-3 flex items-start gap-2 text-sm text-[var(--text)]">
          <input
            type="checkbox"
            className="mt-1"
            checked={compress}
            onChange={(e) => setCompress(e.target.checked)}
          />
          SSH 压缩（跨国、移动网络等高延迟线路可减轻传输压力；本地网络无需开启）
        </label>
        <label className="mb-3 flex items-start gap-2 text-sm text-[var(--text)]">
          <input
            type="checkbox"
            className="mt-1"
            checked={shellIntegration}
            onChange={(e) => setShellIntegration(e.target.checked)}
          />
          外壳集成（注入 OSC 133 钩子，为 AI 提供当前目录上下文；bash 与 zsh 生效）
        </label>
        <div>
          <label className={label}>出站代理（经它建立 SSH 连接，只作用于第一跳）</label>
          <div className="flex gap-2">
            <select
              className={input}
              value={proxyKind}
              onChange={(e) => setProxyKind(e.target.value as ProxyKind)}
            >
              <option value="none">不使用</option>
              <option value="socks5">SOCKS5</option>
              <option value="http">HTTP CONNECT</option>
              <option value="command">ProxyCommand 外部命令</option>
            </select>
            {(proxyKind === "socks5" || proxyKind === "http") && (
              <>
                <input
                  className={input}
                  placeholder="代理地址"
                  value={proxyHost}
                  onChange={(e) => setProxyHost(e.target.value)}
                />
                <input
                  className={`${input} w-24`}
                  placeholder="端口"
                  value={proxyPort}
                  onChange={(e) => setProxyPort(e.target.value)}
                />
              </>
            )}
          </div>
          {proxyKind === "command" && (
            <input
              className={`${input} mt-2`}
              placeholder="例如：nc -X connect -x 127.0.0.1:7890 %h %p"
              value={proxyCommand}
              onChange={(e) => setProxyCommand(e.target.value)}
            />
          )}
        </div>
      </div>
    </SidePanel>
  );
}
