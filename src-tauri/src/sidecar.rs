use std::path::{Path, PathBuf};

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
}
