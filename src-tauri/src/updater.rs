use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// 启动时静默检查（失败静默）
pub fn check_on_startup(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let cfg = state.config.lock().unwrap().clone();
    if !cfg.check_updates_on_start {
        return;
    }
    let app = app.clone();
    // 在 Tauri 异步运行时后台执行，不阻塞主线程/UI；失败静默（更新是锦上添花）
    tauri::async_runtime::spawn(async move {
        let _ = check(&app).await;
    });
}

/// 手动检查；返回状态文案
pub async fn check(app: &AppHandle) -> Result<String, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => {
            let version = update.version.clone();
            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| e.to_string())?;
            // 安装成功后需用户重启应用生效（自动重启后续发布流程稳定后再加，YAGNI）
            Ok(format!("新版本 {version} 已安装，请重启应用生效"))
        }
        None => Ok("已是最新版本".to_string()),
    }
}
