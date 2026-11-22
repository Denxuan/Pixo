use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::error::ScreenshotError;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS shot_files (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('image', 'video')),
  path TEXT NOT NULL UNIQUE,
  bytes INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  text TEXT
);
CREATE TABLE IF NOT EXISTS shot_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub id: i64,
    pub kind: String,
    pub path: String,
    pub name: String,
    pub bytes: i64,
    pub created_at: i64,
    pub text: Option<String>,
}

pub struct Library {
    conn: Connection,
}

impl Library {
    pub fn open(path: &Path, save_dir: &Path) -> Result<Self, ScreenshotError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| ScreenshotError::new("storage"))?;
        }
        let conn = Connection::open(path).map_err(storage)?;
        conn.busy_timeout(std::time::Duration::from_millis(2_000))
            .map_err(storage)?;
        conn.execute_batch(SCHEMA).map_err(storage)?;
        let library = Self { conn };
        library.adopt_save_dir(save_dir)?;
        Ok(library)
    }

    pub fn remember(&self, path: &Path, text: Option<&str>) -> Result<(), ScreenshotError> {
        let kind = kind_of(path).ok_or_else(|| ScreenshotError::new("invalid_input"))?;
        let meta = fs::metadata(path).map_err(|_| ScreenshotError::new("storage"))?;
        if !meta.is_file() {
            return Err(ScreenshotError::new("invalid_input"));
        }
        let stored = clip_text(text);
        self.conn
            .execute(
                "INSERT INTO shot_files (kind, path, bytes, created_at, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path) DO UPDATE SET
                   kind = excluded.kind,
                   bytes = excluded.bytes,
                   created_at = excluded.created_at,
                   text = COALESCE(excluded.text, shot_files.text)",
                params![
                    kind,
                    path.to_string_lossy().to_string(),
                    i64::try_from(meta.len()).unwrap_or(i64::MAX),
                    now_millis(),
                    stored,
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub fn note_recent(&self, text: &str, within_ms: i64) -> Result<bool, ScreenshotError> {
        let Some(stored) = clip_text(Some(text)) else {
            return Ok(false);
        };
        let changed = self
            .conn
            .execute(
                "UPDATE shot_files SET text = ?1
                 WHERE id = (
                   SELECT id FROM shot_files
                   WHERE kind = 'image' AND created_at >= ?2
                   ORDER BY created_at DESC, id DESC
                   LIMIT 1
                 )",
                params![stored, now_millis().saturating_sub(within_ms)],
            )
            .map_err(storage)?;
        Ok(changed > 0)
    }

    pub fn list(&self, query: Option<&str>, limit: u32) -> Result<Vec<LibraryItem>, ScreenshotError> {
        let limit = limit.clamp(1, 200);
        let pattern = query
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(like_pattern);
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, kind, path, bytes, created_at, text FROM shot_files
                 WHERE (?1 IS NULL OR path LIKE ?1 ESCAPE '\\' OR IFNULL(text, '') LIKE ?1 ESCAPE '\\')
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?2",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(params![pattern, i64::from(limit)], map_item)
            .map_err(storage)?;
        let mut items = Vec::new();
        let mut missing = Vec::new();
        for row in rows {
            let item = row.map_err(storage)?;
            if Path::new(&item.path).is_file() {
                items.push(item);
            } else {
                missing.push(item.id);
            }
        }
        for id in missing {
            let _ = self.conn.execute("DELETE FROM shot_files WHERE id = ?1", params![id]);
        }
        Ok(items)
    }

    pub fn get(&self, id: i64) -> Result<LibraryItem, ScreenshotError> {
        self.conn
            .query_row(
                "SELECT id, kind, path, bytes, created_at, text FROM shot_files WHERE id = ?1",
                params![id],
                map_item,
            )
            .optional()
            .map_err(storage)?
            .filter(|item| Path::new(&item.path).is_file())
            .ok_or_else(|| ScreenshotError::new("not_found"))
    }

    pub fn delete(&self, id: i64, save_dir: &Path) -> Result<(), ScreenshotError> {
        let path: String = self
            .conn
            .query_row(
                "SELECT path FROM shot_files WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?
            .ok_or_else(|| ScreenshotError::new("not_found"))?;
        let file = PathBuf::from(&path);
        if file.is_file() && inside_dir(save_dir, &file) {
            fs::remove_file(&file).map_err(|_| ScreenshotError::new("storage"))?;
        }
        self.conn
            .execute("DELETE FROM shot_files WHERE id = ?1", params![id])
            .map_err(storage)?;
        Ok(())
    }

    fn adopt_save_dir(&self, save_dir: &Path) -> Result<(), ScreenshotError> {
        let adopted: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM shot_meta WHERE key = 'adopted'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        if adopted.is_some() {
            return Ok(());
        }
        if save_dir.is_dir() {
            let mut files = Vec::new();
            if let Ok(entries) = fs::read_dir(save_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if kind_of(&path).is_none() {
                        continue;
                    }
                    let Ok(meta) = entry.metadata() else { continue };
                    if !meta.is_file() {
                        continue;
                    }
                    let modified = meta.modified().ok().and_then(|time| {
                        time.duration_since(UNIX_EPOCH)
                            .ok()
                            .map(|duration| duration.as_millis() as i64)
                    });
                    files.push((modified.unwrap_or(0), path, meta.len()));
                }
            }
            files.sort_by(|left, right| right.0.cmp(&left.0));
            for (created, path, bytes) in files.into_iter().take(200) {
                let Some(kind) = kind_of(&path) else { continue };
                let _ = self.conn.execute(
                    "INSERT OR IGNORE INTO shot_files (kind, path, bytes, created_at, text) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        kind,
                        path.to_string_lossy().to_string(),
                        i64::try_from(bytes).unwrap_or(i64::MAX),
                        created,
                    ],
                );
            }
        }
        self.conn
            .execute(
                "INSERT INTO shot_meta (key, value) VALUES ('adopted', '1')",
                [],
            )
            .map_err(storage)?;
        Ok(())
    }
}

fn map_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    let path: String = row.get(2)?;
    let name = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let text: Option<String> = row.get(5)?;
    Ok(LibraryItem {
        id: row.get(0)?,
        kind: row.get(1)?,
        path,
        name,
        bytes: row.get(3)?,
        created_at: row.get(4)?,
        text: text.filter(|value| !value.trim().is_empty()),
    })
}

fn kind_of(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "webp") => Some("image"),
        Some("gif" | "mp4") => Some("video"),
        _ => None,
    }
}

fn clip_text(text: Option<&str>) -> Option<String> {
    let trimmed = text?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(8_000).collect())
}

fn like_pattern(query: &str) -> String {
    let mut escaped = String::new();
    for ch in query.chars().take(200) {
        if ch == '\\' || ch == '%' || ch == '_' {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    format!("%{escaped}%")
}

fn inside_dir(root: &Path, file: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(file) = file.canonicalize() else {
        return false;
    };
    file.starts_with(root)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn storage(error: impl std::fmt::Display) -> ScreenshotError {
    eprintln!("screenshot library: {error}");
    ScreenshotError::new("storage")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_lists_and_deletes_inside_the_save_dir() {
        let root = std::env::temp_dir().join(format!(
            "pixo-shots-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let save = root.join("Pixo");
        fs::create_dir_all(&save).unwrap();
        let picture = save.join("shot.png");
        fs::write(&picture, b"png").unwrap();
        let library = Library::open(&root.join("library.db"), &save).unwrap();
        let adopted = library.list(None, 20).unwrap();
        assert_eq!(adopted.len(), 1);
        assert_eq!(adopted[0].name, "shot.png");
        library.remember(&picture, Some("hello")).unwrap();
        let found = library.list(Some("hello"), 20).unwrap();
        assert_eq!(found.len(), 1);
        library.delete(found[0].id, &save).unwrap();
        assert!(!picture.exists());
        assert!(library.list(None, 20).unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
