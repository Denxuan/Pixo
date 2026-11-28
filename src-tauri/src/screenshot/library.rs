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

    pub fn list(
        &self,
        query: Option<&str>,
        limit: u32,
    ) -> Result<Vec<LibraryItem>, ScreenshotError> {
        let limit = limit.clamp(1, 200) as usize;
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, kind, path, bytes, created_at, text FROM shot_files
                 ORDER BY created_at DESC, id DESC",
            )
            .map_err(storage)?;
        let rows = statement.query_map([], map_item).map_err(storage)?;
        let mut items = Vec::new();
        let mut gone = Vec::new();
        for row in rows {
            let item = row.map_err(storage)?;
            if !matches_shot(&item.path, item.text.as_deref(), query) {
                continue;
            }
            let path = Path::new(&item.path);
            if path.is_file() {
                if items.len() < limit {
                    items.push(item);
                }
                continue;
            }
            // A missing parent means the folder or disk is unavailable. Keep the row.
            if path.parent().is_some_and(|dir| dir.is_dir()) {
                gone.push(item.id);
            }
        }
        drop(statement);
        for id in gone {
            let _ = self
                .conn
                .execute("DELETE FROM shot_files WHERE id = ?1", params![id]);
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

    pub fn set_text(&self, id: i64, text: &str) -> Result<(), ScreenshotError> {
        let stored = clip_text(Some(text)).ok_or_else(|| ScreenshotError::new("invalid_input"))?;
        let changed = self
            .conn
            .execute(
                "UPDATE shot_files SET text = ?1 WHERE id = ?2 AND kind = 'image'",
                params![stored, id],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(ScreenshotError::new("not_found"));
        }
        Ok(())
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

fn matches_shot(path: &str, text: Option<&str>, query: Option<&str>) -> bool {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let needle: String = query.chars().take(200).collect::<String>().to_lowercase();
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_lowercase();
    name.contains(&needle) || text.is_some_and(|value| value.to_lowercase().contains(&needle))
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

    #[test]
    fn search_uses_the_file_name_and_keeps_rows_while_the_folder_is_away() {
        assert!(!matches_shot(
            r"C:\Pictures\Pixo\note.png",
            None,
            Some("Pictures")
        ));
        assert!(matches_shot(
            r"C:\Pictures\Pixo\note.png",
            None,
            Some("Note")
        ));
        assert!(matches_shot(
            "/Users/zdx/Pictures/Pixo/note.png",
            Some("会议记录"),
            Some("会议")
        ));
        assert!(!matches_shot(
            "/Users/zdx/Pictures/Pixo/note.png",
            Some("会议"),
            Some("Users")
        ));

        let root = std::env::temp_dir().join(format!(
            "pixo-shots-search-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let save = root.join("Pictures").join("Pixo");
        fs::create_dir_all(&save).unwrap();
        let picture = save.join("note.png");
        fs::write(&picture, b"png").unwrap();
        let library = Library::open(&root.join("library.db"), &save).unwrap();
        library.remember(&picture, Some("hello")).unwrap();
        assert!(library.list(Some("Pictures"), 20).unwrap().is_empty());
        assert_eq!(library.list(Some("note"), 20).unwrap().len(), 1);
        assert_eq!(library.list(Some("HELLO"), 20).unwrap().len(), 1);

        let away = root.join("volume").join("Pixo");
        fs::create_dir_all(&away).unwrap();
        let kept = away.join("kept.png");
        fs::write(&kept, b"png").unwrap();
        library.remember(&kept, None).unwrap();
        fs::remove_dir_all(root.join("volume")).unwrap();
        assert_eq!(library.list(Some("kept"), 20).unwrap().len(), 0);
        fs::create_dir_all(&away).unwrap();
        fs::write(&kept, b"png").unwrap();
        assert_eq!(library.list(Some("kept"), 20).unwrap().len(), 1);

        fs::remove_file(&picture).unwrap();
        assert!(library.list(Some("note"), 20).unwrap().is_empty());
        fs::write(&picture, b"png").unwrap();
        assert!(
            library.list(Some("note"), 20).unwrap().is_empty(),
            "deleting the file while its folder remains drops the library row"
        );
        let _ = fs::remove_dir_all(root);
    }
}
