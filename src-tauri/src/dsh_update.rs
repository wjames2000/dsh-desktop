//! dsh 运行时自动更新。
//!
//! 启动前检查 npm registry 上的最新 dsh 版本（latest 优先，回退 next），与内置版本
//! 不同则用内置的 node + npm 安装到用户数据目录，sidecar 优先使用更新后的版本。
//! 任何失败都降级为内置版本，不阻塞启动。
//!
//! 目录布局（更新版，即标准 npm 安装布局）：
//! ```text
//! <update_root>/dsh-<version>/
//!   ├── node_modules/@deepseek-ai/dsh/lib/bin.js   # 入口（依赖解析天然正确）
//!   ├── node_modules/...                            # 完整依赖树（含嵌套依赖）
//!   └── package.json                                # 安装根
//! ```
//!
//! 注意：不从 node_modules 复制 lib 到顶层——npm 会把部分依赖装在 dsh 包的
//! 嵌套 node_modules 下（如 @deepseek-ai/dsh-app-boot），复制后从顶层解析会失败。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// dsh 包在 registry 上的名字
const DSH_PKG: &str = "@deepseek-ai/dsh";
/// 默认 registry（国内镜像；与 prepare-bundle.sh 一致）
pub const REGISTRY: &str = "https://registry.npmmirror.com";

/// 查询结果：某个通道对应的最新版本
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteVersions {
    /// latest 标签版本（成功查到才有）
    pub latest: Option<String>,
    /// next 标签版本
    pub next: Option<String>,
}

/// 更新通道：latest 优先，回退 next。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Latest,
    Next,
}

impl Channel {
    /// 从「latest 优先、回退 next」的查询结果里选目标版本。
    /// 返回 None 表示两个通道都没查到。
    pub fn select(self, remote: &RemoteVersions) -> Option<String> {
        match self {
            Channel::Latest => remote.latest.clone(),
            Channel::Next => remote.next.clone().or_else(|| remote.latest.clone()),
        }
    }
}

/// 读取 dsh 包版本（`<dsh_dir>/node_modules/@deepseek-ai/dsh/package.json`）。
/// 内置布局与更新布局都适用（两者都在 dsh_dir/node_modules 下有该包）。
pub fn installed_version(dsh_dir: &Path) -> Option<String> {
    let pkg = dsh_dir
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("package.json");
    let text = fs::read_to_string(pkg).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("version")?.as_str().map(|s| s.to_string())
}

/// 判断远端版本是否值得安装。
/// 语义：远端版本（semver 解析）高于当前版本才更新；解析失败时按字符串不等判断（保守更新）。
pub fn should_update(current: Option<&str>, remote: &str) -> bool {
    let Some(cur) = current else {
        return true; // 读不到当前版本：尝试安装
    };
    if cur == remote {
        return false;
    }
    match (
        semver::Version::parse(cur),
        semver::Version::parse(remote),
    ) {
        (Ok(c), Ok(r)) => r > c,
        _ => cur != remote,
    }
}

/// 在指定通道下查 registry 的 dist-tags，返回目标版本。
/// `channel` 为 Latest 时表示「latest 优先，回退 next」。
pub fn fetch_target_version(channel: Channel, timeout: Duration) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;
    let url = format!("{REGISTRY}/{DSH_PKG}");
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("查询 registry 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("registry 返回 HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp.json().map_err(|e| format!("解析 registry 响应失败: {e}"))?;
    let tags = json.get("dist-tags").cloned().unwrap_or_default();
    let remote = RemoteVersions {
        latest: tags.get("latest").and_then(|v| v.as_str()).map(str::to_string),
        next: tags.get("next").and_then(|v| v.as_str()).map(str::to_string),
    };
    channel
        .select(&remote)
        .ok_or_else(|| "registry 未返回可用的 dist-tags".to_string())
}

/// 更新根目录：`<系统应用数据目录>/dsh-desktop/dsh-runtime`。
pub fn update_root() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("dsh-desktop").join("dsh-runtime")
}

/// 某个版本的安装目录。
pub fn version_dir(root: &Path, version: &str) -> PathBuf {
    root.join(format!("dsh-{version}"))
}

/// 更新目录内是否有可用的 dsh 运行时（node_modules 里的 bin.js 与包声明都在）。
pub fn is_usable(dir: &Path) -> bool {
    dsh_entry_in(dir).is_some()
}

/// 更新目录里 dsh 包的入口（标准 npm 布局：node_modules/@deepseek-ai/dsh/lib/bin.js）。
pub fn dsh_entry_in(dir: &Path) -> Option<PathBuf> {
    let pkg = dir.join("node_modules").join("@deepseek-ai").join("dsh");
    let bin = pkg.join("lib").join("bin.js");
    if bin.is_file() && pkg.join("package.json").is_file() {
        Some(bin)
    } else {
        None
    }
}

/// 解析出本次启动应使用的 dsh 目录：
/// 更新目录里存在可用运行时则用它，否则用内置目录。
pub fn resolve_dsh_dir(root: &Path, builtin: &Path) -> PathBuf {
    let Ok(entries) = fs::read_dir(root) else {
        return builtin.to_path_buf();
    };
    // 选版本号最高的可用目录
    let mut best: Option<(semver::Version, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !is_usable(&path) {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let Some(ver) = name.strip_prefix("dsh-") else { continue };
        let Ok(parsed) = semver::Version::parse(ver) else { continue };
        if best.as_ref().map(|(b, _)| parsed > *b).unwrap_or(true) {
            best = Some((parsed, path));
        }
    }
    best.map(|(_, p)| p).unwrap_or_else(|| builtin.to_path_buf())
}

/// 用内置 node + npm 安装指定版本 dsh 到更新目录，并整理成 sidecar 期望的布局。
///
/// - `node`：内置 node 可执行文件
/// - `npm_cli`：内置 npm 的 CLI 入口（`npm/bin/npm-cli.js`）
/// - 返回安装后的目录（调用方应再确认 `is_usable`）
pub fn install(
    version: &str,
    root: &Path,
    node: &Path,
    npm_cli: &Path,
    registry: &str,
) -> Result<PathBuf, String> {
    let dir = version_dir(root, version);
    // 已存在但不可用：清掉重装
    if dir.exists() {
        if is_usable(&dir) {
            return Ok(dir);
        }
        fs::remove_dir_all(&dir).map_err(|e| format!("清理旧目录失败: {e}"))?;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;

    // 1) npm install --prefix <dir> @deepseek-ai/dsh@<version>
    //    缓存目录放到更新根目录下：避免依赖用户家目录 ~/.npm 的写权限
    //    （受限环境/沙箱下 ~/.npm 可能不可写，写不了缓存会导致安装直接失败）
    let cache = root.join(".npm-cache");
    let _ = fs::create_dir_all(&cache);
    let status = std::process::Command::new(node)
        .arg(npm_cli)
        .arg("install")
        .arg("--prefix")
        .arg(&dir)
        .arg(format!("{DSH_PKG}@{version}"))
        .arg("--registry")
        .arg(registry)
        .arg("--no-audit")
        .arg("--no-fund")
        .arg("--omit=dev")
        .arg("--prefer-offline")
        .arg("--cache")
        .arg(&cache)
        .env("npm_config_cache", &cache)
        .env("npm_config_update_notifier", "false")
        .current_dir(&dir)
        .status()
        .map_err(|e| format!("执行 npm install 失败: {e}"))?;
    if !status.success() {
        return Err(format!("npm install 退出码异常: {status}"));
    }

    // 2) 校验入口（标准 npm 布局，不复制 lib——依赖解析依赖 node_modules 的层级结构）
    if dsh_entry_in(&dir).is_none() {
        return Err("安装后布局校验失败（node_modules 内缺少 dsh 入口）".to_string());
    }
    Ok(dir)
}

/// 清理更新目录，仅保留 `keep_version` 对应目录（以及当前正在使用的目录）。
pub fn cleanup(root: &Path, keep_version: Option<&str>) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let Some(ver) = name.strip_prefix("dsh-") else { continue };
        if Some(ver) == keep_version {
            continue;
        }
        let _ = fs::remove_dir_all(&path);
    }
}

/// 启动前的完整更新流程：查版本 → 需要则安装 → 返回最终应使用的 dsh 目录。
///
/// 失败一律降级返回 `builtin`，不阻塞启动。
pub fn ensure_updated(
    builtin: &Path,
    node: &Path,
    npm_cli: &Path,
    registry: &str,
    timeout: Duration,
    progress: impl Fn(&str),
) -> PathBuf {
    let root = update_root();
    let _ = fs::create_dir_all(&root);

    let builtin_version = installed_version(builtin);
    progress("正在检查 dsh 更新…");
    let target = match fetch_target_version(Channel::Latest, timeout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[dsh-update] 查询更新失败，使用内置版本: {e}");
            return builtin.to_path_buf();
        }
    };
    if !should_update(builtin_version.as_deref(), &target) {
        let _ = cleanup(&root, builtin_version.as_deref());
        return resolve_dsh_dir(&root, builtin);
    }
    // 已安装过该版本：直接用
    if is_usable(&version_dir(&root, &target)) {
        progress(&format!("dsh {target} 已就绪"));
        return version_dir(&root, &target);
    }

    progress(&format!("正在更新 dsh 到 {target}…"));
    match install(&target, &root, node, npm_cli, registry) {
        Ok(dir) => {
            let _ = cleanup(&root, Some(&target));
            progress(&format!("dsh {target} 更新完成"));
            dir
        }
        Err(e) => {
            eprintln!("[dsh-update] 更新到 {target} 失败，使用内置版本: {e}");
            resolve_dsh_dir(&root, builtin)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_prefers_latest_then_next() {
        let remote = RemoteVersions {
            latest: Some("0.1.5-rc.1".into()),
            next: Some("0.2.0-rc.1".into()),
        };
        assert_eq!(Channel::Latest.select(&remote), Some("0.1.5-rc.1".into()));
        assert_eq!(Channel::Next.select(&remote), Some("0.2.0-rc.1".into()));
        // latest 缺失时回退 next 通道值（Next 语义）
        let only_next = RemoteVersions { latest: None, next: Some("0.2.0".into()) };
        assert_eq!(Channel::Next.select(&only_next), Some("0.2.0".into()));
    }

    #[test]
    fn should_update_uses_semver_order() {
        assert!(should_update(Some("0.1.5-rc.1"), "0.1.6"));
        assert!(!should_update(Some("0.1.5"), "0.1.5"));
        // 预发布 < 正式版
        assert!(should_update(Some("0.1.5-rc.1"), "0.1.5"));
        // 降级不算更新
        assert!(!should_update(Some("0.2.0"), "0.1.5"));
        // 无当前版本时尝试安装
        assert!(should_update(None, "0.1.5"));
    }

    #[test]
    fn resolve_prefers_highest_usable_version() {
        let tmp = std::env::temp_dir().join(format!("dsh-update-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let builtin = tmp.join("builtin");
        fs::create_dir_all(&builtin).unwrap();
        // 构造两个可用版本和一个损坏版本（标准 npm 布局）
        for (ver, usable) in [("0.1.4", true), ("0.1.6", true), ("0.1.7", false)] {
            let dir = version_dir(&tmp, ver);
            let pkg = dir.join("node_modules").join("@deepseek-ai").join("dsh");
            fs::create_dir_all(pkg.join("lib")).unwrap();
            if usable {
                fs::write(pkg.join("lib").join("bin.js"), "x").unwrap();
                fs::write(pkg.join("package.json"), "{\"version\":\"x\"}").unwrap();
            }
        }
        let picked = resolve_dsh_dir(&tmp, &builtin);
        assert_eq!(picked, version_dir(&tmp, "0.1.6"));
        // 全部不可用时回退内置
        let empty_root = tmp.join("empty");
        fs::create_dir_all(&empty_root).unwrap();
        assert_eq!(resolve_dsh_dir(&empty_root, &builtin), builtin);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn installed_version_reads_package_json() {
        let tmp = std::env::temp_dir().join(format!("dsh-ver-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let pkg = tmp.join("node_modules").join("@deepseek-ai").join("dsh");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("package.json"), "{\"version\": \"0.1.5-rc.1\"}").unwrap();
        assert_eq!(installed_version(&tmp), Some("0.1.5-rc.1".to_string()));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cleanup_keeps_requested_version() {
        let tmp = std::env::temp_dir().join(format!("dsh-clean-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        for ver in ["0.1.4", "0.1.5", "0.1.6"] {
            fs::create_dir_all(version_dir(&tmp, ver)).unwrap();
        }
        cleanup(&tmp, Some("0.1.5"));
        assert!(version_dir(&tmp, "0.1.5").exists());
        assert!(!version_dir(&tmp, "0.1.4").exists());
        assert!(!version_dir(&tmp, "0.1.6").exists());
        let _ = fs::remove_dir_all(&tmp);
    }
}
