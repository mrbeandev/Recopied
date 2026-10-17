use serde_json::json;
use tauri::command;
use tauri::Manager;

use crate::clipboard::types::ClipboardItem;
use crate::clipboard::watcher;
use crate::db;
use crate::notification;
use crate::settings::{self, NotificationSettings};
use crate::telemetry;
use crate::util::short_hash;

/// Record a failed command and pass the error through to the UI.
fn fail(command: &str, error: impl ToString) -> String {
    let error = error.to_string();
    telemetry::record("command_error", json!({ "command": command, "error": error }));
    error
}

#[command]
pub fn get_history(limit: u32, offset: u32) -> Result<Vec<ClipboardItem>, String> {
    // Polled every second by the UI — only failures are recorded.
    let db_path = db::get_db_path();
    db::queries::get_history(&db_path, limit, offset).map_err(|e| fail("get_history", e))
}

#[command]
pub fn search_history(query: String) -> Result<Vec<ClipboardItem>, String> {
    let db_path = db::get_db_path();
    db::queries::search_history(&db_path, &query).map_err(|e| fail("search_history", e))
}

#[command]
pub fn pin_item(id: i64) -> Result<bool, String> {
    let db_path = db::get_db_path();
    let pinned = db::queries::pin_item(&db_path, id).map_err(|e| fail("pin_item", e))?;
    telemetry::record("item_pin_toggled", json!({ "id": id, "found": pinned.is_some(), "pinned": pinned }));
    Ok(pinned.is_some())
}

#[command]
pub fn delete_item(id: i64) -> Result<bool, String> {
    let db_path = db::get_db_path();
    let (deleted, report) = db::queries::delete_item(&db_path, id).map_err(|e| fail("delete_item", e))?;
    telemetry::record("item_deleted", json!({ "id": id, "deleted": deleted, "cleanup": report }));
    Ok(deleted)
}

#[command]
pub fn clear_history() -> Result<bool, String> {
    let db_path = db::get_db_path();
    let report = db::queries::clear_history(&db_path).map_err(|e| fail("clear_history", e))?;
    log::info!(
        "Cleared history: {} items, {} image files ({} bytes)",
        report.removed_items, report.removed_files, report.freed_bytes
    );
    telemetry::record("history_cleared", json!(report));
    // Let the item that is currently on the clipboard be recorded again.
    watcher::forget_seen();
    Ok(report.removed_items > 0 || report.removed_files > 0)
}

/// Run the image cleanup now (age-based expiry + orphan sweep).
#[command]
pub fn cleanup_images_now() -> Result<db::queries::CleanupReport, String> {
    run_image_cleanup("manual").map_err(|e| fail("cleanup_images_now", e))
}

pub fn run_image_cleanup(trigger: &str) -> Result<db::queries::CleanupReport, rusqlite::Error> {
    let days = settings::load_settings().image_retention_days;
    let report = db::queries::cleanup_images(&db::get_db_path(), days)?;
    if report.removed_files > 0 || !report.errors.is_empty() || trigger == "manual" {
        log::info!(
            "Image cleanup ({}): {} items, {} files ({} orphans), {} bytes freed",
            trigger, report.removed_items, report.removed_files, report.orphan_files, report.freed_bytes
        );
    }
    telemetry::record("image_cleanup", json!({ "trigger": trigger, "retentionDays": days, "report": report }));
    Ok(report)
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
    let started = std::time::Instant::now();
    match paste_item_inner(id, &app) {
        Ok(details) => {
            let mut details = details;
            details["ms"] = json!(started.elapsed().as_millis() as u64);
            telemetry::record("item_pasted", details);
            Ok(())
        }
        Err(e) => {
            telemetry::record("paste_failed", json!({ "id": id, "error": e }));
            Err(e)
        }
    }
}

fn paste_item_inner(id: i64, app: &tauri::AppHandle) -> Result<serde_json::Value, String> {
    let db_path = db::get_db_path();
    // Look the item up directly (the old code scanned the 500 newest items, so
    // pasting a pinned item beyond that window failed with "Item not found").
    let item = db::queries::get_item(&db_path, id)
        .map_err(|e| e.to_string())?
        .ok_or("Item not found")?;

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

    // Optionally simulate Ctrl+V in the window that regains focus after the
    // popup hides. Runs on a background thread so the command returns
    // immediately; the short delay lets the window manager restore focus.
    let auto_paste = settings::load_settings().auto_paste;
    if auto_paste {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let result = std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", "ctrl+v"])
                .status();
            match result {
                Ok(status) if status.success() => telemetry::record("auto_paste_sent", json!({})),
                Ok(status) => {
                    log::warn!("auto-paste: xdotool exited with {status}");
                    telemetry::record("auto_paste_failed", json!({ "error": status.to_string() }));
                }
                Err(e) => {
                    log::warn!("auto-paste failed (is xdotool installed?): {e}");
                    telemetry::record("auto_paste_failed", json!({ "error": e.to_string() }));
                }
            }
        });
    }

    Ok(json!({
        "id": item.id,
        "type": item.content_type,
        "pinned": item.pinned,
        "hash": short_hash(&item.hash),
        "autoPaste": auto_paste,
    }))
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
    telemetry::record("setting_changed", json!({ "setting": "shortcut", "from": current.shortcut, "to": shortcut }));
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
    telemetry::record("setting_changed", json!({ "setting": "autostart", "to": enabled }));

    Ok(())
}

#[command]
pub fn set_auto_paste(enabled: bool) -> Result<(), String> {
    let mut settings = settings::load_settings();
    settings.auto_paste = enabled;
    settings::save_settings(&settings)?;
    telemetry::record("setting_changed", json!({ "setting": "autoPaste", "to": enabled }));
    Ok(())
}

#[command]
pub fn set_clipboard_limit(limit: u32) -> Result<(), String> {
    let safe = limit.max(1); // minimum of 1, no upper cap
    let mut current = settings::load_settings();
    current.clipboard_limit = safe;
    settings::save_settings(&current)?;
    telemetry::record("setting_changed", json!({ "setting": "clipboardLimit", "to": safe }));
    Ok(())
}

#[command]
pub fn set_notification_settings(notification: NotificationSettings) -> Result<(), String> {
    let mut current = settings::load_settings();
    telemetry::record("setting_changed", json!({ "setting": "notification", "to": &notification }));
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

#[command]
pub fn set_image_retention_days(days: u32) -> Result<(), String> {
    let mut current = settings::load_settings();
    current.image_retention_days = days;
    settings::save_settings(&current)?;
    telemetry::record("setting_changed", json!({ "setting": "imageRetentionDays", "to": days }));
    Ok(())
}

#[command]
pub fn set_telemetry_enabled(enabled: bool) -> Result<(), String> {
    let mut current = settings::load_settings();
    current.telemetry_enabled = enabled;
    settings::save_settings(&current)?;
    telemetry::set_enabled(enabled);
    Ok(())
}

/// Health snapshot shown in the Diagnostics section of Settings.
#[command]
pub fn get_diagnostics() -> Result<serde_json::Value, String> {
    let storage = db::queries::storage_stats(&db::get_db_path()).map_err(|e| fail("get_diagnostics", e))?;
    Ok(json!({
        "telemetryEnabled": telemetry::is_enabled(),
        "telemetryDir": telemetry::telemetry_dir(),
        "telemetryBytes": telemetry::disk_usage(),
        "session": telemetry::session_id(),
        "watcher": watcher::status(),
        "storage": storage,
        "environment": telemetry::environment(),
    }))
}

/// Write a JSON bundle (environment, settings, watcher + storage health, recent
/// events) that a user can attach to a bug report. Returns the file path.
#[command]
pub fn export_diagnostics() -> Result<String, String> {
    let mut settings_json = serde_json::to_value(settings::load_settings()).unwrap_or_default();
    // The shortcut is harmless; everything else in settings is non-personal too.
    settings_json["telemetryEnabled"] = json!(telemetry::is_enabled());
    let bundle = json!({
        "kind": "recopied-diagnostics",
        "schemaVersion": telemetry::SCHEMA_VERSION,
        "exportedAt": chrono::Local::now().to_rfc3339(),
        "session": telemetry::session_id(),
        "environment": telemetry::environment(),
        "settings": settings_json,
        "watcher": watcher::status(),
        "storage": db::queries::storage_stats(&db::get_db_path()).ok(),
        "events": telemetry::read_recent_events(5000),
    });
    let path = telemetry::write_export(&bundle).map_err(|e| fail("export_diagnostics", e))?;
    telemetry::record("diagnostics_exported", json!({ "path": path }));
    Ok(path.to_string_lossy().to_string())
}

/// Open the folder containing telemetry logs / exported bundles in the file manager.
#[command]
pub fn open_diagnostics_folder(path: Option<String>) -> Result<(), String> {
    let target = match path {
        Some(p) => std::path::PathBuf::from(p)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(telemetry::telemetry_dir),
        None => telemetry::telemetry_dir(),
    };
    std::fs::create_dir_all(&target).ok();
    std::process::Command::new("xdg-open")
        .arg(&target)
        .spawn()
        .map(|_| ())
        .map_err(|e| fail("open_diagnostics_folder", e))
}

/// Lets the UI log its own errors and lifecycle events into the same log.
#[command]
pub fn record_ui_event(event: String, data: Option<serde_json::Value>) -> Result<(), String> {
    let name = format!("ui_{}", crate::util::truncate_at_char_boundary(&event, 64));
    telemetry::record(&name, data.unwrap_or(json!({})));
    Ok(())
}
