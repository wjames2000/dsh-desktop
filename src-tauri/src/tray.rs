// 系统托盘：显示主窗口 / 打开设置 / 退出
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

/// 创建系统托盘图标与菜单。
///
/// 菜单项：
/// - tray-show：显示并聚焦主窗口
/// - tray-settings：显示并聚焦设置窗口
/// - tray-quit：退出应用（sidecar 由 lib.rs 的 RunEvent::Exit 兜底停止）
///
/// 说明：`build` 会把托盘图标注册进 app 的资源表（resources_table），
/// 因此局部变量 `_tray` 在 setup 结束后被 drop 也不会移除托盘图标。
pub fn setup_tray(app: AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(&app, "tray-show", "显示主窗口", true, None::<&str>)?;
    let settings = MenuItem::with_id(&app, "tray-settings", "打开设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(&app, "tray-quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(&app, &[&show, &settings, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false);

    // 图标缺失时优雅跳过，不 panic；build 失败会经 ? 返回 Err，由 lib.rs 降级处理
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    let _tray = builder
        .on_tray_icon_event(|tray, event| {
            // 左键单击托盘图标：显示并聚焦主窗口（macOS/Windows）
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "tray-settings" => {
                if let Some(w) = app.get_webview_window("settings") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "tray-quit" => {
                // 停 sidecar 后退出（on_window_event 与 RunEvent::Exit 兜底）
                app.exit(0);
            }
            _ => {}
        })
        .build(&app)?;

    Ok(())
}
