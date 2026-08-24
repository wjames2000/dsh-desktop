fn main() {
    // 通过 AppManifest 注册自定义 command，为每个 command 自动生成 allow-<name>/deny-<name>
    // ACL 权限（如 allow-get-config）。默认 tauri_build::build() 不注册任何 command，
    // capabilities 引用 allow-* 时会因权限不存在而构建失败；不注册则远程页面调用
    // command 会被 ACL 拒绝（Command not allowed by ACL）。
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "get_config",
                "set_config",
                "choose_workspace",
                "restart_service",
                "quit_app",
                "check_for_updates",
            ]),
        ),
    )
    .expect("failed to run tauri build script");
}
