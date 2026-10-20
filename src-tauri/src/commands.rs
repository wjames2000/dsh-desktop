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
pub async fn restart_service(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state.config.lock().unwrap().clone();
    let workspace = cfg.workspace_dir.clone().unwrap_or_default();
    let data_dir = cfg.data_dir.clone();
    // 复用本次启动选定的 dsh 运行时（可能是自动更新后的目录）
    let dsh_root = state.dsh_root.lock().unwrap().clone();
    // find_free_port 可能返回与上次不同的端口（原端口被占用 → 端口漂移）
    let port = crate::port::find_free_port(cfg.port);
    let mut sm = state.sidecar.lock().unwrap();
    sm.stop();
    sm.start(&workspace, data_dir.as_deref(), port, Some(&dsh_root))?;
    sm.wait_ready(Duration::from_secs(120))?;
    // 成功后重新引导认证：新进程 token 变了，由 auth::bootstrap 重新取 cookie 注入并 reload。
    let auth_url = sm
        .authenticated_url()
        .ok_or_else(|| "服务已重启但未捕获到认证 URL".to_string())?;
    drop(sm);
    crate::auth::bootstrap(&app, &auth_url)?;
    Ok(auth_url)
}

/// 当前运行时信息（设置页展示用）：内置版本、实际使用版本、是否来自自动更新。
#[tauri::command]
pub fn get_runtime_info(state: State<AppState>) -> serde_json::Value {
    let used = state.dsh_root.lock().unwrap().clone();
    let builtin = crate::sidecar::dsh_dir();
    let builtin_version = crate::dsh_update::installed_version(&builtin);
    let used_version = crate::dsh_update::installed_version(&used);
    let is_updated = used != builtin;
    serde_json::json!({
        "builtin_version": builtin_version,
        "runtime_version": used_version,
        "runtime_dir": used.to_string_lossy(),
        "from_update": is_updated,
    })
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<String, String> {
    crate::updater::check(&app).await
}
