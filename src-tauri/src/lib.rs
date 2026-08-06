// 模块声明
pub mod atomic_file;
pub mod backend_router;
pub mod backend_trait;
pub mod bandwidth;
pub mod benchmark;
pub mod cache;
pub mod commands;
pub mod commands_binary;
pub mod commands_mfs;
pub mod compat_test;
pub mod config;
pub mod content_index;
pub mod daemon;
pub mod error;
pub mod identity;
pub mod iroh_adapter;
pub mod keyring;
pub mod kubo_adapter;
pub mod offline_queue;
pub mod path_security;
pub mod peer_geo;
pub mod proxy;
pub mod state;
pub mod tray;
pub mod types;

use config::AppConfig;
use state::AppState;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

/// 初始化日志系统
fn init_logging() {
    // 获取日志目录
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap())
        .join("ipfs-desktop-rust")
        .join("logs");

    // 创建日志目录
    std::fs::create_dir_all(&log_dir).ok();

    // 创建文件日志
    let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
    let (file_writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .lossy(false)
        .finish(file_appender);
    // Keep the worker alive until process exit; dropping this guard stops file writes.
    let _ = LOG_GUARD.set(guard);

    // 初始化订阅者
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .init();

    tracing::info!(path = %log_dir.display(), "Logging initialized");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志
    init_logging();

    tracing::info!("IPFS Desktop Rust starting...");

    // 加载配置
    let config = AppConfig::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config: {}, using defaults", e);
        AppConfig::default()
    });
    tracing::info!("Configuration loaded");
    tracing::info!("IPFS Path: {:?}", config.get_ipfs_path());
    tracing::info!("API Address: {}", config.api_addr);

    // 创建应用状态
    let app_state = AppState::new(config);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_status,
            commands::start_daemon,
            commands::stop_daemon,
            commands::restart_daemon,
            commands::get_config,
            commands::update_config,
            commands::get_node_id,
            commands::open_webui,
            commands::get_webui_url,
            commands::add_file,
            commands::add_files,
            commands::add_file_with_progress,
            commands::list_content,
            commands::remove_content_record,
            commands::set_auto_launch,
            commands::get_auto_launch,
            commands::cat_file,
            commands::download_file,
            commands::get_file_size,
            commands::get_pin_list,
            commands::add_pin,
            commands::remove_pin,
            commands::get_dashboard_stats,
            commands::get_peer_geography,
            commands::get_cached_dashboard,
            commands::generate_key,
            commands::list_keys,
            commands::delete_key,
            commands::ipns_publish,
            commands::ipns_resolve,
            commands::get_proxy_stats,
            commands::set_prefetch_hint,
            commands::get_offline_queue,
            commands::flush_offline_queue,
            commands::get_bandwidth_config,
            commands::set_bandwidth_config,
            commands::get_bandwidth_status,
            commands::add_file_safe,
            commands::get_active_backend,
            commands::switch_backend,
            commands::get_backend_capabilities,
            commands::run_compat_test,
            commands::run_benchmark,
            // Phase B (a): iroh 原生收发 + BlobTicket
            commands::iroh_add_file,
            commands::iroh_node_info,
            commands::iroh_share,
            commands::iroh_fetch_ticket,
            commands::iroh_register_ticket,
            commands::iroh_keep,
            commands::iroh_unkeep,
            commands::iroh_shutdown,
            // Phase C (b): 双栈路由
            commands::get_route_policy,
            commands::set_route_policy,
            commands::get_backend_route,
            commands::get_usage_mode,
            commands::set_usage_mode,
            commands::get_migration_status,
            // Phase D1: 节点身份
            commands::get_node_identity,
            commands::set_node_label,
            commands::export_identity,
            // Phase D3: 节点健康度
            commands::get_node_health,
            // 二进制哈希校验
            commands_binary::get_binary_verification_info,
            commands_binary::set_binary_hash,
            // MFS (Mutable File System)
            commands_mfs::mfs_ls,
            commands_mfs::mfs_stat,
            commands_mfs::mfs_mkdir,
            commands_mfs::mfs_rm,
            commands_mfs::mfs_cp,
            commands_mfs::mfs_mv,
            commands_mfs::mfs_read,
            commands_mfs::mfs_write,
        ])
        .on_window_event(|window, event| {
            // Phase D2「可长期在线」：关窗不退出，隐藏到托盘让节点后台常驻。
            // 真正退出走托盘菜单的 Quit（app.exit）。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
                tracing::info!("Window close intercepted — hidden to tray, node keeps running");
            }
        })
        .setup(|app| {
            tracing::info!("Tauri setup complete");
            tracing::info!("App version: {}", app.package_info().version);

            // 初始化系统托盘（manage 保持 TrayIcon 存活，防止被 Drop 移除）
            let tray = tray::setup_tray(app.handle())?;
            app.handle().manage(tray);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
