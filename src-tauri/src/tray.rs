// 系统托盘：显示主窗口 / 打开设置 / 退出
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

/// 创建系统托盘图标与菜单。
///
/// 菜单项：
/// - show：显示并聚焦主窗口
/// - settings：显示并聚焦设置窗口
/// - quit：退出应用（sidecar 由 lib.rs 的 RunEvent::Exit 兜底停止）
///
/// 说明：`build` 会把托盘图标注册进 app 的资源表（resources_table），
/// 因此局部变量 `_tray` 在 setup 结束后被 drop 也不会移除托盘图标。
pub fn setup_tray(app: AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(&app, "show", "显示主窗口", true, None::<&str>)?;
    let settings = MenuItem::with_id(&app, "settings", "打开设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(&app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(&app, &[&show, &settings, &quit])?;

    let icon = app.default_window_icon().cloned().expect(
        "default window icon 缺失：请检查 tauri.conf.json 的 bundle.icon 是否配置了 PNG 图标",
    );

    let _tray = TrayIconBuilder::new()
        .icon(icon)
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
        .build(&app)?;

    Ok(())
}
