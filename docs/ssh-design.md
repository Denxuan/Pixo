# SSH 终端工具设计

> 状态：草案　｜　适用版本：Pixo 0.1.x 之后的第一版 SSH 功能
>
> 实现进度：阶段 1 已接入（主机列表、指纹校验、密码和私钥登录、主窗口终端标签、开关）。可选记住密码和口令已写入系统钥匙串。上传文件和文件夹已接入：可从工具栏选择，也可拖到当前终端，直接传到该会话的当前目录。下载、跳板机、`~/.ssh/config` 导入、agent 和 keyboard-interactive 仍按第 10 节后续阶段实现。

## 1. 目标与范围

在 Pixo 工具箱中新增“SSH”工具，用于管理远程主机、打开交互式终端，并在本机与远程主机之间传输文件。

第一版包含：

- 主机列表：新建、编辑、删除、分组、搜索；从 `~/.ssh/config` 导入。
- 认证：密码、私钥文件（可带口令）、ssh-agent、keyboard-interactive。
- 主机指纹校验：首次连接由用户确认，指纹变化时阻止连接。
- 跳板机：主机可指定一台或多台跳板机（等同 `ProxyJump`）。
- 交互式终端：主窗口内多标签页，窗口尺寸同步，复制粘贴，搜索，断线重连。
- 文件传输：上传文件和文件夹、下载文件和文件夹，带进度、可取消，同名冲突由用户选择。
- 可选“记住密码 / 口令”，存入系统钥匙串。

不包含：

- 远程目录浏览器（第二版；第一版通过远程路径输入选择目标，见 4.5）。
- 端口转发、X11 转发、SSH 服务器端功能。
- 主机配置或凭据的云同步、跨设备共享。
- 独立终端窗口、标签页拖出成窗口。
- 会话录制、命令审计。

## 2. 设计原则

- **系统能力在 Rust，界面在前端。** 网络连接、认证、SFTP、本地文件读写、钥匙串都在 Rust 实现；终端渲染、主机表单、传输列表在前端。
- **不静默接受主机指纹。** 未知指纹必须由用户确认；指纹变化直接阻止连接，界面不提供“一键忽略”。
- **配置里不存秘密。** SQLite 只存主机地址、用户名、私钥路径等；密码和口令要么每次询问，要么在用户明确勾选后放入系统钥匙串。
- **本地路径只来自用户操作。** 上传源和下载目标只能来自 Rust 侧弹出的系统对话框或窗口拖放事件，前端不能传入任意本地路径。
- **避免数据丢失。** 传输先写临时文件再重命名；默认不覆盖已有文件；中断或取消不留下损坏的目标文件。
- **不静默失败。** 钥匙串不可用、agent 不可达、远程不支持 SFTP 等情况，都返回稳定错误码，由界面明确提示。

## 3. 平台支持

SSH 协议和 SFTP 由纯 Rust 的 `russh`、`russh-sftp` 实现，三个平台行为一致。差异集中在 agent、钥匙串和快捷键。

| 能力 | macOS | Windows | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| SSH / SFTP | 支持 | 支持 | 支持 | 支持 |
| ssh-agent | `SSH_AUTH_SOCK` | OpenSSH 命名管道、Pageant | `SSH_AUTH_SOCK` | `SSH_AUTH_SOCK` |
| 记住密码 | 钥匙串 | 凭据管理器 | Secret Service | Secret Service |
| 拖放上传 | 支持 | 支持 | 支持 | 需实测 |
| 终端复制粘贴 | `Cmd+C` / `Cmd+V` | `Ctrl+Shift+C` / `Ctrl+Shift+V` | 同 Windows | 同 Windows |

### 3.1 macOS

- 从 Finder 或 Dock 启动时，进程环境里可能没有 `SSH_AUTH_SOCK`。启动时先读环境变量，读不到时尝试 `launchctl getenv SSH_AUTH_SOCK`；仍不可用时 agent 认证返回 `agent_unavailable`。
- 钥匙串使用 `keyring` 的 `apple-native` 后端，条目的服务名为 `pixo.ssh`。

### 3.2 Windows

- agent 依次尝试 Windows OpenSSH 的 `\\.\pipe\openssh-ssh-agent` 和 Pageant。`russh` 对两者的支持需要在实现前实测确认。
- 默认读取 `%USERPROFILE%\.ssh\config` 和 `%USERPROFILE%\.ssh\known_hosts`。
- 钥匙串使用凭据管理器（`keyring` 的 `windows-native` 后端）。

### 3.3 Linux X11 / Wayland

- 钥匙串依赖 Secret Service（GNOME Keyring、KWallet 等）。启动时检测是否可用；不可用时隐藏“记住密码”选项并说明原因，不退回到明文存储。
- Tauri 拖放在 X11 和 Wayland 下都依赖 WebKitGTK，Wayland 下的行为需要实测；不可用时仍可通过“选择文件”按钮上传。

## 4. 用户界面

### 4.1 工具页

侧栏新增“SSH”入口，路由 `/ssh`，和剪贴板、截图一样可在设置中启用或关闭。

页面结构：

- 左侧：主机列表，按分组折叠，顶部搜索框和“新建 / 导入”按钮。每项显示名称、`user@host:port`、跳板机标记；双击或回车连接。
- 右侧：终端标签页（Ant Design `Tabs` 的 `editable-card`）。标签显示主机名和连接状态点；同一主机可开多个标签。
- 底部：可折叠的传输面板，列出进行中和已完成的任务。
- 空状态：区分“还没有主机”和“没有打开的会话”。

### 4.2 主机表单

| 字段 | 说明 |
|---|---|
| 名称 | 默认取主机地址 |
| 分组 | 可选 |
| 主机、端口 | 端口默认 22，校验 1–65535 |
| 用户名 | 必填 |
| 认证方式 | 密码 / 私钥 / agent / 自动（依次尝试 agent、私钥、密码） |
| 私钥路径 | 通过系统对话框选择，只保存路径 |
| 记住密码或口令 | 钥匙串可用时显示，默认关闭 |
| 跳板机 | 从已有主机中选择，可多级，禁止形成环 |
| 初始目录 | 可选，连接后执行 `cd` |
| 终端编码与 `TERM` | 默认 UTF-8、`xterm-256color` |

### 4.3 连接过程

会话状态：`connecting → verifying → authenticating → ready → closed`。

- **未知指纹：** 弹窗显示主机、算法和 SHA256 指纹，用户选择“信任并连接”或“取消”。信任后写入 Pixo 自己的 known_hosts。
- **指纹变化：** 显示旧指纹和新指纹，说明可能存在中间人攻击，只提供“取消”。用户需要在主机设置里手动删除旧指纹后才能重新连接。
- **需要输入：** 密码、私钥口令和 keyboard-interactive 提示都通过同一个输入弹窗完成。输入框为密码类型；钥匙串可用时附带“记住”选项。
- **跳板机：** 按顺序逐台连接，每一跳都单独做指纹校验和认证；状态栏显示当前进行到哪一跳。
- **断开：** 终端保留已有输出，显示断开原因和“重新连接”按钮。

### 4.4 终端

- 使用 xterm.js，配合自适应尺寸插件和搜索插件。终端工具栏可以调整配色、对比度、字体、字号、行高、字距和光标；默认跟随界面明暗，自定义配色保存在本机。高亮颜色作用在远程程序发出的 ANSI 颜色上。
- 复制粘贴快捷键见第 3 节；终端内的 `Ctrl+C` 始终发给远程程序。右键菜单提供复制、粘贴、全选、清屏、搜索。
- 粘贴多行文本前提示确认，避免误执行命令；可在设置中关闭这项提示。
- 当 shell 通过 OSC 7 报告当前目录时，记录为该会话的“当前远程目录”，作为上传的默认目标。

### 4.5 文件传输

上传：

- 入口有两个：终端工具栏选择文件或文件夹，以及把文件拖到当前终端。
- 直接传到这个会话的当前远程目录。目录优先来自 shell 报告的 OSC 7，其次是本机记录的 `cd` 和初始目录；都没有时用 SFTP 解析出的主目录。工具栏会显示目标目录。
- 文件夹递归上传，在远程保留目录结构；不跟随本地符号链接，跳过的项目计入任务结果。

下载：

- 入口为“下载”按钮，在弹窗中输入远程文件或目录路径（输入框支持对远程目录做 Tab 补全）；本地保存位置通过系统对话框选择。
- 目录递归下载；远程符号链接不跟随。

通用规则：

- 同名冲突在任务开始前统一询问一次：覆盖、自动重命名（`name (1).ext`）、跳过；可勾选“对本次任务全部适用”。默认选“自动重命名”。
- 传输面板显示文件名、已传 / 总大小、速度、剩余时间，以及取消和重试按钮。
- 会话断开时，进行中的任务标记为失败，可在重新连接后重试。

### 4.6 设置

| 设置 | 默认值 | 说明 |
|---|---|---|
| 启用 SSH 工具 | 关闭 | 与其他工具一致 |
| 终端字体、字号 | 系统等宽字体、14 | |
| 回滚行数 | 10000 | 1000–100000 |
| 多行粘贴确认 | 开启 | |
| 终端复制不进入剪贴板历史 | 开启 | 复制时调用剪贴板模块的“忽略下一次” |
| 默认冲突策略 | 自动重命名 | |
| 读取 `~/.ssh/known_hosts` | 开启 | 只读 |
| 保持连接间隔 | 30 秒 | 0 表示关闭 |

### 4.7 无障碍与国际化

- 主机列表、标签页、传输面板都可以只用键盘操作；图标按钮提供 `aria-label`。
- 终端开启 xterm 的屏幕阅读器模式选项；提供 `Ctrl/Cmd+Shift+E` 让焦点离开终端回到标签栏，避免键盘被困在终端里。
- 文案使用 `zdx.label.ssh.*`、`zdx.text.ssh.*`、`zdx.placeholder.ssh.*`，例如 `zdx.label.ssh.upload`、`zdx.text.ssh.hostkey.changed`、`zdx.placeholder.ssh.host`；中英文同步维护。
- 文件大小、速度、剩余时间使用 `Intl.NumberFormat` 并跟随当前语言。
- 终端输出、主机名、路径不翻译。

## 5. 架构

```
xterm.js（TerminalView）
   │ 输入：ssh_write / ssh_resize
   │ 输出：Channel（原始字节）+ Channel（会话事件 JSON）
   ▼
SshState（Rust）
   ├─ Session：russh 连接 → 跳板机链路 → PTY shell 通道
   ├─ Transfer：russh-sftp 上传 / 下载队列
   ├─ HostStore：SQLite 主机配置
   ├─ KnownHosts：指纹校验
   └─ Secrets：系统钥匙串
```

### 5.1 Rust 模块

```
src-tauri/src/
├── lib.rs                 初始化 SshState、注册命令
└── ssh/
    ├── mod.rs             对外接口：SshState、initialize
    ├── commands.rs        Tauri 命令，只做参数校验和错误转换
    ├── error.rs           SshError { code, params }，与 ClipboardError 一致
    ├── model.rs           主机配置、会话状态、传输任务等数据结构
    ├── store.rs           主机配置与设置的 SQLite 读写
    ├── config_import.rs   ~/.ssh/config 子集解析
    ├── known_hosts.rs     known_hosts 解析、匹配、写入
    ├── auth.rs            密码、私钥、agent、keyboard-interactive
    ├── agent.rs           各平台 agent 连接
    ├── secret.rs          钥匙串读写与可用性检测
    ├── session.rs         连接、跳板机链路、PTY、保活、关闭
    ├── local_paths.rs     记录来自对话框和拖放的本地路径授权
    └── transfer/
        ├── mod.rs         任务队列、进度、取消
        ├── upload.rs
        ├── download.rs
        └── naming.rs      冲突重命名、远程路径拼接
```

### 5.2 前端

```
src/
├── api/ssh.ts                     invoke 与 Channel 封装（显式导入 Tauri API）
├── types/ssh.ts                   与 Rust 对应的类型
├── stores/ssh.ts                  打开的标签页、会话状态、传输任务（zustand）
├── hooks/useTerminal.ts           xterm 实例的创建、尺寸同步、销毁
├── pages/Ssh.tsx                  工具页
├── components/ssh/
│   ├── HostList.tsx
│   ├── HostForm.tsx
│   ├── ImportConfig.tsx
│   ├── TerminalView.tsx
│   ├── HostKeyPrompt.tsx
│   ├── CredentialPrompt.tsx
│   ├── TransferDialog.tsx         目标路径、冲突策略
│   └── TransferPanel.tsx
└── lang/zh-CN.json、en.json
```

xterm 不属于框架 API，按项目规则显式导入，不加入自动导入配置。新增使用的 Ant Design 组件和图标加入 `vite.config.ts` 的自动导入配置。

### 5.3 终端数据通道

- `ssh_connect` 接收两个 `tauri::ipc::Channel`：`output` 使用 `InvokeResponseBody::Raw` 传原始字节，`events` 传 JSON 事件（状态变化、指纹确认、输入提示、OSC 7 目录、断开原因）。
- Rust 侧把输出按 16 ms 或 64 KB 合并后再发送，降低 IPC 次数。
- 流控：前端在 xterm 的写入回调完成后调用 `ssh_ack(session_id, bytes)`。未确认的数据超过 1 MB 时，Rust 暂停读取远程通道，避免 `cat` 大文件时撑爆内存。
- 输入：xterm 的 `onData` 文本以 UTF-8 发送；`onBinary` 以字节发送。

### 5.4 窗口与生命周期

- 会话属于主窗口。后台模式下关闭主窗口只是隐藏，会话保持；窗口被销毁或应用退出时断开所有会话。
- 退出应用或关闭主窗口时，如果有活跃会话或进行中的传输，先提示确认。
- 不新增窗口，不新增 capability 权限；对话框通过 Rust 侧的 `tauri-plugin-dialog` 弹出。

### 5.5 依赖

| 依赖 | 用途 |
|---|---|
| `russh` | SSH 客户端、密钥解析、agent 客户端 |
| `russh-sftp` | SFTP 客户端 |
| `tokio`（按需开启 feature） | 异步运行时相关类型；运行时复用 `tauri::async_runtime` |
| `keyring` | 系统钥匙串 |
| `rusqlite` | 已有，主机配置存储 |
| `tauri-plugin-dialog` | 已有，选择文件、文件夹、私钥和保存位置 |
| `@xterm/xterm`、`@xterm/addon-fit`、`@xterm/addon-search` | 终端渲染 |

`~/.ssh/config` 只需要第 6.4 节列出的子集，自己实现解析并配测试，不引入额外依赖。

## 6. 数据与存储

### 6.1 位置

- 数据库：`<app_data_dir>/ssh/hosts.db`，开启 WAL。
- 指纹：`<app_data_dir>/ssh/known_hosts`，OpenSSH 格式；用户的 `~/.ssh/known_hosts` 只读不写。
- 钥匙串条目：服务名 `pixo.ssh`，账户名为 `<host_id>:password` 或 `<host_id>:passphrase`。

### 6.2 表结构

```sql
CREATE TABLE ssh_hosts (
  id            INTEGER PRIMARY KEY,
  name          TEXT    NOT NULL,
  group_name    TEXT,
  host          TEXT    NOT NULL,
  port          INTEGER NOT NULL DEFAULT 22,
  username      TEXT    NOT NULL,
  auth          TEXT    NOT NULL CHECK (auth IN ('auto', 'password', 'key', 'agent')),
  key_path      TEXT,
  remember      INTEGER NOT NULL DEFAULT 0,
  start_dir     TEXT,
  term          TEXT    NOT NULL DEFAULT 'xterm-256color',
  sort_order    INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE ssh_jumps (
  host_id   INTEGER NOT NULL REFERENCES ssh_hosts(id) ON DELETE CASCADE,
  position  INTEGER NOT NULL,
  jump_id   INTEGER NOT NULL REFERENCES ssh_hosts(id) ON DELETE RESTRICT,
  PRIMARY KEY (host_id, position)
);

CREATE TABLE ssh_settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL              -- JSON
);
```

- 删除被其他主机用作跳板机的主机时，提示有哪些主机依赖它，用户确认后先移除依赖再删除。
- 删除主机时同时删除其钥匙串条目；Pixo known_hosts 中的指纹保留，因为同一地址可能被其他主机配置使用。
- 数据库带 `user_version`，启动时按版本迁移。

### 6.3 指纹校验

1. 按 `[host]:port`（端口为 22 时为 `host`）查找 Pixo known_hosts，再查找用户 known_hosts；支持哈希主机名（`|1|`）。
2. 找到且一致：继续认证。
3. 找到但不一致：返回 `hostkey_changed`，附带新旧指纹。
4. 未找到：发出指纹确认事件，等待 `ssh_hostkey_decide`；120 秒无响应视为取消。
5. 通过跳板机连接时，按目标主机自身的地址校验，不按跳板机地址。

### 6.4 `~/.ssh/config` 导入

- 支持的关键字：`Host`（只导入不含通配符的条目）、`HostName`、`User`、`Port`、`IdentityFile`、`ProxyJump`、`Include`。
- `IdentityFile` 中的 `~` 和 `%d`、`%u`、`%h` 会被展开；不支持的关键字在导入预览中列出并跳过。
- 导入前展示预览，用户勾选要导入的条目；和已有主机重复（主机、端口、用户名都相同）时默认不勾选。
- `ProxyJump` 引用的主机需要一起导入；找不到时导入该条目但不设置跳板机，并在预览中标注。

### 6.5 传输

- 分块大小 256 KB，最多 4 个并发请求；进度每 200 ms 通过 Channel 推送一次。
- 上传写入远程 `<name>.pixo-part`，完成后 `rename`；如果服务器不支持覆盖式 rename 且策略是覆盖，先删除旧文件再 rename，失败时保留 `.pixo-part` 并报错。
- 下载写入本地 `<name>.pixo-part`，完成后重命名；取消或失败时删除临时文件。
- 远程路径统一使用 `/` 拼接；文件名中的 `/` 和 `\0` 视为非法；不允许 `..` 逃出用户选择的目标目录。
- 上传时保留文件的修改时间，不同步权限位，远程文件使用服务器默认权限。

## 7. 隐私与安全

- **本地路径授权：** `local_paths.rs` 记录由 Rust 对话框和窗口拖放事件得到的路径，并返回一次性令牌；传输命令只接受令牌，不接受原始路径。令牌在任务开始后失效。
- **秘密处理：** 密码和口令在 Rust 中使用后立即清零（`zeroize`）；不写日志、不进 SQLite、不回传前端。
- **剪贴板：** 默认情况下，从终端复制的内容不进入 Pixo 剪贴板历史；凭据输入框禁止复制。
- **权限最小化：** 不引入 shell 插件，不新增前端可直接调用的 fs、dialog 或网络权限。
- **算法：** 使用 `russh` 默认的安全算法集合；默认不启用 `ssh-rsa`（SHA-1）签名和 CBC 加密模式。如果为了兼容旧服务器需要开启，放到第二版作为单台主机的显式选项。
- **日志：** 只记录主机 ID、阶段和错误码，不记录用户名以外的认证信息，也不记录终端内容和文件路径。

## 8. 命令与事件

### 8.1 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `ssh_status` | 无 | 工具开关、钥匙串和 agent 可用性 |
| `ssh_set_enabled` | `enabled` | 无 |
| `ssh_get_settings` / `ssh_update_settings` | 设置对象 | 设置对象；校验取值范围 |
| `ssh_hosts_list` | 无 | 主机列表（不含秘密） |
| `ssh_host_save` | 主机对象、`secret?` | 主机对象；校验字段并检查跳板机环 |
| `ssh_host_delete` | `id` | 无 |
| `ssh_host_forget_key` | `id` | 无；删除 Pixo known_hosts 中的对应指纹 |
| `ssh_pick_key` | 无 | 私钥路径或空 |
| `ssh_config_preview` | 无 | 可导入条目与警告 |
| `ssh_config_import` | 条目序号列表 | 新建的主机 |
| `ssh_connect` | `hostId`、`cols`、`rows`、`output`、`events` | 会话 ID |
| `ssh_hostkey_decide` | `sessionId`、`accept` | 无 |
| `ssh_answer` | `sessionId`、`promptId`、`answers`、`remember` | 无 |
| `ssh_write` | `sessionId`、`data` | 无 |
| `ssh_ack` | `sessionId`、`bytes` | 无 |
| `ssh_resize` | `sessionId`、`cols`、`rows` | 无 |
| `ssh_disconnect` | `sessionId` | 无 |
| `ssh_pick_upload` | `kind`（文件 / 文件夹） | 本地路径令牌与显示名称 |
| `ssh_pick_download_dir` | 无 | 本地路径令牌与显示名称 |
| `ssh_remote_complete` | `sessionId`、`path` | 补全候选（最多 50 条） |
| `ssh_transfer_start` | `sessionId`、方向、本地令牌、远程路径、冲突策略、`progress` | 任务 ID |
| `ssh_transfer_cancel` | `taskId` | 无 |

窗口拖放由 Rust 监听 `WindowEvent::DragDrop`，通过事件把令牌和显示名称发给前端。

### 8.2 事件

- 会话 `events` Channel：`state`、`hostkey`、`prompt`、`cwd`、`closed`。
- 传输 `progress` Channel：`started`、`progress`、`file_done`、`finished`、`failed`。
- `ssh://dropped`：拖放得到的本地路径令牌。

### 8.3 错误

Rust 返回 `{ code, params }`，前端翻译为 `zdx.text.ssh.error.<code>`：

| code | 含义 |
|---|---|
| `invalid_input` | 参数校验失败 |
| `not_found` | 主机、会话或任务不存在 |
| `jump_cycle` | 跳板机形成环 |
| `resolve_failed` | 域名解析失败 |
| `connect_failed` | 连接被拒绝或网络不可达 |
| `timeout` | 连接或认证超时 |
| `hostkey_rejected` | 用户拒绝了未知指纹 |
| `hostkey_changed` | 指纹与记录不一致 |
| `auth_failed` | 所有认证方式都失败 |
| `key_unreadable` | 私钥文件不存在、格式不支持或口令错误 |
| `agent_unavailable` | 找不到可用的 ssh-agent |
| `keyring_unavailable` | 系统钥匙串不可用 |
| `disconnected` | 会话已断开 |
| `sftp_unavailable` | 服务器未启用 SFTP 子系统 |
| `remote_permission` | 远程路径无权限 |
| `remote_missing` | 远程路径不存在 |
| `local_permission` | 本地路径无权限 |
| `token_expired` | 本地路径令牌无效或已使用 |
| `disk_full` | 本地或远程空间不足 |
| `too_many` | 一次选择的文件过多 |
| `transfer_failed` | 单个文件没有传完 |
| `cancelled` | 用户取消 |
| `storage` | 数据库或钥匙串读写失败 |

内部错误详情只写日志，不直接展示。

## 9. 测试与验证

自动化测试：

- Rust 单元测试：known_hosts 解析与匹配（含哈希主机名、非默认端口）、`~/.ssh/config` 子集解析与 `Include`、跳板机环检测、冲突重命名、远程路径拼接与 `..` 拦截、主机配置读写与迁移、令牌一次性使用。
- Rust 集成测试：在测试进程内用 `russh` 的服务端启动 SSH 和 SFTP 服务器，覆盖密码和私钥认证、未知指纹等待确认、指纹变化被拦截、单级跳板机、上传下载、取消后不残留文件、断线后任务失败。不依赖外部 sshd，三个平台的 CI 都能运行。
- 前端测试：大小、速度格式化，传输状态归并等纯函数。
- 构建：`pnpm build`、`cargo check --manifest-path src-tauri/Cargo.toml`、`cargo test --manifest-path src-tauri/Cargo.toml`。

各平台手动验证：

| 场景 | macOS | Windows | X11 | Wayland |
|---|---|---|---|---|
| 连接 OpenSSH 服务器，运行 `vim`、`htop`，调整窗口大小 | ✓ | ✓ | ✓ | ✓ |
| agent 认证（Finder / 开始菜单启动） | ✓ | OpenSSH、Pageant | ✓ | ✓ |
| 记住密码后重启仍可自动登录 | ✓ | ✓ | ✓ | 无 Secret Service 时显示不可用 |
| 拖放上传 | ✓ | ✓ | ✓ | 需实测 |
| 上传、下载 1 GB 文件与 1 万个小文件的目录 | ✓ | ✓ | ✓ | ✓ |
| 中文、emoji、宽字符显示 | ✓ | ✓ | ✓ | ✓ |
| 后台模式下关闭窗口后会话保持 | ✓ | ✓ | ✓ | ✓ |

## 10. 实施阶段

1. **主机与连接：** 数据模型、SQLite 存储、主机列表与表单、指纹校验、密码和私钥认证、单个终端标签、错误提示。
2. **终端完善：** 多标签、流控、复制粘贴与剪贴板历史联动、搜索、重连、OSC 7、设置项。
3. **凭据：** agent（三平台）、keyboard-interactive、钥匙串与可用性检测。
4. **跳板机与导入：** 多级跳板机、`~/.ssh/config` 预览与导入。
5. **传输：** 本地路径令牌、上传下载文件、文件夹递归、冲突策略、传输面板、拖放、远程路径补全。

每个阶段独立可用，并完成对应验证后再进入下一阶段。

## 11. 待定问题

- 第二版的远程目录浏览器是否放在终端侧边，以及是否支持远程文件的重命名和删除。
- 是否需要为旧服务器提供 `ssh-rsa`、CBC 等兼容开关。
- `russh` 对 Windows OpenSSH 命名管道和 Pageant 的支持程度，需要实现前实测。
- Wayland 下 WebKitGTK 拖放的可靠性，需要在 GNOME 和 KDE 上实测。
- 主机配置是否需要导出 / 导入为文件（不含秘密）。
