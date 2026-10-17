mod clipboard;
mod commands;
mod db;
pub mod notification;
pub mod settings;
pub mod telemetry;
pub mod util;

use clipboard::watcher::ClipboardWatcher;
use tauri::Manager;
use tauri::PhysicalPosition;
use log::info;
use serde_json::json;

/// How often the background image cleanup runs (it also runs at startup).
const IMAGE_CLEANUP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

fn start_image_cleanup_scheduler() {
    std::thread::Builder::new()
        .name("image-cleanup".into())
        .spawn(|| {
            // Give the app a moment to finish starting up first.
            std::thread::sleep(std::time::Duration::from_secs(20));
            loop {
                let result = std::panic::catch_unwind(|| commands::run_image_cleanup("scheduled"));
                match result {
                    Ok(Err(e)) => {
                        log::error!("Image cleanup failed: {}", e);
                        telemetry::record("image_cleanup_failed", json!({ "error": e.to_string() }));
                    }
                    Err(_) => telemetry::record("image_cleanup_failed", json!({ "error": "panic" })),
                    Ok(Ok(_)) => {}
                }
                std::thread::sleep(IMAGE_CLEANUP_INTERVAL);
            }
        })
        .ok();
}

pub fn position_window_bottom_right(window: &tauri::WebviewWindow) {
    if let Ok(Some(monitor)) = window.current_monitor() {
        let monitor_size = monitor.size();
        let monitor_pos = monitor.position();
        let win_size = window.outer_size().unwrap_or(tauri::PhysicalSize { width: 380, height: 560 });
        let x = monitor_pos.x + monitor_size.width as i32 - win_size.width as i32 - 12;
        let y = monitor_pos.y + monitor_size.height as i32 - win_size.height as i32 - 48;
        window.set_position(PhysicalPosition::new(x, y)).ok();
    }
}

fn toggle_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            window.hide().ok();
            telemetry::record("window_hidden", json!({ "via": "toggle" }));
        } else {
            // Position near the cursor so "focus follows mouse" works
            position_window_at_cursor(&window);
            window.show().ok();
            window.set_focus().ok();
            telemetry::record("window_shown", json!({ "via": "toggle" }));
        }
    }
}

fn position_window_at_cursor(window: &tauri::WebviewWindow) {
    if let Ok(cursor_pos) = window.cursor_position() {
        let win_size = window.outer_size().unwrap_or(tauri::PhysicalSize { width: 380, height: 560 });
        if let Ok(Some(monitor)) = window.current_monitor() {
            let monitor_size = monitor.size();
            let monitor_pos = monitor.position();

            // Position window so it's near the cursor but stays on screen
            let mut x = cursor_pos.x as i32 - (win_size.width as i32 / 2);
            let mut y = cursor_pos.y as i32 - 20; // slightly above cursor

            // Clamp to monitor bounds
            let right_edge = monitor_pos.x + monitor_size.width as i32;
            let bottom_edge = monitor_pos.y + monitor_size.height as i32;
            if x + win_size.width as i32 > right_edge {
                x = right_edge - win_size.width as i32;
            }
            if x < monitor_pos.x {
                x = monitor_pos.x;
            }
            if y + win_size.height as i32 > bottom_edge {
                y = bottom_edge - win_size.height as i32;
            }
            if y < monitor_pos.y {
                y = monitor_pos.y;
            }

            window.set_position(PhysicalPosition::new(x, y)).ok();
        } else {
            window.set_position(PhysicalPosition::new(cursor_pos.x as i32, cursor_pos.y as i32)).ok();
        }
    } else {
        // Fallback to bottom-right if cursor position unavailable
        position_window_bottom_right(window);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_settings = settings::load_settings();
    telemetry::init(startup_settings.telemetry_enabled);
    telemetry::install_panic_hook();

    let db_path = db::get_db_path();

    if let Err(e) = db::init_db(&db_path) {
        eprintln!("Failed to initialize database: {}", e);
        telemetry::record("db_init_failed", json!({ "error": e.to_string() }));
        std::process::exit(1);
    }

    telemetry::record(
        "app_started",
        json!({
            "environment": telemetry::environment(),
            "settings": &startup_settings,
            "storage": db::queries::storage_stats(&db_path).ok(),
        }),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state == ShortcutState::Pressed {
                        let shortcut_str = shortcut.to_string();
                        info!("Shortcut pressed: {}", shortcut_str);
                        telemetry::record("shortcut_pressed", json!({ "shortcut": shortcut_str }));
                        toggle_window(app);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // System tray
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::TrayIconBuilder;

            let show_item = MenuItemBuilder::with_id("show", "Show Recopied").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Recopied — Clipboard Manager")
                .icon(app.default_window_icon().cloned().unwrap())
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => toggle_window(app),
                    "quit" => {
                        telemetry::record("app_quit", json!({ "via": "tray" }));
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                        toggle_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Start clipboard watcher with app handle for notifications
            let watcher = ClipboardWatcher::new();
            watcher.start(db_path.clone(), app.handle().clone());
            info!("Recopied started — clipboard watcher active");

            // Delete expired / orphaned images now and every few hours.
            start_image_cleanup_scheduler();

            // Register global shortcut from saved settings
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let saved = settings::load_settings();
            match app.global_shortcut().register(saved.shortcut.as_str()) {
                Ok(_) => {
                    info!("Registered {} global shortcut", saved.shortcut);
                    telemetry::record("shortcut_registered", json!({ "shortcut": saved.shortcut }));
                }
                Err(e) => {
                    eprintln!("Failed to register {}: {}", saved.shortcut, e);
                    telemetry::record("shortcut_register_failed", json!({ "shortcut": saved.shortcut, "error": e.to_string() }));
                    // Try Ctrl+Shift+V as fallback
                    match app.global_shortcut().register("Ctrl+Shift+V") {
                        Ok(_) => {
                            info!("Registered Ctrl+Shift+V as fallback shortcut");
                            telemetry::record("shortcut_registered", json!({ "shortcut": "Ctrl+Shift+V", "fallback": true }));
                        }
                        Err(e2) => {
                            eprintln!("Failed to register fallback shortcut: {}", e2);
                            telemetry::record("shortcut_register_failed", json!({ "shortcut": "Ctrl+Shift+V", "fallback": true, "error": e2.to_string() }));
                        }
                    }
                }
            }

            // Show window on startup during development
            if let Some(window) = app.get_webview_window("main") {
                position_window_bottom_right(&window);
                window.show().ok();
                window.set_focus().ok();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_history,
            commands::search_history,
            commands::pin_item,
            commands::delete_item,
            commands::clear_history,
            commands::paste_item,
            commands::hide_window,
            commands::start_drag,
            commands::set_window_mode,
            commands::get_settings,
            commands::set_shortcut,
            commands::set_autostart,
            commands::set_clipboard_limit,
            commands::set_auto_paste,
            commands::set_notification_settings,
            commands::test_notification,
            commands::cleanup_images_now,
            commands::set_image_retention_days,
            commands::set_telemetry_enabled,
            commands::get_diagnostics,
            commands::export_diagnostics,
            commands::open_diagnostics_folder,
            commands::record_ui_event,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Recopied");
}
