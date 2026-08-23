use std::path::{Path, PathBuf};

/// 从可执行文件路径解析 sidecar 资源根目录。
/// 布局规则：
/// - macOS 打包：<App>.app/Contents/MacOS/<exe> → 资源在上级 Contents/Resources
/// - Windows 打包（NSIS）与 dev 模式：资源与 exe 同目录
/// - Linux 打包（deb/AppImage）：资源在 /usr/lib/<productName>/（AppImage 下为 $APPDIR/usr/lib/<productName>/）
pub fn resource_dir_for(exe: &Path) -> PathBuf {
    let dir = exe.parent().expect("exe has no parent");

    // macOS 打包：Contents/MacOS/<exe> → Contents/Resources
    #[cfg(target_os = "macos")]
    {
        if dir.file_name().map(|s| s == "MacOS").unwrap_or(false) {
            return dir.parent().unwrap().join("Resources");
        }
    }

    // Linux 打包：deb 布局 /usr/lib/<productName>/，AppImage 布局 $APPDIR/usr/lib/<productName>/
    #[cfg(target_os = "linux")]
    {
        if let Ok(appdir) = std::env::var("APPDIR") {
            // AppImage：exe 在 $APPDIR/usr/bin，资源在 $APPDIR/usr/lib/<productName>/
            let lib = Path::new(&appdir).join("usr").join("lib");
            if let Ok(entries) = std::fs::read_dir(&lib) {
                let mut names: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                names.sort();
                if let Some(name) = names.first() {
                    return lib.join(name);
                }
            }
        }
        // deb：exe 在 /usr/bin，资源在 /usr/lib/<productName>/
        let lib = Path::new("/usr/lib");
        if let Ok(entries) = std::fs::read_dir(lib) {
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            names.sort();
            if let Some(name) = names.first() {
                return lib.join(name);
            }
        }
        // 兜底：exe 同目录（dev 模式）
        return dir.to_path_buf();
    }

    // Windows 打包与 dev 模式：exe 同目录
    dir.to_path_buf()
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
    fn paths_point_to_resources() {
        assert!(node_binary().to_string_lossy().ends_with("node") || node_binary().to_string_lossy().ends_with("node.exe"));
        assert!(dsh_dir().to_string_lossy().contains("dsh"));
        assert!(profile_dir().to_string_lossy().contains("profile"));
    }
}
