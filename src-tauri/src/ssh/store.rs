use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::error::SshError;
use super::model::{Host, NormalizedHost, auth_from_db, auth_to_db};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ssh_hosts (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  group_name TEXT,
  host TEXT NOT NULL,
  port INTEGER NOT NULL DEFAULT 22,
  username TEXT NOT NULL,
  auth TEXT NOT NULL CHECK (auth IN ('password', 'key')),
  key_path TEXT,
  remember INTEGER NOT NULL DEFAULT 0,
  start_dir TEXT,
  term TEXT NOT NULL DEFAULT 'xterm-256color',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ssh_hosts_order ON ssh_hosts (group_name, sort_order, id);
CREATE TABLE IF NOT EXISTS ssh_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, SshError> {
        let conn = Connection::open(path).map_err(storage)?;
        conn.busy_timeout(std::time::Duration::from_millis(2_000))
            .map_err(storage)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .map_err(storage)?;
        conn.execute_batch("PRAGMA foreign_keys = ON")
            .map_err(storage)?;
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(storage)?;
        if version < 1 {
            conn.execute_batch(SCHEMA).map_err(storage)?;
            conn.pragma_update(None, "user_version", 2)
                .map_err(storage)?;
        } else if version < 2 {
            conn.execute_batch(
                "ALTER TABLE ssh_hosts ADD COLUMN remember INTEGER NOT NULL DEFAULT 0",
            )
            .map_err(storage)?;
            conn.pragma_update(None, "user_version", 2)
                .map_err(storage)?;
        }
        Ok(Self { conn })
    }

    pub fn enabled(&self) -> Result<bool, SshError> {
        self.bool_setting("enabled", false)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), SshError> {
        self.conn
            .execute(
                "INSERT INTO ssh_settings (key, value) VALUES ('enabled', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![if enabled { "true" } else { "false" }],
            )
            .map(|_| ())
            .map_err(storage)
    }

    pub fn list(&self) -> Result<Vec<Host>, SshError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order
                 FROM ssh_hosts
                 ORDER BY group_name IS NOT NULL, group_name, sort_order, id",
            )
            .map_err(storage)?;
        let rows = statement.query_map([], row_to_host).map_err(storage)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage)
    }

    pub fn get(&self, id: i64) -> Result<Host, SshError> {
        self.conn
            .query_row(
                "SELECT id, name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order
                 FROM ssh_hosts WHERE id = ?1",
                params![id],
                row_to_host,
            )
            .optional()
            .map_err(storage)?
            .ok_or_else(|| SshError::new("not_found"))
    }

    pub fn save(&self, host: &NormalizedHost, now: i64) -> Result<Host, SshError> {
        let auth = auth_to_db(host.auth);
        if let Some(id) = host.id {
            let changed = self
                .conn
                .execute(
                    "UPDATE ssh_hosts
                     SET name = ?1, group_name = ?2, host = ?3, port = ?4, username = ?5,
                         auth = ?6, key_path = ?7, remember = ?8, start_dir = ?9, term = ?10, updated_at = ?11
                     WHERE id = ?12",
                    params![
                        host.name,
                        host.group_name,
                        host.host,
                        host.port,
                        host.username,
                        auth,
                        host.key_path,
                        host.remember,
                        host.start_dir,
                        host.term,
                        now,
                        id,
                    ],
                )
                .map_err(storage)?;
            if changed == 0 {
                return Err(SshError::new("not_found"));
            }
            return self.get(id);
        }
        let sort_order: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM ssh_hosts",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        self.conn
            .execute(
                "INSERT INTO ssh_hosts
                 (name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
                params![
                    host.name,
                    host.group_name,
                    host.host,
                    host.port,
                    host.username,
                    auth,
                    host.key_path,
                    host.remember,
                    host.start_dir,
                    host.term,
                    sort_order,
                    now,
                ],
            )
            .map_err(storage)?;
        self.get(self.conn.last_insert_rowid())
    }

    pub fn delete(&self, id: i64) -> Result<(), SshError> {
        let changed = self
            .conn
            .execute("DELETE FROM ssh_hosts WHERE id = ?1", params![id])
            .map_err(storage)?;
        if changed == 0 {
            return Err(SshError::new("not_found"));
        }
        Ok(())
    }

    pub fn set_remember(&self, id: i64, remember: bool, now: i64) -> Result<(), SshError> {
        let changed = self
            .conn
            .execute(
                "UPDATE ssh_hosts SET remember = ?1, updated_at = ?2 WHERE id = ?3",
                params![remember, now, id],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(SshError::new("not_found"));
        }
        Ok(())
    }

    fn bool_setting(&self, key: &str, default: bool) -> Result<bool, SshError> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM ssh_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        Ok(match value.as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        })
    }
}

fn row_to_host(row: &rusqlite::Row<'_>) -> rusqlite::Result<Host> {
    let auth: String = row.get(6)?;
    Ok(Host {
        id: row.get(0)?,
        name: row.get(1)?,
        group_name: row.get(2)?,
        host: row.get(3)?,
        port: row.get(4)?,
        username: row.get(5)?,
        auth: auth_from_db(&auth).map_err(|_| {
            rusqlite::Error::InvalidColumnType(6, "auth".into(), rusqlite::types::Type::Text)
        })?,
        key_path: row.get(7)?,
        remember: row.get::<_, i64>(8)? != 0,
        start_dir: row.get(9)?,
        term: row.get(10)?,
        sort_order: row.get(11)?,
    })
}

fn storage(error: impl std::fmt::Display) -> SshError {
    eprintln!("ssh storage: {error}");
    SshError::new("storage")
}

#[cfg(test)]
mod tests {
    use super::Store;
    use crate::ssh::model::{AuthKind, HostDraft, normalize};

    #[test]
    fn saves_lists_and_deletes_hosts() {
        let path = std::env::temp_dir().join(format!(
            "pixo-ssh-store-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        assert!(!store.enabled().unwrap());
        store.set_enabled(true).unwrap();
        assert!(store.enabled().unwrap());
        let host = normalize(HostDraft {
            id: None,
            name: "build".into(),
            group_name: Some("lab".into()),
            host: "example.com".into(),
            port: 2222,
            username: "dev".into(),
            auth: AuthKind::Key,
            key_path: Some("/tmp/id_ed25519".into()),
            remember: false,
            start_dir: None,
            term: Some("xterm-256color".into()),
        })
        .unwrap();
        let saved = store.save(&host, 10).unwrap();
        assert!(!saved.remember);
        assert_eq!(saved.name, "build");
        assert_eq!(store.list().unwrap().len(), 1);
        let mut updated = host;
        updated.id = Some(saved.id);
        updated.name = "build-2".into();
        assert_eq!(store.save(&updated, 11).unwrap().name, "build-2");
        store.delete(saved.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migrates_a_version_one_database() {
        let path = std::env::temp_dir().join(format!(
            "pixo-ssh-migrate-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE ssh_hosts (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                group_name TEXT,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                username TEXT NOT NULL,
                auth TEXT NOT NULL,
                key_path TEXT,
                start_dir TEXT,
                term TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE ssh_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO ssh_hosts
             (name, group_name, host, port, username, auth, key_path, start_dir, term, sort_order, created_at, updated_at)
             VALUES ('box', NULL, 'example.com', 22, 'dev', 'password', NULL, NULL, 'xterm-256color', 1, 1, 1)",
            [],
        )
        .unwrap();
        drop(conn);
        let store = Store::open(&path).unwrap();
        let host = store.list().unwrap().pop().unwrap();
        assert!(!host.remember);
        store.set_remember(host.id, true, 2).unwrap();
        assert!(store.get(host.id).unwrap().remember);
        let _ = std::fs::remove_file(path);
    }
}
