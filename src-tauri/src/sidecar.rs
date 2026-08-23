use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

/// 从可执行文件路径解析 sidecar 资源根目录。
/// 布局规则：
/// - macOS 打包：<App>.app/Contents/MacOS/<exe> → 资源在上级 Contents/Resources
/// - Windows 打包（NSIS）与 dev 模式：资源与 exe 同目录
/// - Linux 打包（deb）：exe 在 /usr/bin，资源在 /usr/lib/<productName>/
/// - Linux 打包（AppImage）：exe 在 $APPDIR/usr/bin，资源在 $APPDIR/usr/lib/<productName>/
pub fn resource_dir_for(exe: &Path) -> PathBuf {
    let dir = exe.parent().expect("exe has no parent");

    // macOS 打包：Contents/MacOS/<exe> → Contents/Resources
    #[cfg(target_os = "macos")]
    {
        if dir.file_name().map(|s| s == "MacOS").unwrap_or(false) {
            return dir.parent().unwrap().join("Resources");
        }
    }

    // Linux：先判断 exe 是否在打包位置，再决定走打包探测还是 dev 兜底
    #[cfg(target_os = "linux")]
    {
        // 判断 exe 是否在 AppImage 挂载目录（$APPDIR/usr/bin）
        let in_appdir = std::env::var("APPDIR")
            .ok()
            .map(|appdir| dir.starts_with(Path::new(&appdir).join("usr").join("bin")))
            .unwrap_or(false);
        // 判断 exe 是否在 deb 安装位置（/usr/bin）
        let in_usr_bin = dir == Path::new("/usr/bin");

        if in_appdir {
            if let Ok(appdir) = std::env::var("APPDIR") {
                let lib = Path::new(&appdir).join("usr").join("lib");
                if let Some(p) = find_resource_dir(&lib) {
                    return p;
                }
            }
            // AppImage 探测失败：回退 exe 同目录，不 fall-through 到宿主 /usr/lib
        } else if in_usr_bin {
            if let Some(p) = find_resource_dir(Path::new("/usr/lib")) {
                return p;
            }
            // deb 探测失败：回退 exe 同目录
        }
        // dev 模式（或打包探测失败）：exe 同目录
        return dir.to_path_buf();
    }

    // Windows 打包与 dev 模式：exe 同目录
    dir.to_path_buf()
}

/// 在 lib 目录下寻找包含 sidecar 资源的子目录（<productName>/）。
/// 校验：必须是目录，且包含 node 子目录（有 node 二进制）或 dsh 子目录（有 lib/bin.js）。
/// 找不到返回 None。
///
/// 说明：此函数仅在 Linux 分支被调用，但故意不加 #[cfg(target_os = "linux")]，
/// 以便在 macOS 上也能被单元测试覆盖（它接收 lib 路径参数，天然可测）。
#[allow(dead_code)]
fn find_resource_dir(lib: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(lib).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // 校验资源存在性：node 二进制 或 dsh/lib/bin.js
        let node_name = if cfg!(target_os = "windows") { "node.exe" } else { "node" };
        let has_node = path.join("node").join(node_name).exists();
        let has_dsh = path.join("dsh").join("lib").join("bin.js").exists();
        if has_node || has_dsh {
            return Some(path);
        }
    }
    None
}

/// 当前可执行文件对应的资源目录
pub fn resource_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("failed to get exe path");
    resource_dir_for(&exe)
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

pub struct SidecarManager {
    child: Option<Child>,
    log_rx: Receiver<String>,
}

impl SidecarManager {
    pub fn new() -> Self {
        // 日志管道占位：接收端保留给任务 8（设置页日志展示）接线，本任务仅编译占位
        Self { child: None, log_rx: mpsc::channel().1 }
    }

    /// 启动 dsh web sidecar。
    /// data_dir 为 None 时不注入 DSH_HOME（用默认 ~/.dsh）。
    pub fn start(&mut self, workspace: &str, data_dir: Option<&str>, port: u16) -> Result<(), String> {
        if self.child.is_some() {
            return Err("DSH 服务已在运行中，请先停止再启动".to_string());
        }
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

        // 占位日志管道：持续排空 stdout，防止子进程写满管道阻塞。
        // 发送端暂未与 self.log_rx 连通，任务 8 接设置页日志展示。
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
        // 与 stdout 相同模式：持续排空 stderr，防止 Node 警告/错误日志
        // 写满管道缓冲（macOS 16KB / Linux 64KB）导致子进程 write(2) 永久阻塞。
        if let Some(mut err) = child.stderr.take() {
            let tx = mpsc::channel().0;
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match err.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => { let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string()); }
                    }
                }
            });
        }

        self.child = Some(child);
        Ok(())
    }

    /// 轮询 http://127.0.0.1:{port} 直到 200，超时 30s。
    /// 每轮轮询同时检测子进程是否提前退出（如端口冲突导致 dsh 立即退出），
    /// 避免白等超时，也避免连上抢占端口的其他 HTTP 服务。
    pub fn wait_ready(&mut self, port: u16, timeout: Duration) -> Result<(), String> {
        if self.child.is_none() {
            return Err("DSH 服务未启动，无法等待就绪".to_string());
        }
        let url = format!("http://127.0.0.1:{port}");
        let deadline = Instant::now() + timeout;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())?;
        while Instant::now() < deadline {
            // 检测子进程是否提前退出（如端口冲突）
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let code = status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    return Err(format!(
                        "DSH 服务进程提前退出（exit code: {code}），可能端口 {port} 被占用。请重试或更换端口。"
                    ));
                }
            }
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
    /// macOS/Linux 用 pgrep 递归；Windows 用 taskkill /T /F。
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
                            // 强杀后再回收一次，避免留下僵尸进程
                            let _ = child.wait();
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

/// 应用退出时清理 sidecar：Child drop 不会 kill 子进程，
/// 不实现 Drop 会让 dsh 变成孤儿进程继续运行。
impl Drop for SidecarManager {
    fn drop(&mut self) {
        self.stop();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_layout_resolves_to_resources() {
        let exe = Path::new("/Applications/DSH 桌面版.app/Contents/MacOS/dsh-desktop");
        let dir = resource_dir_for(exe);
        assert_eq!(dir, Path::new("/Applications/DSH 桌面版.app/Contents/Resources"));
    }

    #[test]
    fn windows_layout_is_exe_dir() {
        // 用正斜杠形式的 Windows 路径，保证非 Windows 平台也能按多组件路径解析
        let exe = Path::new("C:/Program Files/DSH 桌面版/dsh-desktop.exe");
        let dir = resource_dir_for(exe);
        assert_eq!(dir, Path::new("C:/Program Files/DSH 桌面版"));
    }

    #[test]
    fn dev_layout_is_exe_dir() {
        let exe = Path::new("/tmp/dsh-desktop/target/debug/dsh-desktop");
        let dir = resource_dir_for(exe);
        assert_eq!(dir, Path::new("/tmp/dsh-desktop/target/debug"));
    }

    #[test]
    fn find_resource_dir_matches_content() {
        // 构造临时目录树：lib/<product>/node/node + lib/<product>/dsh/lib/bin.js
        let tmp = std::env::temp_dir().join(format!("dsh-desktop-res-test-{}", std::process::id()));
        let product = tmp.join("lib").join("DSH 桌面版");
        std::fs::create_dir_all(product.join("node")).unwrap();
        std::fs::create_dir_all(product.join("dsh").join("lib")).unwrap();
        std::fs::write(product.join("node").join("node"), "x").unwrap();
        std::fs::write(product.join("dsh").join("lib").join("bin.js"), "x").unwrap();

        let found = find_resource_dir(&tmp.join("lib"));
        assert_eq!(found, Some(product));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn find_resource_dir_ignores_empty_dirs() {
        let tmp = std::env::temp_dir().join(format!("dsh-desktop-res-test2-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("lib").join("empty")).unwrap();
        std::fs::write(tmp.join("lib").join("some-file"), "x").unwrap();

        let found = find_resource_dir(&tmp.join("lib"));
        assert_eq!(found, None);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn paths_point_to_resources() {
        assert!(node_binary().to_string_lossy().ends_with("node") || node_binary().to_string_lossy().ends_with("node.exe"));
        assert!(dsh_dir().to_string_lossy().contains("dsh"));
        assert!(profile_dir().to_string_lossy().contains("profile"));
    }

    #[test]
    fn kill_nonexistent_pid_is_ok() {
        // 不存在的 pid：kill_tree 应返回 Ok 或不 panic
        let _ = kill_tree(999_999);
    }
}
