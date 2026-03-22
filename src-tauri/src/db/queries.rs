use rusqlite::{params, Connection};

use crate::clipboard::types::{ClipboardItem, NewClipboardItem};

pub fn insert_item(db_path: &str, item: &NewClipboardItem, limit: u32) -> Result<(), rusqlite::Error> {
    let conn = Connection::open(db_path)?;

    // If duplicate hash exists, update the timestamp (bump to top)
    let updated = conn.execute(
        "UPDATE clipboard_items SET created_at = datetime('now', 'localtime') WHERE hash = ?1",
        params![item.hash],
    )?;

    if updated == 0 {
        conn.execute(
            "INSERT INTO clipboard_items (content_type, text_content, image_path, preview, hash)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                item.content_type,
                item.text_content,
                item.image_path,
                item.preview,
                item.hash,
            ],
        )?;
    }

    // Auto-prune: keep only the most recent `limit` non-pinned items
    conn.execute(
        "DELETE FROM clipboard_items WHERE pinned = 0 AND id NOT IN (
            SELECT id FROM clipboard_items WHERE pinned = 0 ORDER BY created_at DESC LIMIT ?1
        )",
        params![limit],
    )?;

    Ok(())
}

pub fn get_history(db_path: &str, limit: u32, offset: u32) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, content_type, text_content, image_path, preview, pinned, created_at, hash
         FROM clipboard_items
         ORDER BY pinned DESC, created_at DESC
         LIMIT ?1 OFFSET ?2"
    )?;

    let items = stmt.query_map(params![limit, offset], |row| {
        Ok(ClipboardItem {
            id: row.get(0)?,
            content_type: row.get(1)?,
            text_content: row.get(2)?,
            image_path: row.get(3)?,
            preview: row.get(4)?,
            pinned: row.get::<_, i32>(5)? != 0,
            created_at: row.get(6)?,
            hash: row.get(7)?,
        })
    })?
    .filter_map(|r| r.ok())
    .collect();

    Ok(items)
}

pub fn search_history(db_path: &str, query: &str) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    let search_pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, content_type, text_content, image_path, preview, pinned, created_at, hash
         FROM clipboard_items
         WHERE content_type = 'text' AND text_content LIKE ?1
         ORDER BY pinned DESC, created_at DESC
         LIMIT 50"
    )?;

    let items = stmt.query_map(params![search_pattern], |row| {
        Ok(ClipboardItem {
            id: row.get(0)?,
            content_type: row.get(1)?,
            text_content: row.get(2)?,
            image_path: row.get(3)?,
            preview: row.get(4)?,
            pinned: row.get::<_, i32>(5)? != 0,
            created_at: row.get(6)?,
            hash: row.get(7)?,
        })
    })?
    .filter_map(|r| r.ok())
    .collect();

    Ok(items)
}

pub fn pin_item(db_path: &str, id: i64) -> Result<bool, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    let rows = conn.execute(
        "UPDATE clipboard_items SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END WHERE id = ?1",
        params![id],
    )?;
    Ok(rows > 0)
}

pub fn delete_item(db_path: &str, id: i64) -> Result<bool, rusqlite::Error> {
    let conn = Connection::open(db_path)?;

    // Get image path before deleting (to clean up file)
    let image_path: Option<String> = conn
        .query_row(
            "SELECT image_path FROM clipboard_items WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    let rows = conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;

    // Clean up image file if it exists
    if let Some(path) = image_path {
        std::fs::remove_file(&path).ok();
    }

    Ok(rows > 0)
}

pub fn clear_history(db_path: &str) -> Result<bool, rusqlite::Error> {
    let conn = Connection::open(db_path)?;

    // Get all non-pinned image paths for cleanup
    let mut stmt = conn.prepare(
        "SELECT image_path FROM clipboard_items WHERE pinned = 0 AND image_path IS NOT NULL"
    )?;
    let paths: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let rows = conn.execute("DELETE FROM clipboard_items WHERE pinned = 0", [])?;

    // Clean up image files
    for path in paths {
        std::fs::remove_file(&path).ok();
    }

    Ok(rows > 0)
}
