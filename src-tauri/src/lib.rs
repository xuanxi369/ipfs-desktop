// 模块声明
pub mod commands;
pub mod config;
pub mod daemon;
pub mod error;
pub mod state;
pub mod tray;
pub mod types;
pub mod cache;
pub mod keyring;
pub mod proxy;
pub mod offline_queue;
pub mod bandwidth;
pub mod backend_trait;
pub mod kubo_adapter;
pub mod iroh_adapter;
pub mod compat_test;
pub mod benchmark;

use config::AppConfig;
use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tauri::Manager;

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
    let file_appender = tracing_appender::rolling::daily(log_dir, "app.log");
    
    // 初始化订阅者
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(file_appender))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .init();
    
    tracing::info!("Logging initialized");
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            commands::set_auto_launch,
            commands::get_auto_launch,
            commands::cat_file,
            commands::download_file,
            commands::get_file_size,
            commands::get_pin_list,
            commands::add_pin,
            commands::remove_pin,
            commands::get_dashboard_stats,
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
        ])
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
