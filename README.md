# DSH 桌面版（dsh-desktop）

将 DeepSeek Harness 的 Web GUI（`dsh --profile web`）封装为原生桌面应用。内置 Node 运行时与 dsh 包，同事零环境安装即可使用。

## 功能

- 托盘常驻：关闭主窗口最小化到托盘，会话保持运行；托盘菜单可显示主窗口 / 打开设置 / 退出
- 单实例：重复启动聚焦已有窗口
- 开机自启：设置页开关控制
- 应用设置页：工作目录、数据目录（DSH_HOME）、服务端口、开机自启、关闭行为、更新检查
- 自动更新：Tauri updater（需配置内网更新服务器后启用）
- 内置 profile：开箱即用，含插件市场（dsh-plugin-market）

## 构建

前置条件（构建机）：
1. Rust 工具链（rustup/cargo）
2. Tauri CLI v2：`cargo install tauri-cli --version "^2" --locked`
3. Node.js（与内置 Node 版本一致；`node --version` 查看）
4. macOS 构建需 Xcode Command Line Tools

构建（在目标平台上执行，native 模块按平台编译）：

```bash
./scripts/build-all.sh
```

产物：
- macOS：`src-tauri/target/release/bundle/macos/DSH 桌面版.app`（dmg 需在有 Finder 的环境打包）
- Windows：`src-tauri/target/release/bundle/nsis/*.exe` 或 `msi/*.msi`
- Linux：`src-tauri/target/release/bundle/deb/*.deb` 或 `appimage/*.AppImage`

**内置 Node 版本必须与构建机 Node 版本一致**（dsh 的 native 模块按 ABI 编译）。`NODE_VERSION` 环境变量可覆盖。

## 分发与自动更新

- 三平台安装包由对应平台构建生成（`./scripts/build-all.sh`）
- 自动更新链路（发布流程）：
  1. 生成签名密钥对：`cargo tauri signer generate -w ~/.tauri/dsh-desktop.key`（私钥**勿提交 git**，公钥填入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`）
  2. 把 `src-tauri/tauri.conf.json` 的 `plugins.updater.endpoints` 替换为内网更新服务器地址
  3. 构建后把新版本安装包签名（`cargo tauri signer sign -f ~/.tauri/dsh-desktop.key <installer>`，生成密钥时若设了密码加 `-p <密码>`）并连同 `latest.json` 上传更新服务器
- 第一版不做 macOS 公证 / Windows 代码签名：同事首次运行时 macOS 右键"打开"、Windows 提示"仍要运行"即可

## 应用数据

- 默认使用 `~/.dsh`（DSH_HOME），与浏览器里使用的 DSH 数据互通
- 可在设置页更改数据目录（下次启动生效）
- 工作目录未配置时默认用家目录兜底，可在设置页更改（下次启动生效）

## 内置 dsh 版本与认证

- 内置 `@deepseek-ai/dsh@0.1.5-rc.1`（npm lockfile 固化于 `bundle/dsh-lock/`）
- dsh ≥ 0.1.2 引入进程级 token 认证：启动后服务 URL 携带 `?token=`，无 token 的请求返回 401。桌面应用启动时从 dsh 日志解析认证 URL 并导航主窗口，WebView 自动完成 cookie 交换（`303 → Set-Cookie → 干净首页 200`）；重启服务后自动导航到新 token 的 URL
- agent-presets 已随 0.1.2 移入独立包 `@deepseek-ai/dsh-agent-presets`（`prepare-bundle.sh` 无需再复制 `config/`）
- 插件市场（dsh-plugin-market）以 **vendored tgz** 分发（`bundle/profile-template/vendor/`），profile 依赖声明为 `file:./vendor/*.tgz`——构建不再依赖 GitHub 可达性
- 升级内置 dsh：修改 `bundle/dsh-lock/package.json` 的依赖版本 → 重新生成 lockfile（`npm install --package-lock-only`）→ 同步 `scripts/prepare-bundle.sh` 的 `DSH_VERSION` → 重跑 `./scripts/build-all.sh`
- **构建一致性约束**：构建机 node 版本必须等于内置 node 版本（`prepare-bundle.sh` 会 fail-fast 校验）。npm ci 按当前 node 的 ABI 选择 native prebuilds（node-pty 等），不一致会导致 sidecar 启动失败

## 窗口与权限（ACL）

- `main` 窗口加载本地设置页（sidecar 启动中/失败时）→ 由 `default` capability 授权（`local: true`，含全部应用命令），因此启动失败时可在该窗口直接点"重启服务"自救
- `main` 窗口导航到 `http://127.0.0.1:*` 后的远程页面 → 由 `remote` capability 接管（只读，仅 `get_config`）
- `settings` 窗口（托盘打开）→ 与 `main` 的本地阶段共用 `default` capability
- 改动 `capabilities/*.json` 后需重新构建（capabilities 编译进二进制）

## 图标

当前使用占位图标（深蓝 #0d5ed9）。替换正式品牌图标：把设计稿（1024x1024 PNG）放为 `assets/icon-source.png`，然后在仓库根目录运行 `cargo tauri icon assets/icon-source.png` 重新生成全套（输出默认写到 `src-tauri/icons/`）。

## 已知限制

- Windows/Linux 需在对应系统上构建（native 模块按平台编译）
- 数据目录与工作目录更改后需重启应用生效
- dmg 打包需要图形环境（headless CI 下会失败，.app 仍可用）
- 更新源未配置时（endpoints 为空），"检查更新"会提示"检查失败: Updater does not have any endpoints set."（无网络请求）
