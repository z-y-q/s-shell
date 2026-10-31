# s-shell

桌面 SSH 客户端：Tauri 2 + React 19 + russh 0.65。覆盖 macOS / Windows / Linux。

定位：运维与开发自用工具。核心卖点——连接管理（跳板链、代理、四种认证）、GTK 级低延迟终端（GBK/Big5 编码、兼容老设备）、SFTP 双栏、端口转发、asciinema 录制回放、内置 AI 助手与 MCP 服务端、WebDAV 同步。

## 功能总览

**连接与会话**
- 主机卡片网格 + 搜索 + 一键连接；导入 OpenSSH config / Xshell(.xsh) / Termius JSON
- 多标签、水平/垂直分屏、广播输入（一份键盘写多会话）
- 断线自动重连（指数退避 5 次），tmux 策略（附加/包裹）恢复现场
- 会话录制（asciinema v2）与回放；隐身模式（模糊+禁复制）
- keepalive、TCP_NODELAY、8ms→4ms 自适应输出聚合

**认证**：密码（系统钥匙串/主密码库）、私钥（含口令）、OpenSSH 证书、ssh-agent（1Password/FIDO）

**网络**
- 多级跳板链（防环，逐级认证与指纹固定）
- 端口转发：本地 / 远程 / 动态 SOCKS5，随会话自启自停，可在线重启
- 出站代理：SOCKS5 / HTTP CONNECT / ProxyCommand 外部命令
- 兼容模式：老 KEX + ssh-rsa SHA-1 + 3DES + HMAC-SHA1（华为/H3C 老设备）
- SSH 压缩开关（高延迟线路）

**终端**
- 编码：UTF-8 / GBK / Big5 / Windows-1252（状态机解码，跨块多字节无损）
- 主题 5 种、连字开关、滚动缓冲 5 千/5 万/10 万行
- WebGL 渲染（Canvas 自动降级）、搜索、链接点击、sixel/内联图片
- zmodem（rz/sz）：内核检测分流 + zmodem.js 端点，接收落盘

**SFTP**：双栏本地/远端浏览、拖拽传输、递归上传下载（4 路并发）、断点续传、限速（KB/s）、进度条

**AI 助手**：OpenAI 兼容接口（DeepSeek 默认/自定义/本地 Ollama）
- 命令生成、错误诊断、命令解释（bash 块一键写入终端）
- 发送前逐行预览，敏感行可排除；上下文自动剥离转义序列并脱敏
- 危险命令本地规则拦截（rm -rf、mkfs、dd、删库等 9 类）
- 长输出自动摘要（200KB+2s 停顿触发，设置开关）
- 多台主机并行执行 + AI 汇总差异

**MCP 服务端**：`s-shell mcp` 提供 list_hosts / run_command / upload_file 三个工具，默认需界面逐条确认（120 秒超时自动拒绝），可关闭审批

**其他**：已知主机指纹管理页、连接事件日志（脱敏）、WebDAV 同步（快照合并）、命令面板 Ctrl/Cmd+K、片段管理、托盘常驻、全局快捷键 Ctrl/Cmd+Shift+S、深浅色分区界面（浅色管理 + 深色终端）

## 快速开始

```bash
pnpm install
pnpm tauri dev      # 开发模式
pnpm tauri build    # 正式版：src-tauri/target/release/bundle/{macos,dmg}
```

依赖：Node 20+、pnpm、Rust 1.90+（macOS 需 Xcode CLT；Windows 需 WebView2；Linux 需 webkit2gtk）。

### 首次使用

1. 「新建主机」：填地址/账号/密码（存系统钥匙串）或私钥路径
2. 卡片点击连接；老设备勾「兼容模式」，中文老机选 GBK 编码
3. AI 助手需在「设置」填 API Key（默认 DeepSeek，可改任意 OpenAI 兼容端点或本地 Ollama）

### MCP 接入

```bash
/path/to/s-shell mcp    # stdio JSON-RPC，供 Claude Code 等外部工具配置
```

工具：`list_hosts`、`run_command`、`upload_file`。run_command 默认需在主界面弹窗批准（设置可关）。

## 数据位置

| 内容 | 路径 |
|---|---|
| 主机/设置/指纹/片段/MCP 审批 | `~/Library/Application Support/s-shell/s-shell.db`（Windows/Linux 为对应数据目录）|
| 凭据 | macOS 钥匙串 / Windows 凭据管理器 / Linux Secret Service（缺失时主密码加密库） |
| 录制 | 同目录 `recordings/*.cast` |
| zmodem 接收 | 同目录 `zmodem/` |
| 连接日志（脱敏） | 同目录 `logs/s-shell.log` |

## 性能（实测，M 系列本机）

内核解码+聚合 982 MiB/s ｜ IPC 通道 242 MiB/s ｜ xterm 解析 ≤4ms/块 ｜ 空载内存 ~134MB ｜ 击键→内核 ≤6ms ｜ 连接握手 1.5～1.8s（移动网络到美国）

## 测试

```bash
cd src-tauri
cargo test                    # 33 项单元测试
cargo test -- --ignored      # 外网 SSH 协议栈验证（需网络）
cargo test 性能 -- --ignored --nocapture   # 100MB 吞吐基准
```

## 安全模型

- known_hosts：首连自动记录 SHA256 指纹并固定，变更即阻断（「已知主机」页可删除重录）
- 凭据不出进程：AI 上下文经脱敏规则库（IP/密码/令牌/密钥块/邮箱），MCP 拿不到凭据
- 连接日志与 AI 共用同一套脱敏规则
- 无遥测

## 明确不做

串口/telnet、X11 转发、mosh、GSSAPI/Kerberos、移动端。

## 已知边界

- WebDAV 为快照合并（按主机名同名匹配、新时间戳胜出），非增量 oplog
- 隐身模式防复制/模糊显示，防不了系统级截屏
- mosh 未实现
- 首次连接指纹为自动记录（无人工确认弹窗），可在「已知主机」页复核删除
- IME 多平台与老设备真机矩阵未覆盖（无对应硬件）

## 目录结构

```
src/                  # React 前端（组件/视图/IPC 层）
src-tauri/src/
  main.rs             # 命令注册与入口（mcp 子命令分流）
  ssh/session.rs      # 连接/跳板链/认证/聚合泵/重连/回显采样
  ssh/compat.rs       # 兼容模式算法集
  sftp.rs             # SFTP（递归/续传/限速/进度）
  forward.rs          # 端口转发管理器
  record.rs           # 录制与回放
  mcp.rs              # MCP stdio 服务端
  sync.rs             # WebDAV 快照同步
  codec.rs detect.rs masker.rs ai.rs log.rs tmux.rs vault.rs store.rs
docs/ARCHITECTURE.md  # 架构与数据流详解
```
