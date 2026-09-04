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
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            commands::check_for_updates,
        ])
        .setup(|app| {
            let state = app.state::<AppState>();
            let cfg = state.config.lock().unwrap().clone();

            // 启动 sidecar（若未配置工作目录，用家目录兜底启动）。
            // 注意：setup 跑在主线程，不能在这里调用 dialog 的 blocking API——
            // 主线程阻塞时事件循环不泵动，对话框永远不出现、recv 永不返回 → 应用永久挂起。
            // 首次运行的目录选择交给设置页的 choose_workspace command（任务 8）。
            let workspace = match &cfg.workspace_dir {
                Some(w) if !w.is_empty() => w.clone(),
                _ => dirs::home_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
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
                sm.wait_ready(Duration::from_secs(30))
            };
            match sidecar_ready {
                Ok(()) => {
                    // 导航到认证 URL：dsh >= 0.1.2 的 URL 带进程 token（?token=），
                    // WebView 访问后自动完成 cookie 交换再跳转干净首页；
                    // 0.1.1 的 URL 无 token，直接加载。
                    let url = {
                        let sm = state.sidecar.lock().unwrap();
                        sm.authenticated_url()
                    };
                    match url {
                        Some(u) => {
                            let _ = main_win.navigate(u.parse().unwrap());
                        }
                        None => eprintln!("sidecar 就绪但未捕获到服务 URL"),
                    }
                    let _ = main_win.show();
                }
                Err(e) => {
                    // 显示错误页（任务 7 完善）
                    eprintln!("sidecar 启动失败: {e}");
                    let _ = main_win.show();
                }
            }

            // 托盘：失败不阻止主应用运行（如 Linux 无 StatusNotifier 宿主时）
            if let Err(e) = tray::setup_tray(app.handle().clone()) {
                eprintln!("托盘初始化失败（不影响主应用）: {e}");
            }

            // 开机自启：按配置同步
            if cfg.autostart {
                use tauri_plugin_autostart::ManagerExt;
                let _ = app.autolaunch().enable();
            }

            // 启动时后台检查更新
            crate::updater::check_on_startup(app.handle());

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗行为：
            // - main 窗口：minimize_to_tray=true 时关闭即隐藏（托盘常驻），否则显式退出
            //   （settings 窗口常驻窗口表，运行时"最后一个窗口关闭自动退出"不会触发，
            //   因此 minimize_to_tray=false 时必须显式 exit，否则留下无窗口的僵尸进程）。
            // - settings 窗口：关闭即隐藏（不销毁），托盘 tray-settings 菜单可再次打开。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let cfg = state.config.lock().unwrap().clone();
                match window.label() {
                    "main" => {
                        if cfg.minimize_to_tray {
                            api.prevent_close();
                            let _ = window.hide();
                        } else {
                            // 直接退出：停 sidecar 后显式退出应用
                            let mut sm = state.sidecar.lock().unwrap();
                            sm.stop();
                            app.exit(0);
                        }
                    }
                    "settings" => {
                        // 设置窗口：关闭即隐藏（不销毁），托盘可再次打开
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    _ => {}
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
