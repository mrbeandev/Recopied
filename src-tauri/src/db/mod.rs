pub mod queries;

use rusqlite::Connection;
use log::info;

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

pub fn init_db(db_path: &str) -> Result<(), rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    // WAL mode: allows concurrent reads + one write without blocking — prevents
    // the watcher thread and IPC handlers from locking each other out.
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    conn.execute_batch(MIGRATION)?;
    info!("Database initialized at {}", db_path);
    Ok(())
}

pub fn get_db_path() -> String {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("recopied");
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("recopied.db").to_string_lossy().to_string()
}
