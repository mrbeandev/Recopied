//! Local-only diagnostics ("telemetry") collector.
//!
//! Every interesting thing the app does is appended as one JSON object per line
//! to `~/.local/share/recopied/telemetry/events.jsonl`. Nothing is ever sent over
//! the network: users can export a diagnostics bundle from Settings and attach it
//! to a bug report themselves.
//!
//! Privacy: events never contain clipboard contents — only sizes, types, short
//! hash prefixes (for correlation) and error messages.

use chrono::{Local, SecondsFormat};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const EVENTS_FILE: &str = "events.jsonl";
const ROTATED_FILE: &str = "events.1.jsonl";
/// Rotate when the live file reaches this size; one rotated file is kept,
/// so telemetry uses at most ~2x this on disk.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
pub const SCHEMA_VERSION: u32 = 1;

static ENABLED: AtomicBool = AtomicBool::new(true);
static WRITER: Mutex<Option<File>> = Mutex::new(None);
static SESSION_ID: OnceLock<String> = OnceLock::new();

pub fn telemetry_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recopied")
        .join("telemetry")
}

/// Identifies one run of the app so events from different launches can be told apart.
pub fn session_id() -> &'static str {
    SESSION_ID.get_or_init(|| {
        format!("{}-{}", Local::now().format("%Y%m%dT%H%M%S"), std::process::id())
    })
}

pub fn init(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    session_id();
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub fn set_enabled(enabled: bool) {
    if enabled {
        ENABLED.store(true, Ordering::SeqCst);
        record("telemetry_enabled", json!({}));
    } else {
        record("telemetry_disabled", json!({}));
        ENABLED.store(false, Ordering::SeqCst);
    }
}

/// Append one event. Never panics and never blocks the caller for long.
pub fn record(event: &str, data: Value) {
    write_event(event, data, true);
}

fn write_event(event: &str, data: Value, blocking: bool) {
    if !is_enabled() {
        return;
    }
    let line = json!({
        "ts": Local::now().to_rfc3339_opts(SecondsFormat::Millis, false),
        "session": session_id(),
        "event": event,
        "data": data,
    })
    .to_string();
    log::debug!("telemetry: {}", line);

    let mut guard = if blocking {
        WRITER.lock().unwrap_or_else(|e| e.into_inner())
    } else {
        // Used from the panic hook: if the panic happened while this thread held
        // the lock, blocking here would deadlock.
        match WRITER.try_lock() {
            Ok(g) => g,
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        }
    };
    if let Err(e) = write_line(&mut guard, &line) {
        eprintln!("recopied: failed to write telemetry event: {}", e);
        *guard = None; // reopen on next event
    }
}

fn open_append(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn write_line(slot: &mut Option<File>, line: &str) -> std::io::Result<()> {
    let dir = telemetry_dir();
    let live = dir.join(EVENTS_FILE);
    if slot.is_none() {
        fs::create_dir_all(&dir)?;
        *slot = Some(open_append(&live)?);
    }

    let len = slot.as_ref().map(|f| f.metadata().map(|m| m.len()).unwrap_or(0)).unwrap_or(0);
    if len >= MAX_FILE_BYTES {
        *slot = None;
        fs::rename(&live, dir.join(ROTATED_FILE))?;
        *slot = Some(open_append(&live)?);
    }

    let mut buf = String::with_capacity(line.len() + 1);
    buf.push_str(line);
    buf.push('\n');
    match slot.as_mut() {
        Some(file) => file.write_all(buf.as_bytes()),
        None => Ok(()),
    }
}

/// Log every panic (any thread) before the default handler runs.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        write_event(
            "panic",
            json!({
                "message": redact_backticks(&payload),
                "location": location,
                "thread": thread,
            }),
            false,
        );
        default_hook(info);
    }));
}

/// Rust's string-slicing panics quote the string being sliced between backticks,
/// which would leak clipboard contents into the log. Strip those segments.
fn redact_backticks(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut inside = false;
    for ch in msg.chars() {
        if ch == '`' {
            if inside {
                out.push_str("<redacted>`");
            } else {
                out.push('`');
            }
            inside = !inside;
        } else if !inside {
            out.push(ch);
        }
    }
    if inside {
        out.push_str("<redacted>");
    }
    out
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

pub fn has_tool(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

fn os_pretty_name() -> Option<String> {
    let text = fs::read_to_string("/etc/os-release").ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
}

/// Facts about the machine that matter for clipboard bugs.
pub fn environment() -> Value {
    json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "os": os_pretty_name(),
        "kernel": fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|s| s.trim().to_string()),
        "arch": std::env::consts::ARCH,
        "session_type": env_var("XDG_SESSION_TYPE"),
        "desktop": env_var("XDG_CURRENT_DESKTOP"),
        "wayland_display": std::env::var_os("WAYLAND_DISPLAY").is_some(),
        "x11_display": std::env::var_os("DISPLAY").is_some(),
        "flatpak": Path::new("/.flatpak-info").exists(),
        "tools": {
            "xclip": has_tool("xclip"),
            "wl-paste": has_tool("wl-paste"),
            "wl-copy": has_tool("wl-copy"),
            "xdotool": has_tool("xdotool"),
        },
    })
}

/// Size in bytes of all telemetry files on disk.
pub fn disk_usage() -> u64 {
    [EVENTS_FILE, ROTATED_FILE]
        .iter()
        .filter_map(|f| fs::metadata(telemetry_dir().join(f)).ok())
        .map(|m| m.len())
        .sum()
}

/// The most recent `max` events, oldest first (rotated file, then live file).
pub fn read_recent_events(max: usize) -> Vec<Value> {
    let dir = telemetry_dir();
    let mut lines: VecDeque<String> = VecDeque::with_capacity(max.min(4096));
    for name in [ROTATED_FILE, EVENTS_FILE] {
        let Ok(file) = File::open(dir.join(name)) else { continue };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if lines.len() == max {
                lines.pop_front();
            }
            lines.push_back(line);
        }
    }
    lines
        .iter()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

/// Where exported bundles go: ~/Downloads if it exists, otherwise the telemetry dir.
pub fn export_dir() -> PathBuf {
    dirs::download_dir()
        .filter(|d| d.is_dir())
        .unwrap_or_else(telemetry_dir)
}

/// Write a diagnostics bundle (already assembled by the caller) to disk.
pub fn write_export(bundle: &Value) -> Result<PathBuf, String> {
    let dir = export_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!(
        "recopied-diagnostics-{}.json",
        Local::now().format("%Y%m%d-%H%M%S")
    ));
    let text = serde_json::to_string_pretty(bundle).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_quoted_content_from_panic_messages() {
        let msg = "byte index 200 is not a char boundary; it is inside '—' (bytes 199..202) of `my secret password`";
        let r = redact_backticks(msg);
        assert!(!r.contains("secret"));
        assert!(r.contains("byte index 200"));
        assert!(r.ends_with("`<redacted>`"));
        assert_eq!(redact_backticks("unterminated `abc"), "unterminated `<redacted>");
    }
}
