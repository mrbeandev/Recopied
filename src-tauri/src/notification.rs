use log::{error, info};
use tauri::AppHandle;

use crate::settings;

/// Send a copy notification if enabled in settings.
pub fn send_copy_notification(_app: &AppHandle, content_type: &str, preview: Option<&str>) {
    let ns = settings::load_settings().notification;
    if !ns.enabled {
        return;
    }

    let body = if ns.show_content {
        match (content_type, preview) {
            ("text", Some(p)) => {
                let truncated = if p.len() > 100 {
                    format!("{}...", &p[..100])
                } else {
                    p.to_string()
                };
                format!("Copied: {}", truncated)
            }
            ("image", _) => "Image copied to clipboard".to_string(),
            _ => "Copied to clipboard".to_string(),
        }
    } else {
        match content_type {
            "image" => "Image copied to clipboard".to_string(),
            _ => "Text copied to clipboard".to_string(),
        }
    };

    if let Err(e) = notify_rust::Notification::new()
        .summary("Recopied")
        .body(&body)
        .icon("com.recopied.app")
        .timeout(notify_rust::Timeout::Milliseconds(2000))
        .show()
    {
        error!("Failed to send notification: {}", e);
    } else {
        info!("Notification sent: {}", content_type);
    }
}
