use crate::{config::AppConfig, AppState};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

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
pub async fn choose_workspace(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    // blocking API 禁止在主线程调用：主线程阻塞时事件循环不泵动，对话框不出现、recv 永不返回 → 永久挂起。
    // async command 跑在 Tauri 运行时线程池，可以安全地阻塞等待用户选择目录。
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn restart_service(app: AppHandle, state: State<'_, AppState>) -> Result<u16, String> {
    let cfg = state.config.lock().unwrap().clone();
    let workspace = cfg.workspace_dir.clone().unwrap_or_default();
    let data_dir = cfg.data_dir.clone();
    // find_free_port 可能返回与上次不同的端口（原端口被占用 → 端口漂移）
    let port = crate::port::find_free_port(cfg.port);
    let mut sm = state.sidecar.lock().unwrap();
    sm.stop();
    sm.start(&workspace, data_dir.as_deref(), port)?;
    sm.wait_ready(port, Duration::from_secs(30))?;
    // 成功后导航主窗口到实际端口，避免前端停留在旧端口 URL 死页
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.navigate(format!("http://127.0.0.1:{port}").parse().unwrap());
    }
    Ok(port)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<String, String> {
    crate::updater::check(&app).await
}
