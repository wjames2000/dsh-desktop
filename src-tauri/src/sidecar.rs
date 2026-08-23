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
