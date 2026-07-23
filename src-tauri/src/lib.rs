// 模块声明
mod commands;
mod config;
mod daemon;
mod state;
mod types;

use config::AppConfig;
use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_status,
            commands::start_daemon,
            commands::stop_daemon,
            commands::restart_daemon,
            commands::get_config,
            commands::update_config,
            commands::get_node_id,
        ])
        .setup(|app| {
            tracing::info!("Tauri setup complete");
            tracing::info!("App version: {}", app.package_info().version);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
