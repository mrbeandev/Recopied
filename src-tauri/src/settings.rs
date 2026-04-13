use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+V";
const DEFAULT_CLIPBOARD_LIMIT: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub show_content: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_content: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub shortcut: String,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_clipboard_limit")]
    pub clipboard_limit: u32,
    #[serde(default)]
    pub notification: NotificationSettings,
}

fn default_clipboard_limit() -> u32 {
    DEFAULT_CLIPBOARD_LIMIT
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            autostart: false,
            clipboard_limit: DEFAULT_CLIPBOARD_LIMIT,
            notification: NotificationSettings::default(),
        }
    }
}

fn settings_path() -> PathBuf {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recopied");
    fs::create_dir_all(&data_dir).ok();
    data_dir.join("settings.json")
}

fn autostart_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart")
        .join("recopied.desktop")
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let desktop_path = autostart_path();

    if enabled {
        // Find the executable path
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let autostart_dir = desktop_path.parent().unwrap();
        fs::create_dir_all(autostart_dir).map_err(|e| e.to_string())?;

        let desktop_entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Recopied\n\
             Comment=Clipboard history manager\n\
             Exec={}\n\
             Icon=recopied\n\
             Terminal=false\n\
             Categories=Utility;\n\
             StartupNotify=false\n\
             X-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        fs::write(&desktop_path, desktop_entry).map_err(|e| e.to_string())?;
    } else if desktop_path.exists() {
        fs::remove_file(&desktop_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&data) {
                return settings;
            }
        }
    }
    let defaults = AppSettings::default();
    save_settings(&defaults).ok();
    defaults
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}
