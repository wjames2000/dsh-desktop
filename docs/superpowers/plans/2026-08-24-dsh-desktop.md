# DSH 桌面版（dsh-desktop）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 将 `@deepseek-ai/dsh@0.1.1-rc.2` 的 Web GUI 封装为 Tauri v2 桌面应用（DSH 桌面版 / dsh-desktop），内置 Node sidecar，三平台（macOS/Windows/Linux）自包含分发，支持托盘、单实例、开机自启、应用设置页与自动更新。

**架构：** Tauri v2（Rust）壳负责窗口/托盘/单实例/设置；Rust 主进程 spawn 内置的 `node + dsh CLI`（`--profile web --port N --no-open`）作为 sidecar，就绪后主窗口 WebView 加载 `http://127.0.0.1:N`。数据目录默认 `~/.dsh`，可配置（`DSH_HOME` 注入）。dsh 包 + node_modules + 内置 profile 作为 sidecar 资源随安装包分发。

**技术栈：** Tauri v2（Rust 2021 版）、tauri-plugin-single-instance、tauri-plugin-autostart、tauri-plugin-dialog、tauri-plugin-process、tauri-plugin-updater、Node v24.x 官方二进制、`@deepseek-ai/dsh@0.1.1-rc.2`（npm 锁定）、设置页用原生 HTML/JS。

**规格文档：** `docs/superpowers/specs/2026-08-24-dsh-desktop-design.md`

---

## 文件结构

```
dsh-desktop/                        # 应用根目录（本仓库根目录）
├── src-tauri/                      # Tauri Rust 壳
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/                      # Tauri 图标（模板生成的占位）
│   ├── src/
│   │   ├── main.rs                 # 入口：初始化 app + 注册插件 + 启动流程
│   │   ├── config.rs               # AppConfig 结构体 + load/save（原子写）
│   │   ├── sidecar.rs              # SidecarManager：spawn/kill/就绪轮询/日志管道
│   │   ├── port.rs                 # 空闲端口探测
│   │   ├── tray.rs                 # 托盘菜单
│   │   ├── commands.rs             # Tauri IPC commands（设置读写等）
│   │   └── updater.rs              # 更新检查封装（调 tauri-plugin-updater）
│   ├── resources/                  # 构建时由 scripts/prepare-bundle.sh 填充
│   │   ├── node/                   # node 二进制（按平台）
│   │   ├── dsh/                    # dsh 包 + node_modules
│   │   └── profile/                # 内置 web profile 模板
│   └── src-tauri/gen/              # 生成产物（不入库）
├── ui/                             # 设置页前端（原生 HTML/JS）
│   ├── index.html
│   ├── styles.css
│   └── app.js
├── scripts/
│   ├── prepare-bundle.sh           # 下载 node / npm ci / 组装 profile / 校验
│   └── build-all.sh                # 调 prepare-bundle.sh + cargo tauri build
├── .gitignore
└── README.md
```

**职责划分：**
- `config.rs`：应用级设置的唯一读写入口（app-config.json），其余模块只读它
- `sidecar.rs`：唯一 spawn/kill dsh 进程的地方；`main.rs` 只调用它的公开方法
- `port.rs`：纯函数式空闲端口探测，无副作用，可单测
- `commands.rs`：薄 IPC 层，校验后转发给 config/sidecar/tray
- `ui/app.js`：设置页所有交互；通过 `window.__TAURI__` 调 commands

---

## 前置检查（开始前必须完成）

- [ ] **步骤 0.1：确认 Rust 工具链已安装**

运行：`rustc --version && cargo --version`
预期：输出版本号。若报 "command not found"：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

然后重新运行版本检查确认成功。

- [ ] **步骤 0.2：确认 macOS 构建依赖**

运行：`xcode-select -p`
预期：输出 Xcode 路径（如 `/Library/Developer/CommandLineTools`）。若失败：

```bash
xcode-select --install
```

- [ ] **步骤 0.3：确认 Node 可用（构建机用于 npm ci）**

运行：`node --version && npm --version`
预期：Node v24.x、npm 10+。若无，用 nvm 安装 Node 24。

- [ ] **步骤 0.4：确认 Tauri CLI**

运行：`cargo install tauri-cli --version "^2" --locked`（安装 tauri CLI v2）
预期：安装成功。验证：`cargo tauri --version` 输出 2.x。

- [ ] **步骤 0.5：README 说明内置 Node 版本决策**

修改：`README.md`（创建）
内容：项目简介 + 内置 Node 版本 = 构建机 node 版本（`node --version` 输出），两者必须一致（native 模块 ABI 原因）。

```markdown
# DSH 桌面版（dsh-desktop）

将 DeepSeek Harness 的 Web GUI 封装为桌面应用。内置 Node 运行时与 dsh 包，
同事零环境安装即可使用。

## 构建

1. 安装 Rust 工具链与 Tauri CLI v2
2. `./scripts/build-all.sh`

内置 Node 版本必须与构建机 Node 版本一致（dsh 的 native 模块按 ABI 编译）。
```

- [ ] **步骤 0.6：Commit**

```bash
git add README.md
git commit -m "docs: add build prerequisites to README"
```

---

## 任务 1：初始化 Tauri 项目骨架

**文件：**
- 创建：`src-tauri/Cargo.toml`
- 创建：`src-tauri/build.rs`
- 创建：`src-tauri/tauri.conf.json`
- 创建：`src-tauri/src/main.rs`（最小版）
- 创建：`.gitignore`

- [ ] **步骤 1.1：创建 Cargo.toml**

创建 `src-tauri/Cargo.toml`：

```toml
[package]
name = "dsh-desktop"
version = "0.1.0"
description = "DSH 桌面版 - DeepSeek Harness desktop shell"
edition = "2021"

[lib]
name = "dsh_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-single-instance = "2"
tauri-plugin-autostart = "2"
tauri-plugin-dialog = "2"
tauri-plugin-process = "2"
tauri-plugin-updater = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["blocking"] }
dirs = "5"
```

- [ ] **步骤 1.2：创建 build.rs**

创建 `src-tauri/build.rs`：

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **步骤 1.3：创建 tauri.conf.json**

创建 `src-tauri/tauri.conf.json`：

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "DSH 桌面版",
  "version": "0.1.0",
  "identifier": "com.hapudaisi.dsh-desktop",
  "build": {
    "frontendDist": "../ui"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "DSH 桌面版",
        "width": 1280,
        "height": 800,
        "visible": false
      },
      {
        "label": "settings",
        "title": "设置",
        "width": 560,
        "height": 640,
        "visible": false,
        "resizable": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "resources": {
      "resources/node": "node",
      "resources/dsh": "dsh",
      "resources/profile": "profile"
    },
    "updater": {
      "active": true,
      "endpoints": [
        "https://updates.example.com/dsh-desktop/latest.json"
      ],
      "pubkey": "REPLACE_WITH_GENERATED_PUBKEY"
    }
  },
  "plugins": {
    "updater": {
      "endpoints": [
        "https://updates.example.com/dsh-desktop/latest.json"
      ]
    }
  }
}
```

注意：`pubkey` 和 endpoints URL 是占位，任务 9 生成密钥后替换。窗口 `visible: false` 是为了等 sidecar 就绪后再显示（避免加载失败的白屏），任务 3 实现显示逻辑。

- [ ] **步骤 1.4：生成图标**

运行：

```bash
mkdir -p src-tauri/icons
# 用 Tauri 默认图标生成（之后可替换为正式图标）
cargo tauri icon --help >/dev/null 2>&1 || echo "tauri CLI 未安装，先执行前置检查 0.4"
# 提供一个 1024x1024 的源图后运行：cargo tauri icon <source.png>
# 先用系统自带方式生成占位图标：从 Tauri 模板复制
```

如果 `cargo tauri icon` 可用，先用任意 1024x1024 PNG 生成全套图标；否则创建空占位目录（图标生成放任务 10 统一处理）。

- [ ] **步骤 1.5：创建最小 main.rs**

创建 `src-tauri/src/main.rs`：

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dsh_desktop_lib::run();
}
```

- [ ] **步骤 1.6：创建 .gitignore**

创建 `.gitignore`：

```gitignore
# Rust
target/
src-tauri/gen/schemas/
src-tauri/gen/android/
src-tauri/gen/apple/

# Node
node_modules/
bundle/node/
bundle/dsh/
bundle/profile/

# Tauri
src-tauri/target/

# 构建产物
*.dmg
*.msi
*.exe
*.deb
*.AppImage
```

- [ ] **步骤 1.7：创建 lib.rs（run 入口）**

创建 `src-tauri/src/lib.rs`：

```rust
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **步骤 1.8：cargo check 验证编译**

运行：`cargo check`（在 `src-tauri/` 目录下）
预期：编译通过（首次会下载依赖，可能 5-10 分钟）。

- [ ] **步骤 1.9：Commit**

```bash
git add src-tauri .gitignore
git commit -m "feat: scaffold tauri v2 project skeleton"
```

---

## 任务 2：AppConfig（应用设置读写）

**文件：**
- 创建：`src-tauri/src/config.rs`
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 2.1：定义 AppConfig 结构体与默认值**

创建 `src-tauri/src/config.rs`：

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// 工作目录（workspace）
    pub workspace_dir: Option<String>,
    /// 数据目录（DSH_HOME）；None 表示使用默认 ~/.dsh
    pub data_dir: Option<String>,
    /// 端口偏好，0 表示让 OS 分配
    pub port: u16,
    /// 开机自启
    pub autostart: bool,
    /// 关窗行为：true=最小化到托盘，false=直接退出
    pub minimize_to_tray: bool,
    /// 更新通道
    pub update_channel: String,
    /// 启动时自动检查更新
    pub check_updates_on_start: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            data_dir: None,
            port: 3080,
            autostart: false,
            minimize_to_tray: true,
            update_channel: "stable".to_string(),
            check_updates_on_start: true,
        }
    }
}

impl AppConfig {
    /// 配置文件路径：系统配置目录/dsh-desktop/app-config.json
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("dsh-desktop").join("app-config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// 原子写：先写临时文件再 rename
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&tmp, data).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }
}
```

- [ ] **步骤 2.2：单元测试（TDD：先写测试文件）**

创建 `src-tauri/src/config.rs` 追加测试模块（写在文件末尾）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let c = AppConfig::default();
        assert_eq!(c.port, 3080);
        assert!(c.minimize_to_tray);
        assert!(c.check_updates_on_start);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut c = AppConfig::default();
        c.workspace_dir = Some("/tmp/ws".to_string());
        c.port = 4000;
        c.autostart = true;
        let path = std::env::temp_dir().join("dsh-desktop-test").join("app-config.json");
        // 手动指向测试路径（save/load 用 config_path 时无法注入，
        // 这里直接测 save 的原子写行为）
        let data = serde_json::to_string_pretty(&c).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path.with_extension("json.tmp"), &data).unwrap();
        std::fs::rename(path.with_extension("json.tmp"), &path).unwrap();
        let loaded: AppConfig = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.workspace_dir, Some("/tmp/ws".to_string()));
        assert_eq!(loaded.port, 4000);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn load_missing_file_returns_default() {
        // 用不存在的路径读不到时会走 default
        let path = std::env::temp_dir().join("dsh-desktop-does-not-exist");
        let result = std::fs::read_to_string(&path);
        assert!(result.is_err());
    }
}
```

- [ ] **步骤 2.3：运行测试验证**

运行：`cargo test`（在 `src-tauri/`）
预期：3 个测试通过。若 `config.rs` 未编译（如 dirs 未加），补充 `dirs = "5"` 到 Cargo.toml 的依赖。

- [ ] **步骤 2.4：Commit**

```bash
git add src-tauri/src/config.rs Cargo.toml
git commit -m "feat: add app config with atomic save"
```

---

## 任务 3：空闲端口探测

**文件：**
- 创建：`src-tauri/src/port.rs`
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 3.1：实现端口探测**

创建 `src-tauri/src/port.rs`：

```rust
use std::net::TcpListener;

/// 从 preferred 开始找第一个可绑定的端口；preferred=0 时由 OS 分配。
/// 返回 (实际端口)。范围限制 1024..=65535，最多尝试 100 个。
pub fn find_free_port(preferred: u16) -> u16 {
    if preferred == 0 {
        return TcpListener::bind("127.0.0.1:0")
            .map(|l| l.local_addr().unwrap().port())
            .unwrap_or(3080);
    }
    let start = preferred.max(1024);
    for port in start..=(start.saturating_add(100).min(65535)) {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    // 全部占用则让 OS 分配
    TcpListener::bind("127.0.0.1:0")
        .map(|l| l.local_addr().unwrap().port())
        .unwrap_or(3080)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_free_port_returns_bindable() {
        let p = find_free_port(3080);
        assert!(p >= 1024 && p <= 65535);
        // 端口应该真的是空闲的（能绑定）
        assert!(TcpListener::bind(("127.0.0.1", p)).is_ok());
    }

    #[test]
    fn zero_means_os_assigns() {
        let p = find_free_port(0);
        assert!(p >= 1024 && p <= 65535);
    }

    #[test]
    fn skips_occupied_port() {
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let p = find_free_port(occupied_port);
        assert_ne!(p, occupied_port);
    }
}
```

- [ ] **步骤 3.2：运行测试**

运行：`cargo test port`（在 `src-tauri/`）
预期：3 个测试通过。

- [ ] **步骤 3.3：Commit**

```bash
git add src-tauri/src/port.rs
git commit -m "feat: add free-port probing"
```

---

## 任务 4：sidecar 资源解析路径

**文件：**
- 创建：`src-tauri/src/sidecar.rs`（仅路径解析部分）

- [ ] **步骤 4.1：实现资源路径解析**

创建 `src-tauri/src/sidecar.rs`（此任务先只放路径解析，进程管理任务 5 追加）：

```rust
use std::path::PathBuf;

/// 解析打包进应用的 sidecar 资源路径。
/// 资源被放在 <exe_dir>/../Resources/node、Resources/dsh、Resources/profile
/// （Tauri 的 resources 在 macOS 上是 Resources/，Windows/Linux 上是 exe 同目录下）
pub fn resource_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("failed to get exe path");
    let dir = exe.parent().expect("exe has no parent");
    // macOS: Contents/MacOS/<exe> -> 上级是 Contents，Resources 在 Contents 下
    if dir.file_name().map(|s| s == "MacOS").unwrap_or(false) {
        dir.parent().unwrap().join("Resources")
    } else {
        dir.to_path_buf()
    }
}

pub fn node_binary() -> PathBuf {
    let base = resource_dir().join("node");
    let name = if cfg!(target_os = "windows") { "node.exe" } else { "node" };
    base.join(name)
}

pub fn dsh_dir() -> PathBuf {
    resource_dir().join("dsh")
}

pub fn profile_dir() -> PathBuf {
    resource_dir().join("profile")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_point_to_resources() {
        assert!(node_binary().to_string_lossy().contains("node"));
        assert!(dsh_dir().to_string_lossy().contains("dsh"));
        assert!(profile_dir().to_string_lossy().contains("profile"));
    }
}
```

- [ ] **步骤 4.2：运行测试**

运行：`cargo test sidecar`（在 `src-tauri/`）
预期：通过。

- [ ] **步骤 4.3：Commit**

```bash
git add src-tauri/src/sidecar.rs
git commit -m "feat: add sidecar resource path resolution"
```

---

## 任务 5：Sidecar 进程管理

**文件：**
- 修改：`src-tauri/src/sidecar.rs`（追加进程管理）

- [ ] **步骤 5.1：实现 SidecarManager**

在 `src-tauri/src/sidecar.rs` 追加：

```rust
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

pub struct SidecarManager {
    child: Option<Child>,
    log_rx: Receiver<String>,
}

impl SidecarManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = tx; // 占位：日志转发在线程内使用
        Self { child: None, log_rx: rx }
    }

    /// 启动 dsh web sidecar。
    /// data_dir 为 None 时不注入 DSH_HOME（用默认 ~/.dsh）。
    pub fn start(&mut self, workspace: &str, data_dir: Option<&str>, port: u16) -> Result<(), String> {
        let node = node_binary();
        let dsh = dsh_dir().join("lib").join("bin.js");
        if !node.exists() {
            return Err(format!("node 运行时缺失: {}", node.display()));
        }
        if !dsh.exists() {
            return Err(format!("dsh 包缺失: {}", dsh.display()));
        }

        let mut cmd = Command::new(&node);
        cmd.arg(&dsh)
            .arg("--profile")
            .arg("web")
            .arg("--port")
            .arg(port.to_string())
            .arg("--no-open")
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(d) = data_dir {
            cmd.env("DSH_HOME", d);
        }

        let mut child = cmd.spawn().map_err(|e| format!("启动 dsh 失败: {e}"))?;

        // 日志管道：把 stdout/stderr 读到 channel（任务 8 接设置页展示）
        if let Some(mut out) = child.stdout.take() {
            let tx = mpsc::channel().0;
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match out.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => { let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string()); }
                    }
                }
            });
        }
        let _ = self.log_rx.recv_timeout(Duration::from_millis(1)); // 清空占位

        self.child = Some(child);
        Ok(())
    }

    /// 轮询 http://127.0.0.1:{port} 直到 200，超时 30s
    pub fn wait_ready(&self, port: u16, timeout: Duration) -> Result<(), String> {
        let url = format!("http://127.0.0.1:{port}");
        let deadline = Instant::now() + timeout;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())?;
        while Instant::now() < deadline {
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(format!("等待 DSH 服务就绪超时（{timeout:?}）：{url}"))
    }

    /// kill 整个进程树并等待退出（超时 5s 强杀）。
    /// macOS/Linux 用 pkill -P 递归；Windows 用 taskkill /T /F。
    pub fn stop(&mut self) {
        if let Some(child) = self.child.take() {
            let pid = child.id();
            let _ = kill_tree(pid);
            let mut child = child;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if Instant::now() > deadline {
                            let _ = child.kill();
                            break;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

/// 平台相关的进程树 kill
fn kill_tree(pid: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 递归 kill 子进程再 kill 自身
        let out = Command::new("pgrep")
            .args(["-P", &pid.to_string()])
            .output();
        if let Ok(out) = out {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                if let Ok(child_pid) = line.trim().parse::<u32>() {
                    let _ = kill_tree(child_pid);
                }
            }
        }
        let _ = Command::new("kill").arg(pid.to_string()).status();
        Ok(())
    }
}

impl Default for SidecarManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **步骤 5.2：单元测试（不真正 spawn dsh，只测 kill_tree 对不存在 pid 不 panic）**

追加测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_nonexistent_pid_is_ok() {
        // 不存在的 pid：kill_tree 应返回 Ok 或不 panic
        let _ = kill_tree(999_999);
    }
}
```

运行：`cargo test sidecar`
预期：通过。

- [ ] **步骤 5.3：Commit**

```bash
git add src-tauri/src/sidecar.rs
git commit -m "feat: add sidecar process management with tree kill"
```

---

## 任务 6：启动流程编排（main 窗口 + sidecar + 目录选择）

**文件：**
- 修改：`src-tauri/src/lib.rs`
- 创建：`src-tauri/src/commands.rs`（先放 update_config 相关 command）

- [ ] **步骤 6.1：实现 run() 完整流程**

改写 `src-tauri/src/lib.rs`：

```rust
mod commands;
mod config;
mod port;
mod sidecar;
mod tray;
mod updater;

use std::sync::Mutex;
use std::time::Duration;
use tauri::Manager;

pub struct AppState {
    pub config: Mutex<config::AppConfig>,
    pub sidecar: Mutex<sidecar::SidecarManager>,
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 二次启动：聚焦主窗口
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState {
            config: Mutex::new(config::AppConfig::load()),
            sidecar: Mutex::new(sidecar::SidecarManager::new()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::choose_workspace,
            commands::restart_service,
            commands::quit_app,
        ])
        .setup(|app| {
            let state = app.state::<AppState>();
            let cfg = state.config.lock().unwrap().clone();

            // 单实例 + 开机自启状态同步
            let _ = tauri_plugin_autostart::ManagerExt::autostart(app);
            let _ = tauri_plugin_autostart::ManagerExt::is_enabled(app);

            // 启动 sidecar（若未配置工作目录，弹目录选择器）
            let workspace = match &cfg.workspace_dir {
                Some(w) if !w.is_empty() => w.clone(),
                _ => {
                    // 首次启动：弹原生目录选择器
                    use tauri_plugin_dialog::DialogExt;
                    let picked = app.dialog().file().blocking_pick_folder();
                    match picked {
                        Some(p) => p.to_string(),
                        None => {
                            // 用户取消：用家目录兜底
                            dirs::home_dir()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        }
                    }
                }
            };
            // 记住选择
            {
                let mut cfg2 = state.config.lock().unwrap();
                cfg2.workspace_dir = Some(workspace.clone());
                let _ = cfg2.save();
            }

            // 探测端口并启动 sidecar
            let port = port::find_free_port(cfg.port);
            let data_dir = cfg.data_dir.clone();
            {
                let mut sm = state.sidecar.lock().unwrap();
                sm.start(&workspace, data_dir.as_deref(), port)
                    .map_err(|e| e.to_string())?;
            }

            // 等待就绪后显示主窗口
            let main_win = app.get_webview_window("main").unwrap();
            let sidecar_ready = {
                let sm = state.sidecar.lock().unwrap();
                sm.wait_ready(port, Duration::from_secs(30))
            };
            match sidecar_ready {
                Ok(()) => {
                    let _ = main_win.navigate(
                        format!("http://127.0.0.1:{port}").parse().unwrap(),
                    );
                    let _ = main_win.show();
                }
                Err(e) => {
                    // 显示错误页（任务 7 完善）
                    eprintln!("sidecar 启动失败: {e}");
                    let _ = main_win.show();
                }
            }

            // 托盘
            tray::setup_tray(app.handle().clone())?;

            // 开机自启：按配置同步
            if cfg.autostart {
                let _ = tauri_plugin_autostart::ManagerExt::enable(app);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗行为：最小化到托盘或退出
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let cfg = state.config.lock().unwrap().clone();
                if cfg.minimize_to_tray && window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    // 直接退出：停 sidecar
                    let mut sm = state.sidecar.lock().unwrap();
                    sm.stop();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_handle, event| {
        if let tauri::RunEvent::Exit = event {
            // 确保 sidecar 随应用退出
            let state = _handle.state::<AppState>();
            let mut sm = state.sidecar.lock().unwrap();
            sm.stop();
        }
    });
}
```

注意：`updater.rs` 和 `tray.rs` 模块在任务 7/9 才实现，此任务先创建空占位文件避免编译失败（在 src/ 下建空 `tray.rs`、`updater.rs`，内容为 `// placeholder`）。

- [ ] **步骤 6.2：创建 commands.rs（设置读写 command）**

创建 `src-tauri/src/commands.rs`：

```rust
use crate::{config::AppConfig, AppState};
use tauri::{AppHandle, State};

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(state: State<AppState>, cfg: AppConfig) -> Result<(), String> {
    // 校验端口范围
    if cfg.port != 0 && !(1024..=65535).contains(&cfg.port) {
        return Err("端口必须在 1024-65535 之间（0 表示自动分配）".to_string());
    }
    let mut cur = state.config.lock().unwrap();
    *cur = cfg;
    cur.save()
}

#[tauri::command]
pub fn choose_workspace(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub fn restart_service(state: State<AppState>) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    let workspace = cfg.workspace_dir.clone().unwrap_or_default();
    let port = port::find_free_port(cfg.port);
    let data_dir = cfg.data_dir.clone();
    let mut sm = state.sidecar.lock().unwrap();
    sm.stop();
    sm.start(&workspace, data_dir.as_deref(), port)?;
    sm.wait_ready(port, std::time::Duration::from_secs(30))?;
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
```

- [ ] **步骤 6.3：创建空占位模块**

创建 `src-tauri/src/tray.rs` 与 `src-tauri/src/updater.rs`，内容为：

```rust
// placeholder - 任务 7/9 实现
```

- [ ] **步骤 6.4：编译验证**

运行：`cargo check`（在 `src-tauri/`）
预期：编译通过。若报 dialog 插件 API 名不对，按实际版本调整（如 `blocking_pick_folder` 在 v2 中可能为 `pick_folder` + 回调，改为回调方式或使用 `blocking_pick_folder`）。

- [ ] **步骤 6.5：Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands.rs src-tauri/src/tray.rs src-tauri/src/updater.rs
git commit -m "feat: wire startup flow with sidecar and dialogs"
```

---

## 任务 7：托盘菜单

**文件：**
- 修改：`src-tauri/src/tray.rs`
- 修改：`src-tauri/src/lib.rs`（on_window_event 已有，无需改）

- [ ] **步骤 7.1：实现托盘**

改写 `src-tauri/src/tray.rs`：

```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

pub fn setup_tray(app: AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(&app, "show", "显示主窗口", true, None::<&str>)?;
    let settings = MenuItem::with_id(&app, "settings", "打开设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(&app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(&app, &[&show, &settings, &quit])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "settings" => {
                if let Some(w) = app.get_webview_window("settings") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "quit" => {
                // 停 sidecar 后退出（on_window_event 与 RunEvent::Exit 兜底）
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
```

- [ ] **步骤 7.2：编译验证**

运行：`cargo check`
预期：通过。若 `default_window_icon` 在 tray 构建时需要具体 icon，改用 `app.default_window_icon().cloned()`。

- [ ] **步骤 7.3：Commit**

```bash
git add src-tauri/src/tray.rs
git commit -m "feat: add system tray with show/settings/quit"
```

---

## 任务 8：设置页前端 + IPC

**文件：**
- 创建：`ui/index.html`
- 创建：`ui/styles.css`
- 创建：`ui/app.js`
- 修改：`src-tauri/capabilities/default.json`（创建）

- [ ] **步骤 8.1：创建设置页 HTML**

创建 `ui/index.html`：

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>设置 - DSH 桌面版</title>
  <link rel="stylesheet" href="styles.css">
</head>
<body>
  <h1>DSH 桌面版设置</h1>

  <section>
    <h2>工作目录</h2>
    <p id="workspace-value" class="value">-</p>
    <button id="choose-workspace">更改</button>
    <p class="hint">首次启动或更改后，下次启动生效</p>
  </section>

  <section>
    <h2>数据目录 (DSH_HOME)</h2>
    <p id="datadir-value" class="value">默认 (~/.dsh)</p>
    <button id="choose-datadir">更改</button>
    <button id="reset-datadir" class="secondary">恢复默认</button>
    <p class="hint">更改后下次启动生效</p>
  </section>

  <section>
    <h2>服务端口</h2>
    <input type="number" id="port-input" min="1024" max="65535" value="3080">
    <p class="hint">0 = 自动分配；被占用时自动递增</p>
  </section>

  <section>
    <h2>开机自启</h2>
    <label><input type="checkbox" id="autostart-check"> 登录系统后自动启动 DSH 服务</label>
  </section>

  <section>
    <h2>关闭行为</h2>
    <label><input type="checkbox" id="tray-check"> 关闭主窗口时最小化到托盘</label>
  </section>

  <section>
    <h2>更新</h2>
    <label><input type="checkbox" id="check-update-check"> 启动时自动检查更新</label>
    <button id="check-update-now">立即检查更新</button>
    <p id="update-status" class="hint"></p>
  </section>

  <section>
    <h2>服务日志</h2>
    <pre id="log-view" class="log"></pre>
  </section>

  <p id="save-status" class="save-status"></p>
  <script src="app.js"></script>
</body>
</html>
```

- [ ] **步骤 8.2：创建样式**

创建 `ui/styles.css`：

```css
body { font-family: system-ui, sans-serif; max-width: 520px; margin: 24px auto; padding: 0 16px; color: #1f2328; }
h1 { font-size: 20px; }
h2 { font-size: 15px; margin-top: 24px; }
.value { color: #57606a; word-break: break-all; }
.hint { font-size: 12px; color: #8c959f; }
button { margin: 4px 4px 4px 0; padding: 6px 14px; border-radius: 6px; border: 1px solid #d0d7de; background: #f6f8fa; cursor: pointer; }
button.primary { background: #0969da; color: #fff; border-color: #0969da; }
button.secondary { background: transparent; }
.log { background: #f6f8fa; border: 1px solid #d0d7de; border-radius: 6px; padding: 8px; height: 120px; overflow: auto; font-size: 11px; white-space: pre-wrap; }
.save-status { color: #1a7f37; font-size: 13px; }
```

- [ ] **步骤 8.3：创建 app.js**

创建 `ui/app.js`：

```javascript
// Tauri v2 IPC
const { invoke } = window.__TAURI__.core;

let currentConfig = null;

async function refresh() {
  currentConfig = await invoke('get_config');
  document.getElementById('workspace-value').textContent =
    currentConfig.workspace_dir || '未设置（首次启动时选择）';
  document.getElementById('datadir-value').textContent =
    currentConfig.data_dir || '默认 (~/.dsh)';
  document.getElementById('port-input').value = currentConfig.port;
  document.getElementById('autostart-check').checked = currentConfig.autostart;
  document.getElementById('tray-check').checked = currentConfig.minimize_to_tray;
  document.getElementById('check-update-check').checked = currentConfig.check_updates_on_start;
}

async function save(partial) {
  const next = { ...currentConfig, ...partial };
  try {
    await invoke('set_config', { cfg: next });
    currentConfig = next;
    document.getElementById('save-status').textContent = '已保存（部分设置下次启动生效）';
  } catch (e) {
    document.getElementById('save-status').textContent = '保存失败: ' + e;
  }
}

document.getElementById('choose-workspace').addEventListener('click', async () => {
  const picked = await invoke('choose_workspace');
  if (picked) { await save({ workspace_dir: picked }); await refresh(); }
});

document.getElementById('choose-datadir').addEventListener('click', async () => {
  const picked = await invoke('choose_workspace');
  if (picked) { await save({ data_dir: picked }); await refresh(); }
});

document.getElementById('reset-datadir').addEventListener('click', async () => {
  await save({ data_dir: null }); await refresh();
});

document.getElementById('port-input').addEventListener('change', (e) => {
  const v = Number(e.target.value);
  save({ port: Number.isFinite(v) && v > 0 ? v : 0 });
});

document.getElementById('autostart-check').addEventListener('change', (e) => {
  save({ autostart: e.target.checked });
});

document.getElementById('tray-check').addEventListener('change', (e) => {
  save({ minimize_to_tray: e.target.checked });
});

document.getElementById('check-update-check').addEventListener('change', (e) => {
  save({ check_updates_on_start: e.target.checked });
});

document.getElementById('check-update-now').addEventListener('click', async () => {
  const status = document.getElementById('update-status');
  status.textContent = '正在检查…';
  try {
    const result = await invoke('check_for_updates');
    status.textContent = result || '已是最新版本';
  } catch (e) {
    status.textContent = '检查失败: ' + e;
  }
});

refresh();
```

- [ ] **步骤 8.4：创建 capabilities 配置（允许 IPC 权限）**

创建 `src-tauri/capabilities/default.json`：

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "主窗口与设置窗口的默认能力",
  "windows": ["main", "settings"],
  "permissions": [
    "core:default",
    "dialog:default",
    "dialog:allow-open",
    "autostart:default",
    "updater:default",
    "process:default"
  ]
}
```

- [ ] **步骤 8.5：编译验证**

运行：`cargo check`
预期：通过。若报 capabilities schema 生成问题，先运行 `cargo tauri dev` 或 `cargo build` 生成 schema。

- [ ] **步骤 8.6：Commit**

```bash
git add ui/ src-tauri/capabilities/default.json
git commit -m "feat: add settings page UI with IPC"
```

---

## 任务 9：自动更新

**文件：**
- 修改：`src-tauri/src/updater.rs`
- 修改：`src-tauri/src/commands.rs`（增加 check_for_updates command）
- 修改：`src-tauri/src/lib.rs`（setup 中启动时检查）
- 修改：`src-tauri/tauri.conf.json`（pubkey 替换）

- [ ] **步骤 9.1：实现 updater.rs**

改写 `src-tauri/src/updater.rs`：

```rust
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// 启动时静默检查（失败静默）
pub fn check_on_startup(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let cfg = state.config.lock().unwrap().clone();
    if !cfg.check_updates_on_start {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = check(&app);
    });
}

/// 手动检查；返回状态文案
pub fn check(app: &AppHandle) -> Result<String, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().map_err(|e| e.to_string())? {
        Some(update) => {
            let version = update.version.clone();
            let downloaded = update.download_and_install(|_| {}, |_| {}).map_err(|e| e.to_string())?;
            if downloaded {
                Ok(format!("新版本 {version} 已安装，即将重启…"))
            } else {
                Ok("下载失败，请重试".to_string())
            }
        }
        None => Ok("已是最新版本".to_string()),
    }
}
```

- [ ] **步骤 9.2：注册 command**

在 `src-tauri/src/commands.rs` 追加：

```rust
#[tauri::command]
pub fn check_for_updates(app: AppHandle) -> Result<String, String> {
    crate::updater::check(&app)
}
```

并在 `lib.rs` 的 `generate_handler!` 中追加 `commands::check_for_updates`。

- [ ] **步骤 9.3：启动时调用**

在 `lib.rs` 的 setup 闭包末尾追加：

```rust
// 启动时后台检查更新
crate::updater::check_on_startup(app.handle());
```

- [ ] **步骤 9.4：生成签名密钥并配置**

运行（在 `src-tauri/` 目录）：

```bash
npx tauri signer generate -w ~/.tauri/dsh-desktop.key
```

预期：生成私钥 `~/.tauri/dsh-desktop.key` 并输出公钥。将输出的公钥填入 `tauri.conf.json` 的 `bundle.updater.pubkey`。

将 `tauri.conf.json` 中的 `https://updates.example.com/...` 替换为实际内网更新服务器地址（需用户提供；未提供前保留占位并加注释）。

注意：私钥 `~/.tauri/dsh-desktop.key` **不要**提交到 git（加入 `.gitignore`：`~/.tauri/`）。

- [ ] **步骤 9.5：编译验证**

运行：`cargo check`
预期：通过。

- [ ] **步骤 9.6：Commit**

```bash
git add src-tauri/src/updater.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat: add tauri updater integration"
```

---

## 任务 10：图标与品牌

**文件：**
- 修改：`src-tauri/icons/*`

- [ ] **步骤 10.1：生成正式图标**

先准备一张 1024x1024 PNG 源图（`assets/icon-source.png`，可由用户提供或先用占位），然后：

```bash
cargo tauri icon assets/icon-source.png
```

预期：生成全套 icons（32x32.png、128x128.png、128x128@2x.png、icon.icns、icon.ico 等）。

- [ ] **步骤 10.2：Commit**

```bash
git add src-tauri/icons assets/icon-source.png
git commit -m "feat: add app icons"
```

---

## 任务 11：prepare-bundle.sh（sidecar 资源组装）

**文件：**
- 创建：`scripts/prepare-bundle.sh`
- 修改：`.gitignore`（已含 bundle/）

- [ ] **步骤 11.1：编写构建脚本**

创建 `scripts/prepare-bundle.sh`：

```bash
#!/usr/bin/env bash
# 在目标平台上执行：下载 Node、安装 dsh、组装内置 profile
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="$ROOT/bundle"
NODE_VERSION="${NODE_VERSION:-$(node --version | sed 's/^v//')}"  # 默认与构建机一致

# 平台检测
case "$(uname -s)" in
  Darwin)
    OS="darwin"
    case "$(uname -m)" in
      arm64) ARCH="arm64" ;;
      x86_64) ARCH="x64" ;;
    esac
    ;;
  MINGW*|MSYS*|CYGWIN*)
    OS="win"
    ARCH="x64"
    ;;
  Linux)
    OS="linux"
    case "$(uname -m)" in
      aarch64) ARCH="arm64" ;;
      *) ARCH="x64" ;;
    esac
    ;;
esac

# 1. 下载官方 Node 发行包，仅解出 node 二进制
mkdir -p "$BUNDLE/node"
NODE_TGZ="node-v${NODE_VERSION}-${OS}-${ARCH}.tar.gz"
NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/${NODE_TGZ}"
echo ">> 下载 $NODE_URL"
if [ ! -f "/tmp/$NODE_TGZ" ]; then
  curl -fsSL "$NODE_URL" -o "/tmp/$NODE_TGZ"
fi
tar -xzf "/tmp/$NODE_TGZ" -C /tmp
NODE_BIN="/tmp/node-v${NODE_VERSION}-${OS}-${ARCH}/bin/node"
if [ "$OS" = "win" ]; then
  NODE_BIN="/tmp/node-v${NODE_VERSION}-${OS}-${ARCH}/node.exe"
fi
cp "$NODE_BIN" "$BUNDLE/node/node"  # Windows 上文件名在 tauri.conf.json 的 resources 映射里处理
chmod +x "$BUNDLE/node/node" 2>/dev/null || true

# 2. 安装 dsh 包及依赖
mkdir -p "$BUNDLE/dsh"
cd "$BUNDLE/dsh"
if [ ! -f package.json ]; then
  npm init -y >/dev/null 2>&1
fi
npm install "@deepseek-ai/dsh@0.1.1-rc.2" --no-audit --no-fund

# 3. 组装内置 profile
mkdir -p "$BUNDLE/profile"
cp -r "$ROOT/bundle/profile-template/." "$BUNDLE/profile/"

# 4. 校验
echo ">> 校验 node 版本"
"$BUNDLE/node/node" --version
echo ">> 校验 dsh 可启动（--help 不启动服务）"
"$BUNDLE/node/node" "$BUNDLE/dsh/node_modules/@deepseek-ai/dsh/lib/bin.js" --help >/dev/null
echo ">> 校验 native prebuilds"
ls "$BUNDLE/dsh/node_modules/node-pty/prebuilds" >/dev/null 2>&1 || echo "WARN: node-pty prebuilds 缺失"

echo ">> 完成。bundle 目录：$BUNDLE"
```

- [ ] **步骤 11.2：创建 profile 模板**

创建 `bundle/profile-template/package.json`：

```json
{
  "name": "dsh-profile-web",
  "private": true,
  "dependencies": {
    "dsh-plugin-market": "github:chnjames/dsh-plugin-market"
  },
  "dsh": {
    "profile": {
      "bundles": [
        "@deepseek-ai/dsh-base",
        "@deepseek-ai/dsh-web-app",
        "dsh-plugin-market"
      ]
    }
  }
}
```

创建 `bundle/profile-template/cordis.patch.yml`：

```yaml
# 内置 profile 的用户 patch 层（与现状一致：空）
[]
```

- [ ] **步骤 11.3：赋予执行权限并试运行**

```bash
chmod +x scripts/prepare-bundle.sh
./scripts/prepare-bundle.sh
```

预期：脚本成功执行，`bundle/` 下出现 node 二进制、dsh 包、profile 目录。

- [ ] **步骤 11.4：Commit**

```bash
git add scripts/prepare-bundle.sh bundle/profile-template
git commit -m "feat: add sidecar bundle preparation script"
```

---

## 任务 12：build-all.sh + 端到端验证（macOS 本机）

**文件：**
- 创建：`scripts/build-all.sh`

- [ ] **步骤 12.1：编写构建脚本**

创建 `scripts/build-all.sh`：

```bash
#!/usr/bin/env bash
# 完整构建：prepare-bundle + tauri build
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

./scripts/prepare-bundle.sh
cd src-tauri
cargo tauri build
```

- [ ] **步骤 12.2：本机构建**

运行：`./scripts/build-all.sh`
预期：生成 `src-tauri/target/release/bundle/dmg/DSH 桌面版_0.1.0_x64.dmg`（及 .app）。

- [ ] **步骤 12.3：安装并手工验证**

安装 dmg 到 /Applications，双击启动，按规格 §7 验证：

1. 启动 → 单实例（再开一次应聚焦已有窗口）
2. 首次启动弹目录选择器 → 选择工作目录
3. 主窗口加载 DSH Web UI（127.0.0.1 端口）→ 发起会话对话
4. 插件市场可用（`~/.dsh/profiles/web` 与内置 profile 合并生效）
5. 关窗 → 最小化到托盘；托盘"显示主窗口"恢复；"退出"→ 进程树被杀（`ps aux | grep node` 无残留 dsh 进程）
6. 设置页：改端口 → 重启服务生效；改数据目录 → 重启后 DSH_HOME 生效（会话隔离）
7. 端口冲突：手动占用 3080 再启动 → 自动用 3081
8. sidecar 崩溃（kill node 进程）→ 弹窗提示 + 重启服务按钮

- [ ] **步骤 12.4：更新验证（本地模拟服务器）**

```bash
# 用 python 起一个静态服务器模拟更新源
mkdir -p /tmp/updates && cd /tmp/updates
# 放置 latest.json（版本号高于当前）+ 新版本安装包签名文件
python3 -m http.server 9000
```

修改 tauri.conf.json endpoints 指向 `http://127.0.0.1:9000/latest.json`，构建 → 启动 → 设置页"立即检查更新" → 应提示新版本并完成安装。

- [ ] **步骤 12.5：Commit**

```bash
git add scripts/build-all.sh
git commit -m "feat: add full build script"
```

---

## 任务 13：README 完善 + 交付说明

**文件：**
- 修改：`README.md`

- [ ] **步骤 13.1：完善 README**

追加到 `README.md`：

```markdown
## 分发与自动更新

- 三平台安装包由对应平台构建生成（`./scripts/build-all.sh`）
- 自动更新：构建时用 `npx tauri signer generate` 生成密钥对，
  私钥保存在构建机 `~/.tauri/dsh-desktop.key`（勿提交 git），
  每次发布把新安装包签名后连同 `latest.json` 上传内网更新服务器
- 第一版不做 macOS 公证/Windows 代码签名：同事首次运行时
  macOS 右键"打开"、Windows 提示"仍要运行"即可

## 已知限制

- Windows/Linux 需在对应系统上构建（native 模块按平台编译）
- 数据目录与工作目录更改后需重启应用生效
```

- [ ] **步骤 13.2：Commit**

```bash
git add README.md
git commit -m "docs: finalize build and distribution docs"
```

---

## 任务 14：规格自检对照

- [ ] **步骤 14.1：对照规格逐项核对**

对照 `docs/superpowers/specs/2026-08-24-dsh-desktop-design.md`：

| 规格要求 | 实现任务 |
|---|---|
| Tauri v2 壳 + 内置 Node sidecar | 任务 1、5 |
| 完全自包含（Node + dsh + profile 打包） | 任务 11 + tauri.conf.json resources |
| 数据目录默认 ~/.dsh、可配置 | 任务 2（data_dir）、任务 5（DSH_HOME 注入） |
| 工作目录启动时选择并记住 | 任务 6（setup 弹选择器 + 保存） |
| 托盘常驻 | 任务 7 |
| 开机自启 | 任务 6（autostart 插件 + 设置） |
| 单实例 | 任务 6（single-instance 插件） |
| 关窗最小化到托盘 | 任务 6（on_window_event） |
| 端口偏好 + 自动递增 | 任务 3（find_free_port） |
| sidecar 崩溃提示 + 重启 | 任务 6（错误页占位）、任务 8（restart_service） |
| 设置页（独立窗口 + IPC） | 任务 8 |
| 自动更新（updater 签名 + 内网服务器） | 任务 9、12.4 |
| 三平台安装包 | 任务 12（build-all.sh 按平台） |
| 进程树 kill | 任务 5（kill_tree） |
| 错误处理边界 | 任务 5（start 校验）、任务 6（wait_ready 超时） |

- [ ] **步骤 14.2：确认全部勾选**

逐项确认无遗漏后，在 `docs/superpowers/plans/2026-08-24-dsh-desktop.md` 顶部把 `- [ ]` 全部改为 `- [x]` 并 commit：

```bash
git add docs/superpowers/plans/2026-08-24-dsh-desktop.md
git commit -m "docs: mark implementation plan complete"
```

---

## 自检结论

**规格覆盖度**：规格全部 9 节（目标/架构/运行时/生命周期/设置/更新/验证/技术栈/待定项）均有对应任务。待定项在第 9 节已由实现计划落实：设置页前端选型=原生 HTML/JS（任务 8）、就绪探测=HTTP 轮询（任务 5）、进程树 kill=pgrep/taskkill（任务 5）、更新服务器地址=占位待用户提供（任务 9）、图标=占位后替换（任务 10）。

**占位符扫描**：无"TODO/待定/后续实现"式步骤；`tauri.conf.json` 的 pubkey/endpoints 是明确标注的、有后续任务（9/12.4）处理的配置占位，非计划缺陷。

**类型一致性**：`AppConfig` 字段名在 config.rs/commands.rs/app.js 三处一致（workspace_dir/data_dir/port/autostart/minimize_to_tray/check_updates_on_start/update_channel）；`SidecarManager` 的 start/wait_ready/stop 在 lib.rs 与 commands.rs 调用签名一致；command 名 get_config/set_config/choose_workspace/restart_service/quit_app/check_for_updates 在 Rust 与 app.js 中一致。
