use tauri::command;
use tauri::Manager;

use crate::clipboard::types::ClipboardItem;
use crate::db;
use crate::notification;
use crate::settings::{self, NotificationSettings};

#[command]
pub fn get_history(limit: u32, offset: u32) -> Result<Vec<ClipboardItem>, String> {
    let db_path = db::get_db_path();
    db::queries::get_history(&db_path, limit, offset).map_err(|e| e.to_string())
}

#[command]
pub fn search_history(query: String) -> Result<Vec<ClipboardItem>, String> {
    let db_path = db::get_db_path();
    db::queries::search_history(&db_path, &query).map_err(|e| e.to_string())
}

#[command]
pub fn pin_item(id: i64) -> Result<bool, String> {
    let db_path = db::get_db_path();
    db::queries::pin_item(&db_path, id).map_err(|e| e.to_string())
}

#[command]
pub fn delete_item(id: i64) -> Result<bool, String> {
    let db_path = db::get_db_path();
    db::queries::delete_item(&db_path, id).map_err(|e| e.to_string())
}

#[command]
pub fn clear_history() -> Result<bool, String> {
    let db_path = db::get_db_path();
    db::queries::clear_history(&db_path).map_err(|e| e.to_string())
}

#[command]
pub async fn hide_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[command]
pub async fn start_drag(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.start_dragging().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[command]
pub async fn set_window_mode(app: tauri::AppHandle, fullscreen: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if fullscreen {
            // Maximize the window to cover the full screen
            window.maximize().map_err(|e| e.to_string())?;
        } else {
            // Restore windowed size using logical (CSS) pixels
            window.unmaximize().map_err(|e| e.to_string())?;
            use tauri::LogicalSize;
            window.set_size(LogicalSize::new(380.0f64, 560.0f64)).map_err(|e| e.to_string())?;
            crate::position_window_bottom_right(&window);
        }
        window.set_focus().ok();
    }
    Ok(())
}

#[command]
pub async fn paste_item(id: i64, app: tauri::AppHandle) -> Result<(), String> {
    let db_path = db::get_db_path();
    let items = db::queries::get_history(&db_path, 500, 0).map_err(|e| e.to_string())?;

    let item = items.iter().find(|i| i.id == id).ok_or("Item not found")?;

    // Write the item content to clipboard using arboard
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;

    match item.content_type.as_str() {
        "text" => {
            if let Some(ref text) = item.text_content {
                clipboard.set_text(text).map_err(|e| e.to_string())?;
            }
        }
        "image" => {
            if let Some(ref path) = item.image_path {
                let img = image::open(path).map_err(|e| e.to_string())?;
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let img_data = arboard::ImageData {
                    width: w as usize,
                    height: h as usize,
                    bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                };
                clipboard.set_image(img_data).map_err(|e| e.to_string())?;
            }
        }
        _ => return Err("Unknown content type".to_string()),
    }

    // Hide the window after copying
    if let Some(window) = app.get_webview_window("main") {
        window.hide().ok();
    }

    Ok(())
}

#[command]
pub fn get_settings() -> Result<settings::AppSettings, String> {
    Ok(settings::load_settings())
}

#[command]
pub async fn set_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    // Validate the shortcut by trying to parse it
    let parsed: tauri_plugin_global_shortcut::Shortcut = shortcut
        .parse()
        .map_err(|e| format!("Invalid shortcut: {}", e))?;

    // Unregister all existing shortcuts
    app.global_shortcut().unregister_all().map_err(|e| e.to_string())?;

    // Register the new shortcut
    app.global_shortcut()
        .register(parsed)
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    // Save to settings
    let mut current = settings::load_settings();
    current.shortcut = shortcut;
    settings::save_settings(&current)?;

    Ok(())
}

#[command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    settings::set_autostart(enabled)?;

    let mut current = settings::load_settings();
    current.autostart = enabled;
    settings::save_settings(&current)?;

    Ok(())
}

#[command]
pub fn set_clipboard_limit(limit: u32) -> Result<(), String> {
    let safe = limit.max(1); // minimum of 1, no upper cap
    let mut current = settings::load_settings();
    current.clipboard_limit = safe;
    settings::save_settings(&current)?;
    Ok(())
}

#[command]
pub fn set_notification_settings(notification: NotificationSettings) -> Result<(), String> {
    let mut current = settings::load_settings();
    current.notification = notification;
    settings::save_settings(&current)?;
    Ok(())
}

#[command]
pub async fn test_notification(app: tauri::AppHandle) -> Result<(), String> {
    notification::send_copy_notification(
        &app,
        "text",
        Some("This is a test notification from Recopied!"),
    );
    Ok(())
}
