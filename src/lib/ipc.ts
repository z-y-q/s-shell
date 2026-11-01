import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type SessionEncodingName = "utf8" | "gbk" | "big5" | "windows1252";
export type TmuxPolicyName = "none" | "attach" | "wrap";
export type AuthMethod = "password" | "key" | "agent" | "cert";
export type ForwardKind = "local" | "remote" | "dynamic";
export type ProxyKind = "none" | "socks5" | "http" | "command";

export interface Host {
  id: number;
  name: string;
  host: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  keyPath: string | null;
  encoding: SessionEncodingName;
  compatMode: boolean;
  tmuxPolicy: TmuxPolicyName;
  jumpHostId: number | null;
  certPath: string | null;
  shellIntegration: boolean;
  proxyKind: ProxyKind;
  proxyHost: string | null;
  proxyPort: number | null;
  proxyCommand: string | null;
  compress: boolean;
  updatedAt: string;
}

export type HostInput = Omit<Host, "id" | "updatedAt">;

export interface Forward {
  id: number;
  hostId: number;
  kind: ForwardKind;
  listenPort: number;
  targetHost: string;
  targetPort: number;
  enabled: boolean;
}

export type ForwardInput = Omit<Forward, "id">;

export interface FsEntry {
  name: string;
  isDir: boolean;
  size: number;
  mtime: number;
}

export interface McpRequest {
  id: number;
  tool: string;
  argsJson: string;
}

export interface AiConfig {
  baseUrl: string;
  model: string;
  hasKey: boolean;
}

export function listHosts(): Promise<Host[]> {
  return invoke("host_list");
}

export function saveHost(input: HostInput, id: number | null): Promise<number> {
  return invoke("host_save", { id, host: input });
}

export function deleteHost(id: number): Promise<void> {
  return invoke("host_delete", { id });
}

export function vaultSet(hostId: number, field: string, value: string): Promise<void> {
  return invoke("vault_set", { hostId, field, value });
}

export interface VaultStatus {
  mode: "keyring" | "master";
  unlocked: boolean;
}

export function vaultStatus(): Promise<VaultStatus> {
  return invoke("vault_status");
}

export function vaultMasterUnlock(password: string): Promise<void> {
  return invoke("vault_master_unlock", { password });
}

export function localList(path: string): Promise<FsEntry[]> {
  return invoke("local_list", { path });
}

export function localMkdir(path: string): Promise<void> {
  return invoke("local_mkdir", { path });
}

export function localRemove(path: string, isDir: boolean): Promise<void> {
  return invoke("local_remove", { path, isDir });
}

export function settingsSet(key: string, value: string): Promise<void> {
  return invoke("settings_set", { key, value });
}

export function settingsGet(key: string): Promise<string | null> {
  return invoke("settings_get", { key });
}

export function aiConfigGet(): Promise<AiConfig> {
  return invoke("ai_config_get");
}

export function aiAsk(question: string, context: string): Promise<string> {
  return invoke("ai_ask", { question, context });
}

export function aiPreview(context: string): Promise<string> {
  return invoke("ai_preview", { context });
}

export function aiCheckCommand(command: string): Promise<string[]> {
  return invoke("ai_check_command", { command });
}

export interface BatchResult {
  hostId: number;
  name?: string;
  output?: string;
  error?: string;
}

export function batchExec(hostIds: number[], command: string): Promise<BatchResult[]> {
  return invoke("batch_exec", { hostIds, command });
}

export function sessionContext(sessionId: number): Promise<{ cwd: string | null }> {
  return invoke("session_context", { sessionId });
}

export interface AiSummary {
  sessionId: number;
  text: string;
}

export function onAiSummary(cb: (s: AiSummary) => void): Promise<() => void> {
  return listen<AiSummary>("ai-summary", (event) => cb(event.payload));
}

export function forwardList(hostId: number): Promise<Forward[]> {
  return invoke("forward_list", { hostId });
}

export function forwardSave(input: ForwardInput, id: number | null): Promise<number> {
  return invoke("forward_save", { id, forward: input });
}

export function forwardDelete(id: number): Promise<void> {
  return invoke("forward_delete", { id });
}

export function forwardApply(hostId: number): Promise<number> {
  return invoke("forward_apply", { hostId });
}

export function openSession(
  hostId: number,
  cols: number,
  rows: number,
  onData: (chunk: Uint8Array) => void,
): Promise<number> {
  const channel = new Channel<unknown>();
  channel.onmessage = (raw) => {
    if (typeof raw === "string") {
      onData(new TextEncoder().encode(raw));
      return;
    }
    onData(new Uint8Array(raw as ArrayBuffer));
  };
  return invoke<number>("session_open", { hostId, cols, rows, onData: channel });
}

export function sessionWrite(id: number, data: Uint8Array): Promise<void> {
  return invoke("session_write", { id, data: Array.from(data) });
}

export function sessionResize(id: number, cols: number, rows: number): Promise<void> {
  return invoke("session_resize", { id, cols, rows });
}

export function sessionClose(id: number): Promise<void> {
  return invoke("session_close", { id });
}

export function localOpen(
  cols: number,
  rows: number,
  onData: (chunk: Uint8Array) => void,
): Promise<number> {
  const channel = new Channel<unknown>();
  channel.onmessage = (raw) => {
    if (typeof raw === "string") {
      onData(new TextEncoder().encode(raw));
      return;
    }
    onData(new Uint8Array(raw as ArrayBuffer));
  };
  return invoke<number>("local_open", { cols, rows, onData: channel });
}

export function localWrite(id: number, data: Uint8Array): Promise<void> {
  return invoke("local_write", { id, data: Array.from(data) });
}

export function localResize(id: number, cols: number, rows: number): Promise<void> {
  return invoke("local_resize", { id, cols, rows });
}

export function localClose(id: number): Promise<void> {
  return invoke("local_close", { id });
}

export function setWindowTheme(dark: boolean): Promise<void> {
  return invoke("set_window_theme", { dark });
}

export function sessionLogStart(sessionId: number): Promise<string> {
  return invoke("session_log_start", { sessionId });
}

export function sessionLogStop(sessionId: number): Promise<void> {
  return invoke("session_log_stop", { sessionId });
}

export interface ForwardRow {
  id: number;
  hostId: number;
  hostName: string;
  kind: string;
  listenPort: number;
  targetHost: string;
  targetPort: number;
  enabled: boolean;
}

export function forwardListAll(): Promise<ForwardRow[]> {
  return invoke("forward_list_all");
}

export function forwardToggle(id: number, enabled: boolean): Promise<void> {
  return invoke("forward_toggle", { id, enabled });
}

export function forwardActiveRules(): Promise<number[]> {
  return invoke("forward_active_rules");
}

export function checkPortFree(port: number): Promise<boolean> {
  return invoke("check_port_free", { port });
}

export function onSessionClosed(cb: (sessionId: number) => void): Promise<() => void> {
  return listen<number>("session-closed", (event) => cb(event.payload));
}

export function recordStart(sessionId: number): Promise<string> {
  return invoke("record_start", { sessionId });
}

export function recordStop(sessionId: number): Promise<void> {
  return invoke("record_stop", { sessionId });
}

export function recordList(): Promise<string[]> {
  return invoke("record_list");
}

export function replayOpen(
  file: string,
  onData: (chunk: Uint8Array) => void,
): Promise<void> {
  const channel = new Channel<unknown>();
  channel.onmessage = (raw) => {
    if (typeof raw === "string") {
      onData(new TextEncoder().encode(raw));
      return;
    }
    onData(new Uint8Array(raw as ArrayBuffer));
  };
  return invoke("replay_open", { file, onData: channel });
}

export function sftpList(sessionId: number, path: string): Promise<FsEntry[]> {
  return invoke("sftp_list", { sessionId, path });
}

export function sftpMkdir(sessionId: number, path: string): Promise<void> {
  return invoke("sftp_mkdir", { sessionId, path });
}

export function sftpRemove(sessionId: number, path: string, isDir: boolean): Promise<void> {
  return invoke("sftp_remove", { sessionId, path, isDir });
}

export function sftpRename(sessionId: number, from: string, to: string): Promise<void> {
  return invoke("sftp_rename", { sessionId, from, to });
}

export function sftpUpload(
  sessionId: number,
  localPath: string,
  remotePath: string,
  resume = false,
  limitKbps = 0,
): Promise<number> {
  return invoke("sftp_upload", { sessionId, localPath, remotePath, resume, limitKbps });
}

export function sftpDownload(
  sessionId: number,
  remotePath: string,
  localPath: string,
  resume = false,
  limitKbps = 0,
): Promise<number> {
  return invoke("sftp_download", { sessionId, remotePath, localPath, resume, limitKbps });
}

export interface SftpProgress {
  taskId: number;
  done: number;
  total: number;
}

export interface SftpDone {
  taskId: number;
  ok: boolean;
  error?: string;
}

export function onSftpProgress(cb: (p: SftpProgress) => void): Promise<() => void> {
  return listen<SftpProgress>("sftp-progress", (event) => cb(event.payload));
}

export function onSftpDone(cb: (r: SftpDone) => void): Promise<() => void> {
  return listen<SftpDone>("sftp-done", (event) => cb(event.payload));
}

export function syncPush(): Promise<string> {
  return invoke("sync_push");
}

export function syncPull(): Promise<string> {
  return invoke("sync_pull");
}

export function mcpPendingList(): Promise<McpRequest[]> {
  return invoke("mcp_pending_list");
}

export function mcpDecide(id: number, approved: boolean): Promise<void> {
  return invoke("mcp_decide", { id, approved });
}

export function zmodemSave(name: string, dataBase64: string): Promise<string> {
  return invoke("zmodem_save", { name, dataBase64 });
}

export function logTail(lines: number): Promise<string[]> {
  return invoke("log_tail", { lines });
}

export type ImportKind = "ssh_config" | "xshell" | "termius";

export function perfStream(totalMb: number, onData: (chunk: Uint8Array) => void): Promise<string> {
  const channel = new Channel<unknown>();
  channel.onmessage = (raw) => {
    if (typeof raw === "string") {
      onData(new TextEncoder().encode(raw));
      return;
    }
    onData(new Uint8Array(raw as ArrayBuffer));
  };
  return invoke("perf_stream", { totalMb, onData: channel });
}

export function importHosts(kind: ImportKind, path: string): Promise<number> {
  return invoke("import_hosts", { kind, path });
}

export interface KnownHost {
  host: string;
  port: number;
  keyAlgo: string;
  fingerprint: string;
  firstSeen: string;
}

export function knownHostsList(): Promise<KnownHost[]> {
  return invoke("known_hosts_list");
}

export function knownHostsDelete(host: string, port: number): Promise<void> {
  return invoke("known_hosts_delete", { host, port });
}

export interface Snippet {
  id: number;
  name: string;
  content: string;
}

export function snippetList(): Promise<Snippet[]> {
  return invoke("snippet_list");
}

export function snippetSave(
  input: { name: string; content: string },
  id: number | null,
): Promise<number> {
  return invoke("snippet_save", { id, snippet: input });
}

export function snippetDelete(id: number): Promise<void> {
  return invoke("snippet_delete", { id });
}
