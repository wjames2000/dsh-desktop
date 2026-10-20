mod auth;
mod commands;
mod config;
mod dsh_update;
mod port;
mod sidecar;
mod tray;
mod updater;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

pub struct AppState {
    pub config: Mutex<config::AppConfig>,
    pub sidecar: Mutex<sidecar::SidecarManager>,
    /// 本次启动使用的 dsh 运行时目录（内置或自动更新后的目录）。
    /// restart_service 复用它，避免重启时又回到内置版本。
    pub dsh_root: Mutex<PathBuf>,
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
            dsh_root: Mutex::new(sidecar::dsh_dir()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::choose_workspace,
            commands::restart_service,
            commands::quit_app,
            commands::check_for_updates,
            commands::get_runtime_info,
        ])
        .setup(|app| {
            // 工作目录：未配置时用家目录兜底（首次运行的目录选择交给设置页的
            // choose_workspace command）。注意 setup 跑在主线程，不能调用 dialog
            // 的 blocking API，也不做耗时的网络/安装操作——否则事件循环不泵动，
            // 窗口无法显示进度、界面挂起。
            let cfg = {
                let state = app.state::<AppState>();
                let c = state.config.lock().unwrap().clone();
                c
            };
            let workspace = match &cfg.workspace_dir {
                Some(w) if !w.is_empty() => w.clone(),
                _ => dirs::home_dir().unwrap_or_default().to_string_lossy().to_string(),
            };
            {
                let state = app.state::<AppState>();
                let mut cfg2 = state.config.lock().unwrap();
                cfg2.workspace_dir = Some(workspace);
                let _ = cfg2.save();
            }

            // 先显示主窗口（此时加载本地设置页，展示启动进度），
            // 之后的 dsh 自动更新 + sidecar 启动 + 导航都在后台线程完成。
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }

            // 后台启动序列：dsh 更新 → sidecar 启动 → 就绪 → 导航主窗口
            let app_handle = app.handle().clone();
            std::thread::spawn(move || boot_sequence(app_handle));

            // 托盘：失败不阻止主应用运行（如 Linux 无 StatusNotifier 宿主时）
            if let Err(e) = tray::setup_tray(app.handle().clone()) {
                eprintln!("托盘初始化失败（不影响主应用）: {e}");
            }

            // 开机自启：按配置同步
            if cfg.autostart {
                use tauri_plugin_autostart::ManagerExt;
                let _ = app.autolaunch().enable();
            }

            // 启动时后台检查桌面应用自身更新
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

/// 启动序列（后台线程）：dsh 自动更新 → 启动 sidecar → 等待就绪 → 导航主窗口。
///
/// 进度通过 `boot-progress` 事件发给前端（本地设置页展示），出错发 `boot-error`。
/// 任何一步失败都降级处理：dsh 更新失败用内置版本继续；sidecar 失败则提示并保留
/// 设置页（本地 origin，有完整命令权限）供用户自救（重启服务、改端口等）。
fn boot_sequence(app: tauri::AppHandle) {
    let emit = {
        let app = app.clone();
        move |msg: &str| {
            let _ = app.emit("boot-progress", msg.to_string());
        }
    };
    let fail = {
        let app = app.clone();
        move |msg: &str| {
            eprintln!("[boot] {msg}");
            let _ = app.emit("boot-error", msg.to_string());
        }
    };

    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let workspace = cfg.workspace_dir.clone().unwrap_or_default();
    let data_dir = cfg.data_dir.clone();

    // 1. dsh 自动更新（latest 优先；失败/超时一律降级内置版本，不阻塞启动）
    let builtin = sidecar::dsh_dir();
    let node = sidecar::node_binary();
    let npm = sidecar::npm_cli();
    let dsh_root = if npm.exists() {
        dsh_update::ensure_updated(
            &builtin,
            &node,
            &npm,
            dsh_update::REGISTRY,
            Duration::from_secs(15),
            |m: &str| emit(m),
        )
    } else {
        eprintln!("[boot] 内置 npm 缺失，跳过 dsh 自动更新（使用内置版本）");
        builtin.clone()
    };
    *state.dsh_root.lock().unwrap() = dsh_root.clone();

    // 2. 启动 sidecar
    emit("正在启动 DSH 服务…");
    let port = port::find_free_port(cfg.port);
    {
        let mut sm = state.sidecar.lock().unwrap();
        if let Err(e) = sm.start(&workspace, data_dir.as_deref(), port, Some(&dsh_root)) {
            fail(&format!("DSH 服务启动失败：{e}（可在设置页点击「重启服务」重试）"));
            return;
        }
    }

    // 3. 等待就绪（首次初始化 profile 实测 20-30s，慢磁盘更久 → 120s）
    emit("正在等待 DSH 服务就绪…");
    let ready = {
        let mut sm = state.sidecar.lock().unwrap();
        sm.wait_ready(Duration::from_secs(120))
    };
    match ready {
        Ok(()) => {
            // 认证引导：dsh >= 0.1.2 的 URL 带进程 token。由 auth::bootstrap 在
            // Rust 侧完成 token→cookie 交换，导航后用 JS 注入 cookie 并 reload——
            // 避免 WebView 自身 cookie 交换失败（页面停在 authentication required）。
            let url = {
                let sm = state.sidecar.lock().unwrap();
                sm.authenticated_url()
            };
            match url {
                Some(auth_url) => {
                    if let Some(win) = app.get_webview_window("main") {
                        if let Err(e) = auth::bootstrap(&app, &auth_url) {
                            fail(&format!("认证引导失败：{e}"));
                        }
                        let _ = win.show();
                    }
                }
                None => fail("服务已就绪但未捕获到认证 URL（请重启服务）"),
            }
        }
        Err(e) => fail(&format!("{e}（可在设置页点击「重启服务」重试）")),
    }
}
