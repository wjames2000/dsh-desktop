use crate::{config::AppConfig, AppState};
use tauri::{AppHandle, State};

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
pub fn choose_workspace(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub fn restart_service(state: State<AppState>) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    let workspace = cfg.workspace_dir.clone().unwrap_or_default();
    let port = crate::port::find_free_port(cfg.port);
    let data_dir = cfg.data_dir.clone();
    let mut sm = state.sidecar.lock().unwrap();
    sm.stop();
    sm.start(&workspace, data_dir.as_deref(), port)?;
    sm.wait_ready(port, std::time::Duration::from_secs(30))?;
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
