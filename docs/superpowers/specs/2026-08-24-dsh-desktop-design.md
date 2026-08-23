# DSH 桌面版（dsh-desktop）设计规格

- 日期：2026-08-24
- 状态：已批准（头脑风暴分节确认）
- 方案：Tauri v2 壳（Rust）+ 内置 Node sidecar

## 1. 目标与背景

将当前使用的 DSH（`@deepseek-ai/dsh@0.1.1-rc.2`，Web GUI 模式）封装为原生桌面应用，供公司内部同事在 macOS / Windows / Linux 上零环境安装使用。

现状：`dsh web`（`--profile web` 别名）在 `127.0.0.1:3080` 启动本地 Web 服务，用户在浏览器中使用 Web GUI。数据存于 `~/.dsh`。用户的 web profile 额外安装了 `dsh-plugin-market` 插件（github:chnjames/dsh-plugin-market）。

### 1.1 需求确认（头脑风暴结论）

| 项目 | 决定 |
|---|---|
| 平台 | macOS + Windows + Linux，公司内部发行 |
| 运行时 | 完全自包含：内置 Node 运行时 + dsh 包，同事零安装 |
| 数据目录 | 默认复用 `~/.dsh`；设置页可配置，通过 `DSH_HOME` 注入 |
| 系统集成 | 托盘常驻、开机自启、单实例、正式安装包（dmg/msi-exe/deb-AppImage）、应用设置页 |
| 工作目录 | 首次启动弹原生目录选择器选择并记住，后续直接使用 |
| Profile | 内置一份与现状一致的 web profile（官方 bundles + 插件市场），开箱即用 |
| 自动更新 | 启用（Tauri updater，内网静态服务器）；第一版跳过系统级代码签名/公证 |
| 命名 | 应用名「DSH 桌面版」，标识 `dsh-desktop` |

### 1.2 明确不做（YAGNI）

- macOS 公证 / Windows 代码签名：第一版跳过，配置预留签名位
- 多语言：仅中文界面
- 更新通道多版本并行：默认 stable，beta 通道仅留配置位

## 2. 总体架构

```
┌─────────────────────────────────────────────────┐
│                DSH 桌面版 (Tauri v2)              │
│                                                 │
│  ┌─────────────────────────────────────────┐    │
│  │  Rust 主进程 (src-tauri)                 │    │
│  │  ┌─────────┐ ┌────────┐ ┌────────────┐  │    │
│  │  │ 主窗口   │ │ 设置窗口 │ │ 托盘/单实例  │  │    │
│  │  │ (WebView)│ │(WebView)│ │ /自启/生命周期│ │    │
│  │  └────┬────┘ └───┬────┘ └─────┬──────┘  │    │
│  │       │          │            │          │    │
│  │  ┌────▼──────────▼────────────▼───────┐  │    │
│  │  │  Sidecar 管理器                      │  │    │
│  │  │  · spawn/kill node + dsh            │  │    │
│  │  │  · 端口探测/就绪检测                  │  │    │
│  │  │  · DSH_HOME / cwd 注入              │  │    │
│  │  └───────────────┬────────────────────┘  │    │
│  └──────────────────┼───────────────────────┘    │
│                     │ spawn                      │
│  ┌──────────────────▼───────────────────────┐    │
│  │  Sidecar：内置 node v24 + dsh CLI         │    │
│  │  node .../dsh/lib/bin.js --profile web    │    │
│  │  --port <N>                              │    │
│  │  （cwd = 用户工作目录）                    │    │
│  └──────────────────┬───────────────────────┘    │
│                     │ HTTP                        │
│  ┌──────────────────▼───────────────────────┐    │
│  │  主窗口 WebView 加载 http://127.0.0.1:N  │    │
│  └─────────────────────────────────────────┘    │
└─────────────────────────────────────────────────┘
```

### 2.1 数据流

1. 应用启动 → 单实例检查（已有实例则聚焦主窗口并退出）
2. 读取应用配置（工作目录、数据目录、端口偏好）
3. 若工作目录未设置：弹原生目录选择器，用户选择并记住
4. Sidecar 管理器探测空闲端口（偏好 3080，占用则递增）
5. spawn sidecar：`node <dsh>/lib/bin.js --profile web --port N`，env 注入 `DSH_HOME`（仅自定义时），cwd = 工作目录
6. 轮询 `http://127.0.0.1:N` 直到就绪（30s 超时）
7. 主窗口加载 `http://127.0.0.1:N`（与浏览器中完全相同的 DSH Web UI）

### 2.2 进程边界

- 主窗口与设置窗口均为 Tauri WebView（系统 WebView：macOS WKWebView / Windows WebView2 / Linux WebKitGTK）
- 主窗口只做 DSH Web UI 的浏览器壳，不注入前端代码
- 设置页为独立小窗口，通过 Tauri IPC 调 Rust commands 管理应用级设置
- sidecar 与主进程同生命周期；退出时 kill 整个进程树（node 及其派生的 bash/pwsh 子进程）

## 3. 内置运行时与打包结构

### 3.1 Sidecar 三组件

| 组件 | 内容 | 来源 |
|---|---|---|
| Node 运行时 | 官方 Node v24.x 二进制本体（darwin-x64/arm64、win-x64、linux-x64） | 构建时下载官方发行包，仅取 node 二进制 |
| dsh 包 | `@deepseek-ai/dsh@0.1.1-rc.2` + 全部 node_modules | 构建时 npm ci（锁定版本） |
| profile | 内置 web profile 模板（官方 bundles + dsh-plugin-market） | 仓库内维护，构建时复制 |

Node 二进制本体自包含（ICU/v8 静态链接），dsh 依赖全在自己的 node_modules，无需 npm 本体，sidecar 体积 ~60MB。

### 3.2 版本一致性（关键风险）

dsh 的 native 模块（node-pty、koffi、sharp 等）针对特定 Node ABI 编译。规则：

- 内置 Node 版本 = 构建时安装 dsh 依赖所用的 Node 版本（v24.x 最新）
- 构建脚本在目标平台上现场执行 npm ci，确保 prebuilds 与平台/Node 版本匹配
- **必须按平台分别构建**，不能跨平台拷贝构建产物

### 3.3 项目目录结构

```
dsh-desktop/
├── src-tauri/            # Rust 壳（主进程、sidecar 管理、托盘、设置）
├── ui/                   # 设置页前端（轻量 HTML/JS，非 DSH UI）
├── bundle/
│   ├── node/             # 构建时填充：各平台 node 二进制
│   ├── dsh/              # 构建时填充：dsh 包 + node_modules
│   └── profile/          # 内置 web profile 模板（含插件市场）
├── scripts/
│   ├── prepare-bundle.sh # 下载 node / npm ci / 组装 profile
│   └── build-all.sh      # 按平台调用 tauri build
└── README.md
```

### 3.4 安装包产物

- macOS：`.dmg`（+ `.app`）
- Windows：`.msi` / NSIS `.exe`
- Linux：`.deb` / `.AppImage`

## 4. 生命周期与系统集成

### 4.1 启动流程

见 §2.1。

### 4.2 运行期行为

- 主窗口 = DSH Web UI 壳；托盘图标常驻
- 托盘菜单：显示主窗口 / 打开设置 / 退出
- 关窗默认最小化到托盘（不退出 sidecar，会话保持运行）；可通过设置改为直接退出
- sidecar 意外退出：弹窗提示 + "重启服务"按钮；主窗口显示错误页而非白屏

### 4.3 退出流程

```
托盘"退出"或系统关机
  → kill sidecar 整个进程树（node + 派生子进程）
  → 等待退出确认（超时 5s 强杀）
  → 退出应用
```

### 4.4 系统集成实现（Tauri v2 官方插件，三平台支持）

| 特性 | 实现 |
|---|---|
| 单实例 | `tauri-plugin-single-instance` |
| 开机自启 | `tauri-plugin-autostart`（设置页开关） |
| 托盘 | Tauri v2 tray-icon 内置能力 |
| 目录选择 | `tauri-plugin-dialog` |
| 子进程 | Rust std::process + 进程树 kill（优先 tauri-plugin-shell 评估） |

### 4.5 错误处理边界

- 端口全被占用：报错提示关闭占用端口的程序
- Node/dsh 缺失或损坏：报错提示重新安装
- 数据目录不可写：启动前检查，报错引导到设置页修改

## 5. 设置页与应用设置

### 5.1 形态与边界

独立小窗口（`ui/` 轻量 HTML/JS），Tauri IPC 调 Rust commands 读写设置。应用级设置与 DSH 内部设置（模型、API key 等在 DSH 主窗口设置里管）完全分离。

### 5.2 设置项（app-config.json，系统标准配置目录）

| 设置项 | 说明 | 默认值 |
|---|---|---|
| 工作目录 | workspace | 首次启动选择 |
| 数据目录 | DSH_HOME | `~/.dsh`（不注入） |
| 端口偏好 | 期望端口，占用自动递增 | 3080 |
| 开机自启 | 登录自动启动服务 | 关闭 |
| 关闭行为 | 最小化到托盘 / 直接退出 | 最小化到托盘 |
| 更新通道 | stable / beta | stable |
| 自动检查更新 | 启动时静默检查 | 开 |
| 服务日志 | sidecar stdout/stderr 只读查看 | 存系统日志目录 |

### 5.3 交互规则

- 工作目录、数据目录：显示当前值 + 更改按钮（原生目录选择器）；更改后**下次启动生效**（不热切换）
- 端口、关闭行为、更新项：即时保存生效
- 所有修改即时原子写（临时文件 + rename）

### 5.4 安全边界

- 设置窗口 CSP 只允许本应用资源
- Rust commands 白名单校验：目录必须存在、端口 1024–65535

## 6. 构建、分发与自动更新

### 6.1 构建流程

```
scripts/prepare-bundle.sh（在目标平台上执行）
  1. 下载官方 Node v24.x 发行包 → 解出 node 二进制 → bundle/node/
  2. npm ci 安装 dsh@0.1.1-rc.2 及依赖 → bundle/dsh/
  3. 复制内置 profile 模板 → bundle/profile/
  4. 校验：node 版本、dsh dry-run 可启动、native prebuilds 存在

cargo tauri build（tauri-bundler）
  → macOS:  .dmg + .app
  → Windows: .msi + NSIS .exe
  → Linux:  .deb + .AppImage
```

### 6.2 自动更新（tauri-plugin-updater）

| 平台 | 更新格式 | 要求 |
|---|---|---|
| macOS | `.dmg` + 更新签名 | updater 签名（自备密钥对） |
| Windows | NSIS `.exe` | updater 签名 |
| Linux | `.AppImage` | updater 签名 |

- **更新签名 ≠ 系统代码签名**：Tauri updater 用自备非对称密钥对给更新文件签名（公钥打进应用，私钥留构建机），不依赖苹果/微软证书
- 更新服务器：公司内网静态目录 + `latest.json` 元数据（备选：GitHub Releases）
- 流程：启动静默检查（可关）+ 设置页手动"检查更新" → 对比版本 → 弹窗确认 → 下载 → 校验签名 → 安装 → 重启
- 失败处理：保留旧版本可用，提示重试/联系管理员
- 新增依赖：`tauri-plugin-updater`（+ `tauri-plugin-process` 更新后重启）
- 构建脚本增加：生成签名密钥对 → 每版签名 → 生成 `latest.json`

### 6.3 签名与公证

- 第一版不做 macOS 公证 / Windows 代码签名（内部信任分发），配置预留签名位
- updater 签名第一版即启用（自动更新必需）

## 7. 验证计划

1. **本机 macOS 验证**：启动 → 选目录 → 主窗口加载 → 会话对话 → 插件市场 → 关窗托盘 → 重启服务 → 退出杀进程树
2. **端口冲突**：先手动占 3080 再启动，验证自动递增
3. **数据目录**：切换自定义 DSH_HOME 后会话隔离正确
4. **自动更新**：本地 `http.server` 模拟更新服务器，验证版本升级、签名校验失败提示、断网降级可用
5. **Windows / Linux**：交付构建脚本，需在对应系统上跑通（用户提供机器或后续接 CI，如 GitHub Actions 三平台矩阵）
6. **回归**：内置 profile 与现状 `~/.dsh` 一致（插件市场可用）

## 8. 技术栈与依赖清单

- Tauri v2（Rust 壳）+ tauri-bundler
- Rust 工具链（rustup/cargo）+ macOS 构建依赖（Xcode CLT）
- 插件：single-instance、autostart、dialog、updater、process、（评估 shell）
- Node v24.x 官方发行包（按平台）
- `@deepseek-ai/dsh@0.1.1-rc.2`（npm 锁定）
- 设置页：轻量 HTML/JS（无框架或极简，待实现计划定）

## 9. 遗留待定项（进入实现计划时确定）

- 设置页前端技术选型（原生 HTML/JS vs 极简框架）
- sidecar 就绪探测路径（/health 是否存在，或改用 stdout URL line 解析）
- 进程树 kill 的具体实现（Rust 侧 taskkill/pkill vs 插件）
- 更新服务器内网地址（需要用户提供）
- 应用图标设计（先用 Tauri 默认图标占位）
