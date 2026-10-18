use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// 桌面应用更新源是否已配置：
/// `plugins.updater.endpoints` 非空且 `pubkey` 不是占位值。
///
/// 未配置时（当前默认状态，内置版本即最新）不发起任何网络请求——
/// Tauri updater 对空 endpoints 会返回 `EmptyEndpoints` 英文错误，对用户没有意义。
pub fn is_configured(app: &AppHandle) -> bool {
    let config = app.config();
    let Some(updater) = config.plugins.0.get("updater") else {
        return false;
    };
    let endpoints_ok = updater
        .get("endpoints")
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    let pubkey_ok = updater
        .get("pubkey")
        .and_then(|v| v.as_str())
        .map(|k| !k.is_empty() && !k.contains("REPLACE_WITH"))
        .unwrap_or(false);
    endpoints_ok && pubkey_ok
}

/// 未配置时的统一提示（手动检查直接返回它，不报错）。
const NOT_CONFIGURED_MSG: &str =
    "桌面应用更新源未配置（当前为内置版本）。dsh 本身仍会在启动时自动更新；如需自动更新桌面应用安装包，请在构建时配置 plugins.updater.endpoints 与签名公钥。";

/// 启动时后台检查（不阻塞主线程/UI；失败只落日志不打扰用户）。
/// 未配置更新源时直接跳过，零网络请求。
pub fn check_on_startup(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let cfg = state.config.lock().unwrap().clone();
    if !cfg.check_updates_on_start {
        return;
    }
    if !is_configured(app) {
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

/// 手动检查；返回状态文案（未配置更新源时返回说明，不视为失败）。
pub async fn check(app: &AppHandle) -> Result<String, String> {
    if !is_configured(app) {
        return Ok(NOT_CONFIGURED_MSG.to_string());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 纯函数版本便于单测：判断给定 updater 配置是否可用。
    fn configured_from(value: &serde_json::Value) -> bool {
        let endpoints_ok = value
            .get("endpoints")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        let pubkey_ok = value
            .get("pubkey")
            .and_then(|v| v.as_str())
            .map(|k| !k.is_empty() && !k.contains("REPLACE_WITH"))
            .unwrap_or(false);
        endpoints_ok && pubkey_ok
    }

    #[test]
    fn empty_endpoints_is_not_configured() {
        assert!(!configured_from(&json!({ "endpoints": [], "pubkey": "abc" })));
    }

    #[test]
    fn placeholder_pubkey_is_not_configured() {
        assert!(!configured_from(&json!({
            "endpoints": ["https://example.com/latest.json"],
            "pubkey": "REPLACE_WITH_GENERATED_PUBKEY"
        })));
    }

    #[test]
    fn complete_config_is_configured() {
        assert!(configured_from(&json!({
            "endpoints": ["https://example.com/latest.json"],
            "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk"
        })));
    }
}
