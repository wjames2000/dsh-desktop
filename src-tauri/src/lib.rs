mod commands;
mod config;
mod port;
mod sidecar;
mod tray;
mod updater;

use std::sync::Mutex;
use std::time::Duration;
use tauri::Manager;

pub struct AppState {
    pub config: Mutex<config::AppConfig>,
    pub sidecar: Mutex<sidecar::SidecarManager>,
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 二次启动：聚焦主窗口
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState {
            config: Mutex::new(config::AppConfig::load()),
            sidecar: Mutex::new(sidecar::SidecarManager::new()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::choose_workspace,
            commands::restart_service,
            commands::quit_app,
        ])
        .setup(|app| {
            let state = app.state::<AppState>();
            let cfg = state.config.lock().unwrap().clone();

            // 启动 sidecar（若未配置工作目录，弹目录选择器）
            let workspace = match &cfg.workspace_dir {
                Some(w) if !w.is_empty() => w.clone(),
                _ => {
                    // 首次启动：弹原生目录选择器
                    use tauri_plugin_dialog::DialogExt;
                    let picked = app.dialog().file().blocking_pick_folder();
                    match picked {
                        Some(p) => p.to_string(),
                        None => {
                            // 用户取消：用家目录兜底
                            dirs::home_dir()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        }
                    }
                }
            };
            // 记住选择
            {
                let mut cfg2 = state.config.lock().unwrap();
                cfg2.workspace_dir = Some(workspace.clone());
                let _ = cfg2.save();
            }

            // 探测端口并启动 sidecar
            let port = port::find_free_port(cfg.port);
            let data_dir = cfg.data_dir.clone();
            {
                let mut sm = state.sidecar.lock().unwrap();
                sm.start(&workspace, data_dir.as_deref(), port)
                    .map_err(|e| e.to_string())?;
            }

            // 等待就绪后显示主窗口
            let main_win = app.get_webview_window("main").unwrap();
            let sidecar_ready = {
                let mut sm = state.sidecar.lock().unwrap();
                sm.wait_ready(port, Duration::from_secs(30))
            };
            match sidecar_ready {
                Ok(()) => {
                    let _ = main_win.navigate(
                        format!("http://127.0.0.1:{port}").parse().unwrap(),
                    );
                    let _ = main_win.show();
                }
                Err(e) => {
                    // 显示错误页（任务 7 完善）
                    eprintln!("sidecar 启动失败: {e}");
                    let _ = main_win.show();
                }
            }

            // 托盘
            tray::setup_tray(app.handle().clone())?;

            // 开机自启：按配置同步
            if cfg.autostart {
                use tauri_plugin_autostart::ManagerExt;
                let _ = app.autolaunch().enable();
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗行为：最小化到托盘或退出
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let cfg = state.config.lock().unwrap().clone();
                if cfg.minimize_to_tray && window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    // 直接退出：停 sidecar
                    let mut sm = state.sidecar.lock().unwrap();
                    sm.stop();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_handle, event| {
        if let tauri::RunEvent::Exit = event {
            // 确保 sidecar 随应用退出
            let state = _handle.state::<AppState>();
            let mut sm = state.sidecar.lock().unwrap();
            sm.stop();
        }
    });
}
