use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::clipboard::types::{ClipboardItem, NewClipboardItem};
use crate::db::{images_dir, open};

/// Image files younger than this are never treated as orphans: the watcher
/// writes the file a moment before it inserts the database row.
const ORPHAN_GRACE: Duration = Duration::from_secs(120);

pub struct InsertOutcome {
    pub was_duplicate: bool,
    pub pruned_rows: usize,
    pub pruned_files: usize,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    /// History rows removed (expired images / cleared items).
    pub removed_items: usize,
    /// Image files deleted from disk, including orphans.
    pub removed_files: usize,
    /// Image files that were on disk but referenced by no history item.
    pub orphan_files: usize,
    pub freed_bytes: u64,
    pub errors: Vec<String>,
}

/// Delete image files, ignoring ones that are already gone.
fn remove_files(paths: &[String], report: &mut CleanupReport) {
    for path in paths {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(path) {
            Ok(()) => {
                report.removed_files += 1;
                report.freed_bytes += size;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => report.errors.push(format!("{}: {}", path, e)),
        }
    }
}

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<ClipboardItem> {
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
}

pub fn insert_item(db_path: &str, item: &NewClipboardItem, limit: u32) -> Result<InsertOutcome, rusqlite::Error> {
    let mut conn = open(db_path)?;
    let tx = conn.transaction()?;

    // If duplicate hash exists, update the timestamp (bump to top)
    let updated = tx.execute(
        "UPDATE clipboard_items SET created_at = datetime('now', 'localtime') WHERE hash = ?1",
        params![item.hash],
    )?;

    if updated == 0 {
        tx.execute(
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

    // Auto-prune: keep only the most recent `limit` non-pinned items.
    // Collect image paths first — previously pruned rows left their image files
    // behind forever, which is what filled up ~/.local/share/recopied/images.
    const PRUNE_SET: &str = "pinned = 0 AND id NOT IN (
        SELECT id FROM clipboard_items WHERE pinned = 0 ORDER BY created_at DESC, id DESC LIMIT ?1
    )";
    let pruned_paths: Vec<String> = {
        let mut stmt = tx.prepare(&format!(
            "SELECT image_path FROM clipboard_items WHERE image_path IS NOT NULL AND {}",
            PRUNE_SET
        ))?;
        let rows = stmt.query_map(params![limit], |row| row.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let pruned_rows = tx.execute(&format!("DELETE FROM clipboard_items WHERE {}", PRUNE_SET), params![limit])?;
    tx.commit()?;

    // Only delete files after the rows are gone for good.
    let mut report = CleanupReport::default();
    remove_files(&pruned_paths, &mut report);

    Ok(InsertOutcome {
        was_duplicate: updated > 0,
        pruned_rows,
        pruned_files: report.removed_files,
    })
}

pub fn get_history(db_path: &str, limit: u32, offset: u32) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, content_type, text_content, image_path, preview, pinned, created_at, hash
         FROM clipboard_items
         ORDER BY pinned DESC, created_at DESC
         LIMIT ?1 OFFSET ?2"
    )?;

    let items = stmt.query_map(params![limit, offset], row_to_item)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(items)
}

pub fn get_item(db_path: &str, id: i64) -> Result<Option<ClipboardItem>, rusqlite::Error> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, content_type, text_content, image_path, preview, pinned, created_at, hash
         FROM clipboard_items WHERE id = ?1"
    )?;
    let mut rows = stmt.query_map(params![id], row_to_item)?;
    rows.next().transpose()
}

pub fn search_history(db_path: &str, query: &str) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
    let conn = open(db_path)?;
    let search_pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, content_type, text_content, image_path, preview, pinned, created_at, hash
         FROM clipboard_items
         WHERE content_type = 'text' AND text_content LIKE ?1
         ORDER BY pinned DESC, created_at DESC
         LIMIT 50"
    )?;

    let items = stmt.query_map(params![search_pattern], row_to_item)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(items)
}

/// Toggle pin. Returns the new pinned state, or None if the item doesn't exist.
pub fn pin_item(db_path: &str, id: i64) -> Result<Option<bool>, rusqlite::Error> {
    let conn = open(db_path)?;
    let rows = conn.execute(
        "UPDATE clipboard_items SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END WHERE id = ?1",
        params![id],
    )?;
    if rows == 0 {
        return Ok(None);
    }
    let pinned: i32 = conn.query_row("SELECT pinned FROM clipboard_items WHERE id = ?1", params![id], |r| r.get(0))?;
    Ok(Some(pinned != 0))
}

pub fn delete_item(db_path: &str, id: i64) -> Result<(bool, CleanupReport), rusqlite::Error> {
    let conn = open(db_path)?;

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

    let mut report = CleanupReport { removed_items: rows, ..Default::default() };
    if rows > 0 {
        if let Some(path) = image_path {
            remove_files(&[path], &mut report);
        }
    }

    Ok((rows > 0, report))
}

/// "Clear all": remove every non-pinned item, its image file, and any image
/// files left behind by older versions that no history item references.
pub fn clear_history(db_path: &str) -> Result<CleanupReport, rusqlite::Error> {
    clear_history_in(db_path, &images_dir())
}

fn clear_history_in(db_path: &str, dir: &Path) -> Result<CleanupReport, rusqlite::Error> {
    let conn = open(db_path)?;

    // Get all non-pinned image paths for cleanup
    let paths: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items WHERE pinned = 0 AND image_path IS NOT NULL"
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let rows = conn.execute("DELETE FROM clipboard_items WHERE pinned = 0", [])?;

    let mut report = CleanupReport { removed_items: rows, ..Default::default() };
    remove_files(&paths, &mut report);
    sweep_orphans(&conn, dir, &mut report)?;
    Ok(report)
}

/// Delete non-pinned image items older than `retention_days` (and their files),
/// then delete orphaned image files. Pinned images are always kept.
pub fn cleanup_images(db_path: &str, retention_days: u32) -> Result<CleanupReport, rusqlite::Error> {
    cleanup_images_in(db_path, &images_dir(), retention_days)
}

fn cleanup_images_in(db_path: &str, dir: &Path, retention_days: u32) -> Result<CleanupReport, rusqlite::Error> {
    let conn = open(db_path)?;
    let mut report = CleanupReport::default();

    if retention_days > 0 {
        let cutoff = format!("-{} days", retention_days);
        let paths: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT image_path FROM clipboard_items
                 WHERE content_type = 'image' AND pinned = 0 AND image_path IS NOT NULL
                   AND created_at < datetime('now', 'localtime', ?1)",
            )?;
            let rows = stmt.query_map(params![cutoff], |row| row.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        report.removed_items = conn.execute(
            "DELETE FROM clipboard_items
             WHERE content_type = 'image' AND pinned = 0
               AND created_at < datetime('now', 'localtime', ?1)",
            params![cutoff],
        )?;
        remove_files(&paths, &mut report);
    }

    sweep_orphans(&conn, dir, &mut report)?;
    Ok(report)
}

/// Remove `*.png` files in `dir` that no history row references.
fn sweep_orphans(conn: &Connection, dir: &Path, report: &mut CleanupReport) -> Result<(), rusqlite::Error> {
    let referenced = referenced_paths(conn)?;
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    let now = SystemTime::now();

    let mut orphans = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        if referenced.contains(&path) {
            continue;
        }
        let young = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(|age| age < ORPHAN_GRACE)
            .unwrap_or(true);
        if !young {
            orphans.push(path.to_string_lossy().to_string());
        }
    }

    report.orphan_files += orphans.len();
    remove_files(&orphans, report);
    Ok(())
}

fn referenced_paths(conn: &Connection) -> Result<HashSet<PathBuf>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT image_path FROM clipboard_items WHERE image_path IS NOT NULL")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.filter_map(|r| r.ok()).map(PathBuf::from).collect())
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub text_items: i64,
    pub image_items: i64,
    pub pinned_items: i64,
    pub oldest_item: Option<String>,
    pub newest_item: Option<String>,
    pub image_files: u64,
    pub image_bytes: u64,
    pub orphan_files: u64,
    pub orphan_bytes: u64,
    pub missing_image_files: u64,
    pub db_bytes: u64,
}

pub fn storage_stats(db_path: &str) -> Result<StorageStats, rusqlite::Error> {
    let conn = open(db_path)?;
    let mut s = StorageStats::default();
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0));
    s.text_items = count("SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'text'")?;
    s.image_items = count("SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'image'")?;
    s.pinned_items = count("SELECT COUNT(*) FROM clipboard_items WHERE pinned = 1")?;
    (s.oldest_item, s.newest_item) = conn.query_row(
        "SELECT MIN(created_at), MAX(created_at) FROM clipboard_items",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let referenced = referenced_paths(&conn)?;
    s.missing_image_files = referenced.iter().filter(|p| !p.exists()).count() as u64;
    if let Ok(entries) = std::fs::read_dir(images_dir()) {
        for entry in entries.flatten() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            s.image_files += 1;
            s.image_bytes += size;
            if !referenced.contains(&entry.path()) {
                s.orphan_files += 1;
                s.orphan_bytes += size;
            }
        }
    }
    s.db_bytes = ["", "-wal", "-shm"]
        .iter()
        .filter_map(|suffix| std::fs::metadata(format!("{}{}", db_path, suffix)).ok())
        .map(|m| m.len())
        .sum();
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("recopied-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn image(db: &str, dir: &Path, name: &str) -> String {
        let path = dir.join(format!("{}.png", name)).to_string_lossy().to_string();
        std::fs::write(&path, b"png").unwrap();
        let item = NewClipboardItem {
            content_type: "image".into(),
            text_content: None,
            image_path: Some(path.clone()),
            preview: None,
            hash: name.into(),
        };
        insert_item(db, &item, 1000).unwrap();
        path
    }

    #[test]
    fn pruning_by_limit_deletes_image_files() {
        let dir = temp_dir("prune");
        let db = dir.join("t.db").to_string_lossy().to_string();
        init_db(&db).unwrap();
        let old = image(&db, &dir, "old");
        let conn = open(&db).unwrap();
        conn.execute("UPDATE clipboard_items SET created_at = datetime('now','localtime','-1 hour')", []).unwrap();

        let item = NewClipboardItem {
            content_type: "text".into(),
            text_content: Some("hi".into()),
            image_path: None,
            preview: Some("hi".into()),
            hash: "t1".into(),
        };
        let out = insert_item(&db, &item, 1).unwrap();
        assert_eq!(out.pruned_rows, 1);
        assert_eq!(out.pruned_files, 1);
        assert!(!Path::new(&old).exists());
    }

    #[test]
    fn expiry_removes_old_unpinned_images_only() {
        let dir = temp_dir("expire");
        let db = dir.join("t.db").to_string_lossy().to_string();
        init_db(&db).unwrap();
        let old = image(&db, &dir, "old");
        let old_pinned = image(&db, &dir, "oldpinned");
        let fresh = image(&db, &dir, "fresh");
        let conn = open(&db).unwrap();
        conn.execute(
            "UPDATE clipboard_items SET created_at = datetime('now','localtime','-31 days') WHERE hash IN ('old','oldpinned')",
            [],
        )
        .unwrap();
        conn.execute("UPDATE clipboard_items SET pinned = 1 WHERE hash = 'oldpinned'", []).unwrap();

        let r = cleanup_images_in(&db, &dir, 30).unwrap();
        assert_eq!(r.removed_items, 1);
        assert!(!Path::new(&old).exists());
        assert!(Path::new(&old_pinned).exists());
        assert!(Path::new(&fresh).exists());
    }

    #[test]
    fn orphan_sweep_respects_grace_period_and_references() {
        let dir = temp_dir("orphan");
        let db = dir.join("t.db").to_string_lossy().to_string();
        init_db(&db).unwrap();
        let kept = image(&db, &dir, "kept");
        let orphan = dir.join("orphan.png");
        std::fs::write(&orphan, b"png").unwrap();
        let conn = open(&db).unwrap();

        // Brand-new orphan is within the grace period → kept.
        let mut report = CleanupReport::default();
        sweep_orphans(&conn, &dir, &mut report).unwrap();
        assert!(orphan.exists());

        // Age it past the grace period → deleted; referenced file stays.
        let old = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&orphan).unwrap().set_modified(old).unwrap();
        std::fs::File::options().write(true).open(&kept).unwrap().set_modified(old).unwrap();
        let mut report = CleanupReport::default();
        sweep_orphans(&conn, &dir, &mut report).unwrap();
        assert!(!orphan.exists());
        assert!(Path::new(&kept).exists());
        assert_eq!(report.orphan_files, 1);
    }

    #[test]
    fn clear_all_deletes_image_files_and_keeps_pinned() {
        let dir = temp_dir("clear");
        let db = dir.join("t.db").to_string_lossy().to_string();
        init_db(&db).unwrap();
        let a = image(&db, &dir, "a");
        let pinned = image(&db, &dir, "pinned");
        open(&db).unwrap().execute("UPDATE clipboard_items SET pinned = 1 WHERE hash = 'pinned'", []).unwrap();
        let leftover = dir.join("leftover.png");
        std::fs::write(&leftover, b"png").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&leftover).unwrap().set_modified(old).unwrap();

        let r = clear_history_in(&db, &dir).unwrap();
        assert_eq!(r.removed_items, 1);
        assert_eq!(r.removed_files, 2); // a.png + leftover.png
        assert!(!Path::new(&a).exists());
        assert!(!leftover.exists());
        assert!(Path::new(&pinned).exists());
    }
}
