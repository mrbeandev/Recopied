use sha2::{Digest, Sha256};
use serde_json::json;
use std::panic::AssertUnwindSafe;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use log::{debug, error, info, warn};
use tauri::AppHandle;

use crate::clipboard::types::NewClipboardItem;
use crate::db;
use crate::notification;
use crate::settings;
use crate::telemetry;
use crate::util::{short_hash, truncate_at_char_boundary};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const READ_TIMEOUT: Duration = Duration::from_millis(1500);
/// If the poll loop has not completed an iteration for this long, the
/// supervisor assumes it is stuck and starts a replacement.
const STALL_THRESHOLD_SECS: u64 = 30;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(600);
const PREVIEW_BYTES: usize = 200;
const NO_IMAGE: &str = "No image on clipboard";

// ── Shared watcher status (read by the supervisor and the diagnostics UI) ────
static GENERATION: AtomicU64 = AtomicU64::new(0);
static FORGET_EPOCH: AtomicU64 = AtomicU64::new(0);
static LAST_TICK: AtomicU64 = AtomicU64::new(0);
static LAST_CAPTURE: AtomicU64 = AtomicU64::new(0);
static CAPTURES: AtomicU64 = AtomicU64::new(0);
static SAVE_FAILURES: AtomicU64 = AtomicU64::new(0);
static RESTARTS: AtomicU64 = AtomicU64::new(0);
static ITERATION_PANICS: AtomicU64 = AtomicU64::new(0);

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Snapshot of watcher health for the diagnostics panel / export bundle.
pub fn status() -> serde_json::Value {
    let now = now_secs();
    let ago = |ts: u64| if ts == 0 { None } else { Some(now.saturating_sub(ts)) };
    json!({
        "generation": GENERATION.load(Ordering::SeqCst),
        "secondsSinceLastPoll": ago(LAST_TICK.load(Ordering::SeqCst)),
        "secondsSinceLastCapture": ago(LAST_CAPTURE.load(Ordering::SeqCst)),
        "capturesThisSession": CAPTURES.load(Ordering::SeqCst),
        "saveFailuresThisSession": SAVE_FAILURES.load(Ordering::SeqCst),
        "restartsThisSession": RESTARTS.load(Ordering::SeqCst),
        "recoveredPanicsThisSession": ITERATION_PANICS.load(Ordering::SeqCst),
    })
}

/// Make the watcher forget what it has already seen, so the item currently on
/// the clipboard is recorded again (used after "Clear all").
pub fn forget_seen() {
    FORGET_EPOCH.fetch_add(1, Ordering::SeqCst);
}

/// Run a subprocess with a hard timeout. Kills the process if it exceeds the deadline.
/// This prevents xclip/wl-paste from hanging the watcher thread indefinitely.
fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<std::process::Output, Box<dyn std::error::Error + Send + Sync>> {
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pid = child.id();
    let (tx, rx) = mpsc::channel::<Result<std::process::Output, std::io::Error>>();

    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => Ok(result?),
        Err(_) => {
            // Kill the hung process so the spawned thread can exit cleanly
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            Err(format!("clipboard command timed out after {}ms", timeout.as_millis()).into())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DisplayServer {
    X11,
    Wayland,
}

fn detect_display_server() -> DisplayServer {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        DisplayServer::Wayland
    } else {
        DisplayServer::X11
    }
}

/// Tracks consecutive failures of one clipboard reader so the telemetry log
/// gets one event when a problem starts (or changes) and one when it recovers,
/// instead of an event every 500ms.
struct ErrorStreak {
    kind: &'static str,
    count: u64,
    last_message: String,
}

impl ErrorStreak {
    fn new(kind: &'static str) -> Self {
        Self { kind, count: 0, last_message: String::new() }
    }

    fn fail(&mut self, message: String) {
        self.count += 1;
        if self.count == 1 || self.count % 1000 == 0 || message != self.last_message {
            telemetry::record(
                "clipboard_read_error",
                json!({ "kind": self.kind, "error": message, "consecutive": self.count }),
            );
        }
        self.last_message = message;
    }

    fn ok(&mut self) {
        if self.count > 0 {
            telemetry::record(
                "clipboard_read_recovered",
                json!({ "kind": self.kind, "failedPolls": self.count }),
            );
        }
        self.count = 0;
        self.last_message.clear();
    }
}

/// Per-loop state. Text and image keep separate "last seen" hashes: with one
/// shared hash, a copy that offers both formats (e.g. an image copied from a
/// browser) made the two readers overwrite each other and re-save every poll.
struct LoopState {
    display: DisplayServer,
    forget_epoch: u64,
    last_text_hash: String,
    last_image_hash: String,
    text_errors: ErrorStreak,
    image_errors: ErrorStreak,
    targets_errors: ErrorStreak,
}

pub struct ClipboardWatcher;

impl ClipboardWatcher {
    pub fn new() -> Self {
        Self
    }

    /// Start the poll loop plus a supervisor that restarts it if it stalls.
    pub fn start(&self, db_path: String, app_handle: AppHandle) {
        spawn_poll_loop(db_path.clone(), app_handle.clone(), "start");

        let spawned = thread::Builder::new()
            .name("clipboard-supervisor".into())
            .spawn(move || loop {
                thread::sleep(Duration::from_secs(10));
                let last = LAST_TICK.load(Ordering::SeqCst);
                let stalled_for = now_secs().saturating_sub(last);
                if last != 0 && stalled_for > STALL_THRESHOLD_SECS {
                    warn!("Clipboard watcher stalled for {}s — restarting", stalled_for);
                    telemetry::record("watcher_stalled", json!({ "stalledForSecs": stalled_for }));
                    spawn_poll_loop(db_path.clone(), app_handle.clone(), "stall");
                }
            });
        if let Err(e) = spawned {
            telemetry::record("watcher_spawn_failed", json!({ "thread": "supervisor", "error": e.to_string() }));
        }
    }
}

fn spawn_poll_loop(db_path: String, app_handle: AppHandle, reason: &'static str) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if reason != "start" {
        RESTARTS.fetch_add(1, Ordering::SeqCst);
    }
    LAST_TICK.store(now_secs(), Ordering::SeqCst);

    let result = thread::Builder::new()
        .name(format!("clipboard-watcher-{}", generation))
        .spawn(move || run_poll_loop(generation, db_path, app_handle, reason));
    if let Err(e) = result {
        error!("Failed to spawn clipboard watcher thread: {}", e);
        telemetry::record("watcher_spawn_failed", json!({ "thread": "watcher", "error": e.to_string() }));
    }
}

fn run_poll_loop(generation: u64, db_path: String, app_handle: AppHandle, reason: &'static str) {
    let display = detect_display_server();
    info!("Clipboard watcher #{} started (display server: {:?}, reason: {})", generation, display, reason);
    telemetry::record(
        "watcher_started",
        json!({ "generation": generation, "displayServer": format!("{:?}", display), "reason": reason }),
    );

    let mut state = LoopState {
        display,
        forget_epoch: FORGET_EPOCH.load(Ordering::SeqCst),
        last_text_hash: String::new(),
        last_image_hash: String::new(),
        text_errors: ErrorStreak::new("text"),
        image_errors: ErrorStreak::new("image"),
        targets_errors: ErrorStreak::new("targets"),
    };
    let mut last_heartbeat = Instant::now();

    // A newer generation means the supervisor replaced this loop (it was
    // considered stalled) — exit so two loops never run at once.
    while GENERATION.load(Ordering::SeqCst) == generation {
        LAST_TICK.store(now_secs(), Ordering::SeqCst);

        let epoch = FORGET_EPOCH.load(Ordering::SeqCst);
        if epoch != state.forget_epoch {
            state.forget_epoch = epoch;
            state.last_text_hash.clear();
            state.last_image_hash.clear();
        }

        // Catch panics per iteration: a single bad clipboard value must never
        // kill history recording for the rest of the session again.
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            poll_once(&mut state, &db_path, &app_handle);
        }));
        if outcome.is_err() {
            // Message + location were already logged by the telemetry panic hook.
            ITERATION_PANICS.fetch_add(1, Ordering::SeqCst);
            error!("Clipboard watcher iteration panicked — continuing");
            telemetry::record("watcher_iteration_panicked", json!({ "generation": generation }));
            thread::sleep(Duration::from_secs(1));
        }

        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
            last_heartbeat = Instant::now();
            telemetry::record("watcher_heartbeat", status());
        }

        thread::sleep(POLL_INTERVAL);
    }

    info!("Clipboard watcher #{} exiting (superseded)", generation);
    telemetry::record("watcher_superseded", json!({ "generation": generation }));
}

/// Which formats the clipboard currently offers.
#[derive(Default)]
struct Offered {
    text: bool,
    image: bool,
}

/// `None` means we could not tell (tool missing / timed out); then both readers are tried.
fn offered_formats(state: &mut LoopState) -> Option<Offered> {
    let result = match state.display {
        DisplayServer::X11 => run_with_timeout(
            Command::new("xclip").args(["-selection", "clipboard", "-t", "TARGETS", "-o"]),
            READ_TIMEOUT,
        ),
        DisplayServer::Wayland => run_with_timeout(Command::new("wl-paste").arg("--list-types"), READ_TIMEOUT),
    };
    match result {
        Ok(out) if out.status.success() => {
            state.targets_errors.ok();
            let types = String::from_utf8_lossy(&out.stdout);
            let mut offered = Offered::default();
            for t in types.lines().map(str::trim) {
                if t == "image/png" {
                    offered.image = true;
                } else if t == "UTF8_STRING" || t == "STRING" || t == "TEXT" || t.starts_with("text/plain") {
                    offered.text = true;
                }
            }
            Some(offered)
        }
        // Non-zero exit = empty clipboard / no owner; that's normal.
        Ok(_) => {
            state.targets_errors.ok();
            Some(Offered::default())
        }
        Err(e) => {
            state.targets_errors.fail(e.to_string());
            None
        }
    }
}

fn poll_once(state: &mut LoopState, db_path: &str, app_handle: &AppHandle) {
    match offered_formats(state) {
        // Prefer the image when both are offered (copying an image in a browser
        // also offers its URL as text).
        Some(o) if o.image => poll_image(state, db_path, app_handle),
        Some(o) if o.text => poll_text(state, db_path, app_handle),
        Some(_) => {}
        None => {
            poll_text(state, db_path, app_handle);
            poll_image(state, db_path, app_handle);
        }
    }
}

fn poll_text(state: &mut LoopState, db_path: &str, app_handle: &AppHandle) {
    let text = match read_clipboard_text(state.display) {
        Ok(t) => {
            state.text_errors.ok();
            t
        }
        Err(e) => {
            state.text_errors.fail(e.to_string());
            return;
        }
    };
    if text.is_empty() {
        return;
    }

    let hash = compute_hash(text.as_bytes());
    if state.last_text_hash == hash {
        return;
    }
    // Mark as seen *before* processing, so even if something below fails the
    // watcher moves on instead of retrying the same value forever.
    state.last_text_hash = hash.clone();

    debug!("New clipboard text captured ({} bytes)", text.len());

    // Must cut on a char boundary: the old `&text[..200]` panicked on emoji /
    // non-ASCII text, killing the watcher thread so history stopped recording.
    let preview = Some(truncate_at_char_boundary(&text, PREVIEW_BYTES).to_string());
    let details = json!({
        "type": "text",
        "bytes": text.len(),
        "chars": text.chars().count(),
        "lines": text.lines().count(),
        "asciiOnly": text.is_ascii(),
        "hash": short_hash(&hash),
    });

    let item = NewClipboardItem {
        content_type: "text".to_string(),
        text_content: Some(text),
        image_path: None,
        preview: preview.clone(),
        hash,
    };

    if save_item(db_path, &item, details) {
        notification::send_copy_notification(app_handle, "text", preview.as_deref());
    }
}

fn poll_image(state: &mut LoopState, db_path: &str, app_handle: &AppHandle) {
    let image_bytes = match read_clipboard_image(state.display) {
        Ok(b) => {
            state.image_errors.ok();
            b
        }
        Err(e) => {
            let msg = e.to_string();
            if msg != NO_IMAGE {
                state.image_errors.fail(msg);
            }
            return;
        }
    };

    let hash = compute_hash(&image_bytes);
    if state.last_image_hash == hash {
        return;
    }
    state.last_image_hash = hash.clone();

    debug!("New clipboard image captured ({} bytes)", image_bytes.len());

    let image_path = match save_image_bytes(&hash, &image_bytes) {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to save image to disk: {}", e);
            SAVE_FAILURES.fetch_add(1, Ordering::SeqCst);
            telemetry::record(
                "capture_save_failed",
                json!({ "type": "image", "stage": "write_file", "bytes": image_bytes.len(), "error": e.to_string() }),
            );
            return;
        }
    };

    let item = NewClipboardItem {
        content_type: "image".to_string(),
        text_content: None,
        image_path: Some(image_path),
        preview: None,
        hash: hash.clone(),
    };

    let details = json!({ "type": "image", "bytes": image_bytes.len(), "hash": short_hash(&hash) });
    if save_item(db_path, &item, details) {
        notification::send_copy_notification(app_handle, "image", None);
    }
}

/// Insert into the DB and record the outcome. Returns true on success.
fn save_item(db_path: &str, item: &NewClipboardItem, mut details: serde_json::Value) -> bool {
    let limit = settings::load_settings().clipboard_limit;
    match db::queries::insert_item(db_path, item, limit) {
        Ok(outcome) => {
            CAPTURES.fetch_add(1, Ordering::SeqCst);
            LAST_CAPTURE.store(now_secs(), Ordering::SeqCst);
            details["duplicate"] = json!(outcome.was_duplicate);
            details["prunedItems"] = json!(outcome.pruned_rows);
            details["prunedImageFiles"] = json!(outcome.pruned_files);
            telemetry::record("clipboard_captured", details);
            true
        }
        Err(e) => {
            error!("Failed to save clipboard item: {}", e);
            SAVE_FAILURES.fetch_add(1, Ordering::SeqCst);
            details["stage"] = json!("database");
            details["error"] = json!(e.to_string());
            telemetry::record("capture_save_failed", details);
            false
        }
    }
}

/// Read text from clipboard (supports X11 via xclip and Wayland via wl-paste)
fn read_clipboard_text(display: DisplayServer) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let output = match display {
        DisplayServer::X11 => run_with_timeout(
            Command::new("xclip").args(["-selection", "clipboard", "-o"]),
            READ_TIMEOUT,
        )?,
        DisplayServer::Wayland => run_with_timeout(
            Command::new("wl-paste").args(["--no-newline", "--type", "text/plain"]),
            READ_TIMEOUT,
        )?,
    };

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("text read exited with {}: {}", output.status, truncate_at_char_boundary(stderr.trim(), 200)).into())
    }
}

/// Read image from clipboard (PNG format, supports X11 and Wayland)
fn read_clipboard_image(display: DisplayServer) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let output = match display {
        DisplayServer::X11 => run_with_timeout(
            Command::new("xclip").args(["-selection", "clipboard", "-t", "image/png", "-o"]),
            READ_TIMEOUT,
        )?,
        DisplayServer::Wayland => run_with_timeout(
            Command::new("wl-paste").args(["--no-newline", "--type", "image/png"]),
            READ_TIMEOUT,
        )?,
    };

    if output.status.success() && !output.stdout.is_empty() {
        Ok(output.stdout)
    } else {
        Err(NO_IMAGE.into())
    }
}

fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn save_image_bytes(hash: &str, png_bytes: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    let data_dir = db::images_dir();
    std::fs::create_dir_all(&data_dir)?;

    let file_path = data_dir.join(format!("{}.png", hash));
    let file_path_str = file_path.to_string_lossy().to_string();

    if file_path.exists() {
        return Ok(file_path_str);
    }

    std::fs::write(&file_path, png_bytes)?;

    info!("Saved clipboard image: {}", file_path_str);
    Ok(file_path_str)
}
