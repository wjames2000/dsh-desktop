use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// 启动时后台检查（不阻塞主线程/UI；失败只落日志不打扰用户）。
/// 未配置真实更新源时（endpoints 为空）check 立即返回 EmptyEndpoints 错误，零网络请求。
pub fn check_on_startup(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let cfg = state.config.lock().unwrap().clone();
    if !cfg.check_updates_on_start {
        return;
    }
    let app = app.clone();
    // 在 Tauri 异步运行时后台执行；失败记日志便于排障（更新是锦上添花，不打扰用户）
    tauri::async_runtime::spawn(async move {
        if let Err(e) = check(&app).await {
            eprintln!("[dsh-desktop] 启动时检查更新失败: {e}");
        }
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
