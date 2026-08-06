//! 系统托盘模块
//!
//! 提供托盘图标、右键菜单以及窗口显示/隐藏控制。

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Manager,
};

/// 构建并挂载系统托盘，返回 TrayIcon 句柄（调用方需保持其存活）
pub fn setup_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    // ── 菜单项 ──
    let show = MenuItemBuilder::with_id("show", "Show Window").build(app)?;
    let hide = MenuItemBuilder::with_id("hide", "Hide Window").build(app)?;
    let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&hide)
        .item(&separator)
        .item(&quit)
        .build()?;

    // ── 图标 ──
    let icon = app.default_window_icon().cloned().unwrap_or_else(|| {
        // 后备：内嵌一个 32×32 的纯色图标
        tauri::image::Image::new(&[0u8; 32 * 32 * 4], 32, 32)
    });

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("IPFS Desktop (Rust)")
        .on_menu_event(move |app_handle, event| {
            handle_tray_event(app_handle, event.id().as_ref());
        })
        .build(app)?;

    tracing::info!("System tray initialized");
    Ok(tray)
}

/// 托盘菜单事件处理
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    Show,
    Hide,
    Quit,
    Unknown,
}

fn tray_action(id: &str) -> TrayAction {
    match id {
        "show" => TrayAction::Show,
        "hide" => TrayAction::Hide,
        "quit" => TrayAction::Quit,
        _ => TrayAction::Unknown,
    }
}

fn handle_tray_event(app: &AppHandle, id: &str) {
    match tray_action(id) {
        TrayAction::Show => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        TrayAction::Hide => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        TrayAction::Quit => {
            tracing::info!("Quit requested from tray menu");
            app.exit(0);
        }
        TrayAction::Unknown => {
            tracing::warn!("Unknown tray menu event: {}", id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_supported_tray_actions() {
        assert_eq!(tray_action("show"), TrayAction::Show);
        assert_eq!(tray_action("hide"), TrayAction::Hide);
        assert_eq!(tray_action("quit"), TrayAction::Quit);
        assert_eq!(tray_action("unexpected"), TrayAction::Unknown);
    }
}
