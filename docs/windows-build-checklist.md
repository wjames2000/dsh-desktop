# Windows 构建清单（DSH 桌面版 dsh-desktop）

> 适用版本：v0.1.0（main 分支 2539f19 及之后）
> 目标产物：`DSH 桌面版_0.1.0_x64.msi` / `DSH 桌面版_0.1.0_x64-setup.exe`（NSIS）
> 前置事实：macOS 版已构建并端到端验证通过；Windows 版**尚未实机验证**，本清单是首次构建的完整指引。

---

## 0. 原理速览

桌面应用 = Tauri 壳 + sidecar（内置 Node + dsh 包 + profile 三件套）。构建分两步：

1. `scripts/prepare-bundle.sh` —— 组装 sidecar 资源到 `bundle/`（Windows 上会下载 `win-x64/node.exe` 单文件、`npm ci` 安装 dsh、安装 profile 依赖）
2. `cargo tauri build` —— 编译 Rust 壳并把 `bundle/` 三件套打进安装包（tauri.conf.json 的 `bundle.resources` 已指向 `../bundle/`）

**关键约束**：native 模块（node-pty、koffi 等）按平台编译，**必须在 Windows 上构建**，不能把 macOS 产物拷过去。

---

## 1. 构建机前置条件（Windows 10/11 x64）

| 依赖 | 版本 | 安装方式 | 用途 |
|---|---|---|---|
| **Git for Windows** | 最新（含 Git Bash） | [git-scm.com](https://git-scm.com/download/win) | 提供 bash 运行构建脚本；**必须**（脚本是 bash） |
| **Rust 工具链** | stable（≥1.88，推荐 1.98） | [rustup.rs](https://rustup.rs/)（.exe 安装器） | 编译 Tauri 壳。装完重启终端使 `cargo` 进 PATH |
| **Node.js** | **v24.x（建议与 macOS 构建机一致，如 24.15.0）** | [nodejs.org](https://nodejs.org/dist/)（win-x64 .msi） | 运行 `npm ci` / `npm install`（prepare-bundle.sh 用它装 dsh） |
| **Microsoft C++ Build Tools** | VS 2022 的 "Desktop development with C++" 工作负载 | [visualstudio.microsoft.com/visual-cpp-build-tools/](https://visualstudio.microsoft.com/visual-cpp-build-tools/) | 编译 Rust 依赖里的 C 组件（如 node-pty 的编译、ring 等） |
| **WebView2 Runtime** | 系统自带（Win10 1803+ / Win11） | 一般无需安装；若缺失从 [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/) 下载 Evergreen 版 | Tauri 渲染引擎 |
| **NSIS** | 由 tauri-bundler 自动下载 | 无需手动装 | NSIS 安装包生成 |
| **WiX Toolset（可选）** | 仅当要产出 `.msi` 时 | [wixtoolset.org](https://wixtoolset.org/) v3（tauri 默认用它） | MSI 安装包生成 |

**网络注意**（与 macOS 构建相同的坑）：
- `prepare-bundle.sh` 默认 `REGISTRY=https://registry.npmmirror.com`（npm 镜像，国内快）
- Node 官方发行包从 nodejs.org 下载——若网络受限可预先手动下载 `node.exe` 放到 `bundle/node/node.exe`，脚本会直接使用（脚本用 `curl -o` 覆盖，可跳过）
- Rust crate 下载走 crates.io，若慢可配 rsproxy.cn 镜像（见第 3 节）

---

## 2. 获取代码

```powershell
# 任选其一
git clone https://github.com/wjames2000/dsh-desktop.git
# 或从已有仓库 pull 最新
git pull origin main
```

---

## 3. 安装 Tauri CLI v2

```bash
# 在 Git Bash 中执行
cargo install tauri-cli --version "^2" --locked
# 验证
cargo tauri --version   # 期望 2.x
```

> 编译 tauri-cli 约 30-50 分钟。若 crates.io 下载慢，在 `C:\Users\<你>\.cargo\config.toml` 配置镜像：
>
> ```toml
> [source.crates-io]
> replace-with = "rsproxy-sparse"
>
> [source.rsproxy-sparse]
> registry = "sparse+https://rsproxy.cn/index/"
> ```

---

## 4. 构建

```bash
# 在 Git Bash 中，进入仓库根目录
cd /c/path/to/dsh-desktop     # 或 cd "dsh-desktop"（Git Bash 的路径写法）

# 可选：显式指定 Node 版本（默认取构建机 node 版本，与 macOS 一致更稳）
export NODE_VERSION=24.15.0

# 一键构建（prepare-bundle + cargo tauri build）
./scripts/build-all.sh
```

### 4.1 构建分步（等价于 build-all.sh，便于排查）

```bash
# 第一步：组装 sidecar 资源（下载 node.exe、npm ci 装 dsh、装 profile 依赖）
./scripts/prepare-bundle.sh
# 预期尾部输出：
#   >> 平台: win-x64, Node 24.15.0, dsh 0.1.2-rc.1
#   >> 校验 node 版本        → v24.15.0
#   >> 校验 dsh 可启动        → 无报错
#   >> 校验 native prebuilds  → 无 WARN
#   >> 完成。bundle 目录：...

# 第二步：编译 + 打包
cd src-tauri
cargo tauri build
```

### 4.2 预期产物

```
src-tauri/target/release/bundle/
├── nsis/
│   └── DSH 桌面版_0.1.0_x64-setup.exe   ← NSIS 安装包（推荐分发）
├── msi/
│   └── DSH 桌面版_0.1.0_x64.msi          ← 若装了 WiX 才有
└── dsh-desktop.exe                       ← 免安装版（也可直接拷出运行）
```

---

## 5. 构建后验证（必做，首次构建尤其重要）

### 5.1 检查安装包内资源完整（关键）

```bash
# 免安装版 exe 与资源同目录结构（NSIS 安装后也是这个布局）
ls "src-tauri/target/release/dsh-desktop.exe"
ls "src-tauri/target/release/node/node.exe"          # 应存在（122M 左右）
ls "src-tauri/target/release/dsh/lib/bin.js"          # 应存在（ESM 入口）
ls "src-tauri/target/release/dsh/config/agent-presets/"  # 应含 standard/code/minimal/cordis
ls "src-tauri/target/release/profile/"                # 应含 package.json + node_modules
```

> 若 `dsh/lib` 缺失或 `agent-presets` 缺失，说明 tauri 打包时 resources 没复制全 —— 检查 `tauri.conf.json` 的 `bundle.resources` 映射和 `prepare-bundle.sh` 的复制逻辑。

### 5.2 运行 sidecar 冒烟（不启动 GUI）

```bash
# 用打包的 node 跑打包的 dsh，验证资源链路
DSH_HOME=$(mktemp -d)/dsh-win-smoke
mkdir -p "$DSH_HOME/profiles"
cp -r "src-tauri/target/release/profile/." "$DSH_HOME/profiles/web/"

"src-tauri/target/release/node/node.exe" \
  "src-tauri/target/release/dsh/lib/bin.js" \
  --profile web --port 3180 --no-open &
sleep 15

# 验证 Web UI 存活
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3180   # 期望 200

# 验证会话创建（agent preset 解析）
curl -s -X POST http://127.0.0.1:3180/api/session.create \
  -H "Content-Type: application/json" \
  -d '{"type":"client-request","rpcId":"t2","method":"session.create","payload":{"cwd":"C:\\"}}'
# 期望返回含 "ok":true 和 "agentPreset":"standard"

# 清理
taskkill //F //PID <node 的 pid> 2>/dev/null || true
rm -rf "$DSH_HOME"
```

### 5.3 运行 GUI 验证

1. 双击 `dsh-desktop.exe`（免安装）或安装 NSIS 包后启动
2. 首次启动：**不弹目录选择**（家目录兜底，与 macOS 一致），主窗口应加载 DSH Web UI
3. 关窗 → 应最小化到托盘；托盘菜单：显示主窗口 / 打开设置 / 退出
4. 设置页：改端口 → 重启服务 → 主窗口跟随新端口
5. 退出应用 → 任务管理器确认 node.exe 进程树被清理（`kill_tree` 走 `taskkill /T /F`）
6. 单实例：再次双击应聚焦已有窗口

---

## 6. 已知 Windows 专项风险与对策

| # | 风险 | 对策 |
|---|---|---|
| 1 | **Git Bash 的 bash 兼容性**：脚本用 `uname -s` 检测 MINGW/MSYS/CYGWIN，Git Bash 下正常；但 PowerShell 直接跑 `.sh` 不行 | 始终在 **Git Bash** 里执行构建 |
| 2 | **`/tmp` 路径**：Git Bash 的 `/tmp` 映射到用户临时目录，macOS 用的 `tar -xzf` 分支在 Windows 不会走到（Windows 分支直接下载 node.exe 单文件，不涉及 tar）——已规避 | 无需处理 |
| 3 | **`chmod`/符号链接**：Windows 分支不执行 `chmod`；`bundle/dsh/lib` 用真实目录复制而非 symlink（任务 11 修复，tauri 打包不跟随 symlink） | 已规避；若 `dsh/lib` 缺失按 5.1 排查 |
| 4 | **node-pty/koffi prebuilds**：脚本校验 `node-pty/prebuilds` 和 `@koromix/koffi-*`；Windows 上 node-pty 的 prebuilds 应含 win32-x64 | 若 WARN，检查 `bundle/dsh/node_modules/node-pty/prebuilds/`，必要时 `npm rebuild node-pty` |
| 5 | **防病毒/Defender**：NSIS 安装包和免安装 exe 可能被 SmartScreen 拦截（未签名） | 内部分发：同事点"更多信息 → 仍要运行"；后续可做代码签名 |
| 6 | **WebView2 缺失**（老 Win10） | 从 Microsoft 装 Evergreen WebView2 或让安装包带引导 |
| 7 | **路径含中文/空格**：`DSH 桌面版` 的 productName 含中文，NSIS 安装路径默认 `%LocalAppData%\DSH 桌面版` | 已知可工作（tauri 处理 Unicode）；若安装失败，检查安装日志 |
| 8 | **npm ci 的 lockfile 指向 npmmirror**：`bundle/dsh-lock/package-lock.json` 的 resolved URL 是 npmmirror | 脚本默认 REGISTRY 就是 npmmirror，一致；若换官方源，需重新生成 lockfile（见计划文档） |
| 9 | **GitHub 依赖（dsh-plugin-market）**：profile 的 `npm install` 拉 github:chnjames/dsh-plugin-market，需要 git + 能访问 github | 失败会 WARN 降级（插件市场不可用，核心功能不受影响）；重试或配置 git 代理 |

---

## 7. 首次构建验证清单（打勾用）

- [ ] Git for Windows 已装，Git Bash 可用
- [ ] `rustc --version` ≥ 1.88（推荐 1.98）
- [ ] `node --version` = v24.x（与 macOS 构建机一致）
- [ ] VS Build Tools "Desktop development with C++" 已装
- [ ] `cargo tauri --version` 输出 2.x
- [ ] `./scripts/prepare-bundle.sh` 尾部无 WARN（除 profile 网络 WARN 可接受）
- [ ] `cargo tauri build` 成功
- [ ] NSIS `..._x64-setup.exe` 生成
- [ ] 免安装 exe 的 `node/node.exe`、`dsh/lib/bin.js`、`dsh/config/agent-presets/`、`profile/` 齐全
- [ ] sidecar 冒烟：HTTP 200 + `session.create` 返回 ok:true
- [ ] GUI：启动 → 托盘 → 设置页 → 重启服务 → 退出进程树清理
- [ ] 安装 NSIS 包后在另一台干净 Windows 上重复 GUI 验证（可选但推荐）

---

## 8. 构建完成后

1. 上传产物到 GitHub Release（参考 macOS 发布流程）：
   ```bash
   gh release upload v0.1.0 "src-tauri/target/release/bundle/nsis/DSH 桌面版_0.1.0_x64-setup.exe" --repo wjames2000/dsh-desktop --clobber
   ```
2. 在 README 或发布说明中补 Windows 构建验证结果
3. 建议在计划文档 `docs/superpowers/plans/2026-08-24-dsh-desktop.md` 末尾的"实现核对记录"补充 Windows 实机验证结论
