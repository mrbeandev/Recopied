pub mod queries;

use rusqlite::Connection;
use log::info;
use std::path::PathBuf;

const MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS clipboard_items (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    content_type TEXT    NOT NULL CHECK(content_type IN ('text', 'image')),
    text_content TEXT,
    image_path   TEXT,
    preview      TEXT,
    pinned       INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    hash         TEXT    NOT NULL UNIQUE
);

CREATE INDEX IF NOT EXISTS idx_created_at ON clipboard_items(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_pinned ON clipboard_items(pinned);
"#;

/// Open a connection with a busy timeout, so a write from the watcher thread
/// and a write from a UI command wait for each other instead of failing
/// immediately with "database is locked".
pub fn open(db_path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

pub fn init_db(db_path: &str) -> Result<(), rusqlite::Error> {
    let conn = open(db_path)?;
    // WAL mode: allows concurrent reads + one write without blocking — prevents
    // the watcher thread and IPC handlers from locking each other out.
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    conn.execute_batch(MIGRATION)?;
    info!("Database initialized at {}", db_path);
    Ok(())
}

pub fn data_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recopied");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn get_db_path() -> String {
    data_dir().join("recopied.db").to_string_lossy().to_string()
}

/// Where copied images are stored as `<sha256>.png`.
pub fn images_dir() -> PathBuf {
    data_dir().join("images")
}
