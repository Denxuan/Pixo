use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use super::error::{ClipboardError, log_clipboard};
use super::model::{ClipKind, Detail, ListPage, Settings, Summary};
use super::util::kind_preview;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS clip_items (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('text', 'image', 'files')),
  content_hash TEXT NOT NULL UNIQUE,
  text TEXT,
  files_json TEXT,
  image_width INTEGER,
  image_height INTEGER,
  size_bytes INTEGER NOT NULL,
  source_app TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  last_copied_at INTEGER NOT NULL,
  copy_count INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_clip_items_order ON clip_items (pinned DESC, last_copied_at DESC, id DESC);
CREATE TABLE IF NOT EXISTS clip_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

const SELECT_COLUMNS: &str = "id, kind, content_hash, text, files_json, image_width, image_height, size_bytes, source_app, pinned, created_at, last_copied_at, copy_count";

#[derive(Debug, Clone)]
struct ItemRow {
    id: i64,
    kind: String,
    content_hash: String,
    text: Option<String>,
    files_json: Option<String>,
    image_width: Option<i64>,
    image_height: Option<i64>,
    size_bytes: i64,
    source_app: Option<String>,
    pinned: i64,
    created_at: i64,
    last_copied_at: i64,
    copy_count: i64,
}

#[derive(Debug)]
pub struct UpsertRequest<'a> {
    pub kind: ClipKind,
    pub hash: &'a str,
    pub text: Option<String>,
    pub files_json: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub size_bytes: i64,
    pub source_app: Option<String>,
    pub now: i64,
    pub self_write: bool,
}

#[derive(Debug)]
pub enum UpsertResult {
    Added(Summary),
    Updated { summary: Summary, incremented: bool },
    Ignored,
}

#[derive(Debug, Default)]
pub struct Removal {
    pub count: u64,
    pub image_hashes: Vec<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, ClipboardError> {
        let conn = Connection::open(path).map_err(storage)?;
        conn.busy_timeout(std::time::Duration::from_millis(2_000))
            .map_err(storage)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .map_err(storage)?;
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(storage)?;
        if version < 1 {
            conn.execute_batch(SCHEMA).map_err(storage)?;
            conn.pragma_update(None, "user_version", 1).map_err(storage)?;
        }
        Ok(Self { conn })
    }

    pub fn load_settings(&self) -> Result<Settings, ClipboardError> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM clip_settings WHERE key = 'settings'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        match raw {
            Some(raw) => match serde_json::from_str::<Settings>(&raw) {
                Ok(settings) => Ok(settings.normalized()),
                Err(error) => {
                    log_clipboard("storage", "settings", 0);
                    eprintln!("clipboard settings reset: {error}");
                    Ok(Settings::default())
                }
            },
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&mut self, settings: &Settings) -> Result<(), ClipboardError> {
        let value = serde_json::to_string(settings).map_err(|error| {
            log_clipboard("storage", "settings", 0);
            eprintln!("clipboard settings encode: {error}");
            ClipboardError::new("storage")
        })?;
        self.conn
            .execute(
                "INSERT INTO clip_settings (key, value) VALUES ('settings', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![value],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub fn upsert(&mut self, request: UpsertRequest<'_>) -> Result<UpsertResult, ClipboardError> {
        let transaction = self.conn.transaction().map_err(storage)?;
        let existing: Option<i64> = transaction
            .query_row(
                "SELECT id FROM clip_items WHERE content_hash = ?1",
                params![request.hash],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        if let Some(id) = existing {
            if request.self_write {
                transaction
                    .execute(
                        "UPDATE clip_items SET last_copied_at = ?1 WHERE id = ?2",
                        params![request.now, id],
                    )
                    .map_err(storage)?;
            } else {
                transaction
                    .execute(
                        "UPDATE clip_items SET last_copied_at = ?1, copy_count = copy_count + 1, source_app = ?2 WHERE id = ?3",
                        params![request.now, request.source_app, id],
                    )
                    .map_err(storage)?;
            }
            let summary = load_summary(&transaction, id)?;
            transaction.commit().map_err(storage)?;
            return Ok(UpsertResult::Updated {
                summary,
                incremented: !request.self_write,
            });
        }
        if request.self_write {
            transaction.commit().map_err(storage)?;
            return Ok(UpsertResult::Ignored);
        }
        transaction
            .execute(
                "INSERT INTO clip_items (
                    kind, content_hash, text, files_json, image_width, image_height,
                    size_bytes, source_app, pinned, created_at, last_copied_at, copy_count
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?9, 1)",
                params![
                    request.kind.as_str(),
                    request.hash,
                    request.text,
                    request.files_json,
                    request.image_width,
                    request.image_height,
                    request.size_bytes,
                    request.source_app,
                    request.now,
                ],
            )
            .map_err(storage)?;
        let id = transaction.last_insert_rowid();
        let summary = load_summary(&transaction, id)?;
        transaction.commit().map_err(storage)?;
        Ok(UpsertResult::Added(summary))
    }

    pub fn list(
        &self,
        query: Option<&str>,
        kind: Option<ClipKind>,
        pinned_only: bool,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ListPage, ClipboardError> {
        let (cursor_pinned, cursor_time, cursor_id) = match cursor {
            Some(cursor) => {
                let parsed = parse_cursor(cursor).ok_or_else(|| ClipboardError::new("invalid_input"))?;
                (Some(parsed.0), Some(parsed.1), Some(parsed.2))
            }
            None => (None, None, None),
        };
        let pattern = query
            .filter(|query| !query.trim().is_empty())
            .map(like_owned);
        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM clip_items
                 WHERE (?1 IS NULL OR kind = ?1)
                   AND (?2 = 0 OR pinned = 1)
                   AND (?3 IS NULL OR IFNULL(text, '') LIKE ?3 ESCAPE '\\' OR IFNULL(files_json, '') LIKE ?3 ESCAPE '\\')
                   AND (
                     ?4 IS NULL
                     OR pinned < ?4
                     OR (pinned = ?4 AND last_copied_at < ?5)
                     OR (pinned = ?4 AND last_copied_at = ?5 AND id < ?6)
                   )
                 ORDER BY pinned DESC, last_copied_at DESC, id DESC
                 LIMIT ?7"
            ))
            .map_err(storage)?;
        let rows = statement
            .query_map(
                params![
                    kind.map(|kind| kind.as_str().to_string()),
                    i64::from(pinned_only),
                    pattern,
                    cursor_pinned,
                    cursor_time,
                    cursor_id,
                    i64::from(limit) + 1,
                ],
                map_row,
            )
            .map_err(storage)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(storage)?.summary());
        }
        let next_cursor = if items.len() > limit as usize {
            items.pop();
            items.last().map(cursor_of)
        } else {
            None
        };
        Ok(ListPage { items, next_cursor })
    }

    pub fn get(&self, id: i64) -> Result<Detail, ClipboardError> {
        load_detail(&self.conn, id)
    }

    pub fn image_hash(&self, id: i64) -> Result<String, ClipboardError> {
        let (kind, hash): (String, String) = self
            .conn
            .query_row(
                "SELECT kind, content_hash FROM clip_items WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage)?
            .ok_or_else(|| ClipboardError::new("not_found"))?;
        if kind != ClipKind::Image.as_str() {
            return Err(ClipboardError::new("not_found"));
        }
        Ok(hash)
    }

    pub fn set_pinned(&mut self, id: i64, pinned: bool) -> Result<(), ClipboardError> {
        let changed = self
            .conn
            .execute(
                "UPDATE clip_items SET pinned = ?1 WHERE id = ?2",
                params![i64::from(pinned), id],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(ClipboardError::new("not_found"));
        }
        Ok(())
    }

    pub fn delete_ids(&mut self, ids: &[i64]) -> Result<Removal, ClipboardError> {
        if ids.is_empty() || ids.len() > 500 {
            return Err(ClipboardError::new("invalid_input"));
        }
        let mut removal = Removal::default();
        let transaction = self.conn.transaction().map_err(storage)?;
        for id in ids {
            if let Some(hash) = image_hash_of(&transaction, *id)? {
                removal.image_hashes.push(hash);
            }
            removal.count += transaction
                .execute("DELETE FROM clip_items WHERE id = ?1", params![id])
                .map_err(storage)? as u64;
        }
        transaction.commit().map_err(storage)?;
        Ok(removal)
    }

    pub fn clear(&mut self, keep_pinned: bool) -> Result<Removal, ClipboardError> {
        let mut removal = Removal::default();
        let transaction = self.conn.transaction().map_err(storage)?;
        let mut statement = transaction
            .prepare("SELECT content_hash FROM clip_items WHERE kind = 'image' AND (?1 = 0 OR pinned = 0)")
            .map_err(storage)?;
        let hashes = statement
            .query_map(params![i64::from(keep_pinned)], |row| row.get(0))
            .map_err(storage)?;
        for hash in hashes {
            removal.image_hashes.push(hash.map_err(storage)?);
        }
        drop(statement);
        removal.count = transaction
            .execute(
                "DELETE FROM clip_items WHERE (?1 = 0 OR pinned = 0)",
                params![i64::from(keep_pinned)],
            )
            .map_err(storage)? as u64;
        transaction.commit().map_err(storage)?;
        Ok(removal)
    }

    pub fn prune_plan(&self, settings: &Settings, now: i64) -> Result<Removal, ClipboardError> {
        let mut removal = Removal::default();
        let mut seen = HashSet::new();
        if settings.retention_days > 0 {
            let cutoff = now - i64::from(settings.retention_days) * 24 * 60 * 60 * 1000;
            collect_matches(
                &self.conn,
                "SELECT id, kind, content_hash FROM clip_items WHERE pinned = 0 AND last_copied_at < ?1",
                params![cutoff],
                &mut seen,
                &mut removal,
            )?;
        }
        collect_matches(
            &self.conn,
            "SELECT id, kind, content_hash FROM clip_items
             WHERE pinned = 0
             ORDER BY last_copied_at DESC, id DESC
             LIMIT -1 OFFSET ?1",
            params![i64::from(settings.max_items)],
            &mut seen,
            &mut removal,
        )?;
        Ok(removal)
    }

    pub fn apply_prune(&mut self, settings: &Settings, now: i64) -> Result<Removal, ClipboardError> {
        let removal = self.prune_plan(settings, now)?;
        if removal.count == 0 {
            return Ok(removal);
        }
        let transaction = self.conn.transaction().map_err(storage)?;
        if settings.retention_days > 0 {
            let cutoff = now - i64::from(settings.retention_days) * 24 * 60 * 60 * 1000;
            transaction
                .execute(
                    "DELETE FROM clip_items WHERE pinned = 0 AND last_copied_at < ?1",
                    params![cutoff],
                )
                .map_err(storage)?;
        }
        transaction
            .execute(
                "DELETE FROM clip_items WHERE id IN (
                    SELECT id FROM clip_items
                    WHERE pinned = 0
                    ORDER BY last_copied_at DESC, id DESC
                    LIMIT -1 OFFSET ?1
                 )",
                params![i64::from(settings.max_items)],
            )
            .map_err(storage)?;
        transaction.commit().map_err(storage)?;
        Ok(removal)
    }

    pub fn image_hashes(&self) -> Result<HashSet<String>, ClipboardError> {
        let mut statement = self
            .conn
            .prepare("SELECT content_hash FROM clip_items WHERE kind = 'image'")
            .map_err(storage)?;
        let rows = statement.query_map([], |row| row.get(0)).map_err(storage)?;
        let mut hashes = HashSet::new();
        for row in rows {
            hashes.insert(row.map_err(storage)?);
        }
        Ok(hashes)
    }
}

fn collect_matches(
    conn: &Connection,
    sql: &str,
    parameters: impl rusqlite::Params,
    seen: &mut HashSet<i64>,
    removal: &mut Removal,
) -> Result<(), ClipboardError> {
    let mut statement = conn.prepare(sql).map_err(storage)?;
    let rows = statement
        .query_map(parameters, |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(storage)?;
    for row in rows {
        let (id, kind, hash) = row.map_err(storage)?;
        if seen.insert(id) {
            removal.count += 1;
            if kind == "image" {
                removal.image_hashes.push(hash);
            }
        }
    }
    Ok(())
}

fn image_hash_of(conn: &Connection, id: i64) -> Result<Option<String>, ClipboardError> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT kind, content_hash FROM clip_items WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(storage)?;
    Ok(row.and_then(|(kind, hash)| (kind == "image").then_some(hash)))
}

fn load_summary(conn: &Connection, id: i64) -> Result<Summary, ClipboardError> {
    let row = conn
        .query_row(
            &format!("SELECT {SELECT_COLUMNS} FROM clip_items WHERE id = ?1"),
            params![id],
            map_row,
        )
        .optional()
        .map_err(storage)?
        .ok_or_else(|| ClipboardError::new("not_found"))?;
    Ok(row.summary())
}

fn load_detail(conn: &Connection, id: i64) -> Result<Detail, ClipboardError> {
    let row = conn
        .query_row(
            &format!("SELECT {SELECT_COLUMNS} FROM clip_items WHERE id = ?1"),
            params![id],
            map_row,
        )
        .optional()
        .map_err(storage)?
        .ok_or_else(|| ClipboardError::new("not_found"))?;
    Ok(row.detail())
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRow> {
    Ok(ItemRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        content_hash: row.get(2)?,
        text: row.get(3)?,
        files_json: row.get(4)?,
        image_width: row.get(5)?,
        image_height: row.get(6)?,
        size_bytes: row.get(7)?,
        source_app: row.get(8)?,
        pinned: row.get(9)?,
        created_at: row.get(10)?,
        last_copied_at: row.get(11)?,
        copy_count: row.get(12)?,
    })
}

impl ItemRow {
    fn files(&self) -> Vec<String> {
        self.files_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default()
    }

    fn summary(&self) -> Summary {
        let kind = ClipKind::parse(&self.kind).unwrap_or(ClipKind::Text);
        let files = self.files();
        let (preview, file_names) = kind_preview(kind, self.text.as_deref(), &files);
        Summary {
            id: self.id,
            kind: self.kind.clone(),
            preview,
            file_names,
            image_width: self.image_width.map(|value| value as u32),
            image_height: self.image_height.map(|value| value as u32),
            size_bytes: self.size_bytes,
            source_app: self.source_app.clone(),
            pinned: self.pinned != 0,
            created_at: self.created_at,
            last_copied_at: self.last_copied_at,
            copy_count: self.copy_count,
        }
    }

    fn detail(&self) -> Detail {
        Detail {
            id: self.id,
            kind: self.kind.clone(),
            text: self.text.clone(),
            files: self.files(),
            image_width: self.image_width.map(|value| value as u32),
            image_height: self.image_height.map(|value| value as u32),
            size_bytes: self.size_bytes,
            source_app: self.source_app.clone(),
            pinned: self.pinned != 0,
            created_at: self.created_at,
            last_copied_at: self.last_copied_at,
            copy_count: self.copy_count,
        }
    }
}

fn cursor_of(summary: &Summary) -> String {
    format!(
        "{}:{}:{}",
        i32::from(summary.pinned),
        summary.last_copied_at,
        summary.id
    )
}

fn parse_cursor(cursor: &str) -> Option<(i64, i64, i64)> {
    let mut parts = cursor.split(':');
    let pinned = parts.next()?.parse::<i64>().ok()?;
    let time = parts.next()?.parse::<i64>().ok()?;
    let id = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || (pinned != 0 && pinned != 1) {
        return None;
    }
    Some((pinned, time, id))
}

fn like_owned(query: &str) -> String {
    super::util::like_pattern(query)
}

fn storage(error: impl std::fmt::Display) -> ClipboardError {
    log_clipboard("storage", "sqlite", 0);
    eprintln!("clipboard storage: {error}");
    ClipboardError::new("storage")
}

pub fn image_paths(directory: &Path, hash: &str) -> (PathBuf, PathBuf) {
    (
        directory.join(format!("{hash}.png")),
        directory.join(format!("{hash}.thumb.png")),
    )
}

pub fn write_image_files(directory: &Path, hash: &str, png: &[u8], thumb: &[u8]) -> Result<(), ClipboardError> {
    std::fs::create_dir_all(directory).map_err(storage)?;
    write_atomic(&directory.join(format!("{hash}.png.tmp")), &directory.join(format!("{hash}.png")), png)?;
    write_atomic(
        &directory.join(format!("{hash}.thumb.png.tmp")),
        &directory.join(format!("{hash}.thumb.png")),
        thumb,
    )?;
    Ok(())
}

fn write_atomic(temporary: &Path, target: &Path, bytes: &[u8]) -> Result<(), ClipboardError> {
    std::fs::write(temporary, bytes).map_err(storage)?;
    std::fs::rename(temporary, target).map_err(storage)?;
    Ok(())
}

pub fn files_json(files: &[String]) -> Result<String, ClipboardError> {
    serde_json::to_string(files).map_err(|error| {
        eprintln!("clipboard files encode: {error}");
        ClipboardError::new("storage")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::model::ClipKind;

    fn test_store() -> (Store, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "pixo-clipboard-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let store = Store::open(&directory.join("history.db")).unwrap();
        (store, directory)
    }

    fn text_request<'a>(hash: &'a str, text: &str, now: i64) -> UpsertRequest<'a> {
        UpsertRequest {
            kind: ClipKind::Text,
            hash,
            text: Some(text.to_string()),
            files_json: None,
            image_width: None,
            image_height: None,
            size_bytes: text.len() as i64,
            source_app: Some("com.example.editor".into()),
            now,
            self_write: false,
        }
    }

    #[test]
    fn dedup_updates_count_and_self_write_does_not() {
        let (mut store, directory) = test_store();
        let hash = "hash-a";
        let added = store.upsert(text_request(hash, "alpha", 1_000)).unwrap();
        let UpsertResult::Added(summary) = added else { panic!("expected insert"); };
        let updated = store.upsert(text_request(hash, "alpha", 2_000)).unwrap();
        let UpsertResult::Updated { summary: updated, incremented } = updated else { panic!("expected update"); };
        assert!(incremented);
        assert_eq!(updated.id, summary.id);
        assert_eq!(updated.copy_count, 2);
        let mut request = text_request(hash, "alpha", 3_000);
        request.self_write = true;
        let touched = store.upsert(request).unwrap();
        let UpsertResult::Updated { summary: touched, incremented } = touched else { panic!("expected touch"); };
        assert!(!incremented);
        assert_eq!(touched.copy_count, 2);
        assert_eq!(touched.last_copied_at, 3_000);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn prune_keeps_pinned_items() {
        let (mut store, directory) = test_store();
        store.upsert(text_request("old", "old", 1)).unwrap();
        store.upsert(text_request("mid", "mid", 2)).unwrap();
        store.upsert(text_request("new", "new", 3)).unwrap();
        let page = store.list(None, None, false, None, 10).unwrap();
        let oldest = page.items.iter().find(|item| item.preview == "old").unwrap();
        store.set_pinned(oldest.id, true).unwrap();
        let mut settings = Settings::default();
        settings.max_items = 1;
        settings.retention_days = 0;
        let removal = store.apply_prune(&settings, 10).unwrap();
        assert_eq!(removal.count, 1);
        let page = store.list(None, None, false, None, 10).unwrap();
        let previews: Vec<_> = page.items.iter().map(|item| item.preview.as_str()).collect();
        assert!(previews.contains(&"old"));
        assert!(previews.contains(&"new"));
        assert!(!previews.contains(&"mid"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn search_escapes_percent_and_pages_in_order() {
        let (mut store, directory) = test_store();
        store.upsert(text_request("one", "100%", 1)).unwrap();
        store.upsert(text_request("two", "plain", 2)).unwrap();
        store.upsert(text_request("three", "percent % value", 3)).unwrap();
        let matches = store.list(Some("%"), None, false, None, 10).unwrap();
        assert_eq!(matches.items.len(), 2);
        let page = store.list(None, None, false, None, 1).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].preview, "percent % value");
        let next = store.list(None, None, false, page.next_cursor.as_deref(), 1).unwrap();
        assert_eq!(next.items[0].preview, "plain");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn retention_does_not_delete_pinned_rows() {
        let (mut store, directory) = test_store();
        store.upsert(text_request("old", "old", 1)).unwrap();
        let id = store.list(None, None, false, None, 1).unwrap().items[0].id;
        store.set_pinned(id, true).unwrap();
        let mut settings = Settings::default();
        settings.retention_days = 1;
        let removal = store.prune_plan(&settings, 1 + 3 * 24 * 60 * 60 * 1000).unwrap();
        assert_eq!(removal.count, 0);
        let _ = std::fs::remove_dir_all(directory);
    }
}
