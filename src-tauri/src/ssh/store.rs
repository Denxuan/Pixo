use std::collections::{HashMap, HashSet};
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
  auth TEXT NOT NULL CHECK (auth IN ('password', 'key', 'agent')),
  key_path TEXT,
  remember INTEGER NOT NULL DEFAULT 0,
  start_dir TEXT,
  term TEXT NOT NULL DEFAULT 'xterm-256color',
  sort_order INTEGER NOT NULL DEFAULT 0,
  agent_forward INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ssh_hosts_order ON ssh_hosts (group_name, sort_order, id);
CREATE TABLE IF NOT EXISTS ssh_jumps (
  host_id INTEGER NOT NULL REFERENCES ssh_hosts(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  jump_id INTEGER NOT NULL REFERENCES ssh_hosts(id) ON DELETE RESTRICT,
  PRIMARY KEY (host_id, position)
);
CREATE TABLE IF NOT EXISTS ssh_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

const SNIPPETS: &str = "
CREATE TABLE IF NOT EXISTS ssh_snippets (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
";

const FORWARDS: &str = "
CREATE TABLE IF NOT EXISTS ssh_forwards (
  id INTEGER PRIMARY KEY,
  host_id INTEGER NOT NULL REFERENCES ssh_hosts(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('local', 'remote', 'dynamic')),
  listen_port INTEGER NOT NULL,
  target_host TEXT NOT NULL,
  target_port INTEGER NOT NULL,
  UNIQUE (host_id, kind, listen_port)
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
        } else {
            if version < 2 {
                conn.execute_batch(
                    "ALTER TABLE ssh_hosts ADD COLUMN remember INTEGER NOT NULL DEFAULT 0",
                )
                .map_err(storage)?;
            }
            if version < 3 {
                conn.execute_batch(MIGRATE_V3).map_err(storage)?;
            }
            if version < 4 {
                conn.execute_batch(
                    "ALTER TABLE ssh_hosts ADD COLUMN agent_forward INTEGER NOT NULL DEFAULT 0",
                )
                .map_err(storage)?;
            }
        }
        conn.execute_batch(SNIPPETS).map_err(storage)?;
        conn.execute_batch(FORWARDS).map_err(storage)?;
        conn.pragma_update(None, "user_version", 5)
            .map_err(storage)?;
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
                "SELECT id, name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order, agent_forward
                 FROM ssh_hosts
                 ORDER BY group_name IS NOT NULL, group_name, sort_order, id",
            )
            .map_err(storage)?;
        let rows = statement.query_map([], row_to_host).map_err(storage)?;
        let hosts = rows.collect::<Result<Vec<_>, _>>().map_err(storage)?;
        self.attach_jumps(hosts)
    }

    pub fn get(&self, id: i64) -> Result<Host, SshError> {
        self.conn
            .query_row(
                "SELECT id, name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order, agent_forward
                 FROM ssh_hosts WHERE id = ?1",
                params![id],
                row_to_host,
            )
            .optional()
            .map_err(storage)?
            .ok_or_else(|| SshError::new("not_found"))
            .and_then(|host| self.attach_jumps(vec![host]).map(|mut hosts| hosts.remove(0)))
    }

    pub fn save(&self, host: &NormalizedHost, now: i64) -> Result<Host, SshError> {
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(storage)?;
        let saved = self.save_in_transaction(host, now);
        if saved.is_ok() {
            self.conn.execute_batch("COMMIT").map_err(storage)?;
        } else {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        saved
    }

    fn save_in_transaction(&self, host: &NormalizedHost, now: i64) -> Result<Host, SshError> {
        let auth = auth_to_db(host.auth);
        if let Some(id) = host.id {
            let changed = self
                .conn
                .execute(
                    "UPDATE ssh_hosts
                     SET name = ?1, group_name = ?2, host = ?3, port = ?4, username = ?5,
                         auth = ?6, key_path = ?7, remember = ?8, start_dir = ?9, term = ?10,
                         agent_forward = ?11, updated_at = ?12
                     WHERE id = ?13",
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
                        host.agent_forward,
                        now,
                        id,
                    ],
                )
                .map_err(storage)?;
            if changed == 0 {
                return Err(SshError::new("not_found"));
            }
            self.replace_jumps(id, &host.jumps)?;
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
                 (name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order, agent_forward, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
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
                    host.agent_forward,
                    now,
                ],
            )
            .map_err(storage)?;
        let id = self.conn.last_insert_rowid();
        self.replace_jumps(id, &host.jumps)?;
        self.get(id)
    }

    pub fn delete(&self, id: i64, detach: bool) -> Result<(), SshError> {
        let names = self.dependent_names(id)?;
        if !names.is_empty() && !detach {
            return Err(SshError::with_params(
                "jump_in_use",
                serde_json::json!({ "hosts": names.join("、") }),
            ));
        }
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(storage)?;
        let deleted = (|| {
            self.conn
                .execute(
                    "DELETE FROM ssh_jumps WHERE host_id = ?1 OR jump_id = ?1",
                    params![id],
                )
                .map_err(storage)?;
            let changed = self
                .conn
                .execute("DELETE FROM ssh_hosts WHERE id = ?1", params![id])
                .map_err(storage)?;
            if changed == 0 {
                return Err(SshError::new("not_found"));
            }
            Ok(())
        })();
        if deleted.is_ok() {
            self.conn.execute_batch("COMMIT").map_err(storage)?;
        } else {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        deleted
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

    fn attach_jumps(&self, mut hosts: Vec<Host>) -> Result<Vec<Host>, SshError> {
        let mut statement = self
            .conn
            .prepare("SELECT host_id, jump_id FROM ssh_jumps ORDER BY host_id, position")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .map_err(storage)?;
        let mut jumps = HashMap::<i64, Vec<i64>>::new();
        for row in rows {
            let (host_id, jump_id) = row.map_err(storage)?;
            jumps.entry(host_id).or_default().push(jump_id);
        }
        for host in &mut hosts {
            host.jumps = jumps.remove(&host.id).unwrap_or_default();
        }
        Ok(hosts)
    }

    fn replace_jumps(&self, host_id: i64, jumps: &[i64]) -> Result<(), SshError> {
        self.ensure_jumps(host_id, jumps)?;
        self.conn
            .execute("DELETE FROM ssh_jumps WHERE host_id = ?1", params![host_id])
            .map_err(storage)?;
        for (position, jump_id) in jumps.iter().enumerate() {
            self.conn
                .execute(
                    "INSERT INTO ssh_jumps (host_id, position, jump_id) VALUES (?1, ?2, ?3)",
                    params![host_id, position as i64, jump_id],
                )
                .map_err(storage)?;
        }
        Ok(())
    }

    fn ensure_jumps(&self, host_id: i64, jumps: &[i64]) -> Result<(), SshError> {
        for jump_id in jumps {
            let exists: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM ssh_hosts WHERE id = ?1",
                    params![jump_id],
                    |row| row.get(0),
                )
                .map_err(storage)?;
            if exists == 0 {
                return Err(SshError::new("invalid_input"));
            }
        }
        if jumps.is_empty() {
            return Ok(());
        }
        let mut edges = HashMap::<i64, Vec<i64>>::new();
        let mut statement = self
            .conn
            .prepare("SELECT host_id, jump_id FROM ssh_jumps ORDER BY position")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .map_err(storage)?;
        for row in rows {
            let (from, to) = row.map_err(storage)?;
            if from != host_id {
                edges.entry(from).or_default().push(to);
            }
        }
        edges.insert(host_id, jumps.to_vec());
        let mut pending = jumps.to_vec();
        let mut seen = HashSet::new();
        while let Some(current) = pending.pop() {
            if current == host_id {
                return Err(SshError::new("jump_cycle"));
            }
            if !seen.insert(current) {
                continue;
            }
            if let Some(next) = edges.get(&current) {
                pending.extend(next.iter().copied());
            }
        }
        Ok(())
    }

    pub fn dependent_names(&self, id: i64) -> Result<Vec<String>, SshError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT h.name FROM ssh_jumps j JOIN ssh_hosts h ON h.id = j.host_id WHERE j.jump_id = ?1 ORDER BY h.name",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(params![id], |row| row.get(0))
            .map_err(storage)?;
        rows.collect::<Result<Vec<String>, _>>().map_err(storage)
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

    pub fn keepalive(&self) -> Result<u32, SshError> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM ssh_settings WHERE key = 'keepalive'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        let seconds = value
            .as_deref()
            .and_then(|text| text.parse::<u32>().ok())
            .unwrap_or(30);
        Ok(if seconds > 600 { 30 } else { seconds })
    }

    pub fn set_keepalive(&self, seconds: u32) -> Result<(), SshError> {
        if seconds > 600 {
            return Err(SshError::new("invalid_input"));
        }
        self.conn
            .execute(
                "INSERT INTO ssh_settings (key, value) VALUES ('keepalive', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![seconds.to_string()],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub fn snippets(&self) -> Result<Vec<super::model::Snippet>, SshError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, title, body FROM ssh_snippets ORDER BY created_at DESC, id DESC")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok(super::model::Snippet {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                })
            })
            .map_err(storage)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage)
    }

    pub fn save_snippet(
        &self,
        draft: &super::model::SnippetDraft,
    ) -> Result<super::model::Snippet, SshError> {
        let title = draft.title.trim();
        let body = draft.body.trim();
        if title.is_empty()
            || body.is_empty()
            || title.chars().count() > 80
            || body.chars().count() > 4_000
        {
            return Err(SshError::new("invalid_input"));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        if let Some(id) = draft.id {
            let changed = self
                .conn
                .execute(
                    "UPDATE ssh_snippets SET title = ?1, body = ?2 WHERE id = ?3",
                    params![title, body, id],
                )
                .map_err(storage)?;
            if changed == 0 {
                return Err(SshError::new("not_found"));
            }
            return Ok(super::model::Snippet {
                id,
                title: title.to_string(),
                body: body.to_string(),
            });
        }
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM ssh_snippets", [], |row| row.get(0))
            .map_err(storage)?;
        if count >= 100 {
            return Err(SshError::new("snippet_limit"));
        }
        self.conn
            .execute(
                "INSERT INTO ssh_snippets (title, body, created_at) VALUES (?1, ?2, ?3)",
                params![title, body, now],
            )
            .map_err(storage)?;
        Ok(super::model::Snippet {
            id: self.conn.last_insert_rowid(),
            title: title.to_string(),
            body: body.to_string(),
        })
    }

    pub fn delete_snippet(&self, id: i64) -> Result<(), SshError> {
        let changed = self
            .conn
            .execute("DELETE FROM ssh_snippets WHERE id = ?1", params![id])
            .map_err(storage)?;
        if changed == 0 {
            return Err(SshError::new("not_found"));
        }
        Ok(())
    }

    pub fn forwards(&self, host_id: i64) -> Result<Vec<SavedForward>, SshError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, kind, listen_port, target_host, target_port
                 FROM ssh_forwards WHERE host_id = ?1 ORDER BY id",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(params![host_id], |row| {
                Ok(SavedForward {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    listen_port: row.get(2)?,
                    target_host: row.get(3)?,
                    target_port: row.get(4)?,
                })
            })
            .map_err(storage)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage)
    }

    pub fn save_forward(
        &self,
        host_id: i64,
        kind: &str,
        listen_port: u16,
        target_host: &str,
        target_port: u16,
    ) -> Result<i64, SshError> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM ssh_forwards WHERE host_id = ?1 AND kind = ?2 AND listen_port = ?3",
                params![host_id, kind, listen_port],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        if existing.is_some() {
            return Err(SshError::new("port_in_use"));
        }
        self.conn
            .execute(
                "INSERT INTO ssh_forwards (host_id, kind, listen_port, target_host, target_port)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![host_id, kind, listen_port, target_host, target_port],
            )
            .map_err(storage)?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_forward(&self, id: i64) -> Result<(), SshError> {
        self.conn
            .execute("DELETE FROM ssh_forwards WHERE id = ?1", params![id])
            .map(|_| ())
            .map_err(storage)
    }
}

pub struct SavedForward {
    pub id: i64,
    pub kind: String,
    pub listen_port: u16,
    pub target_host: String,
    pub target_port: u16,
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
        jumps: Vec::new(),
        agent_forward: row.get::<_, i64>(12)? != 0,
    })
}

const MIGRATE_V3: &str = "
CREATE TABLE ssh_hosts_next (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  group_name TEXT,
  host TEXT NOT NULL,
  port INTEGER NOT NULL DEFAULT 22,
  username TEXT NOT NULL,
  auth TEXT NOT NULL CHECK (auth IN ('password', 'key', 'agent')),
  key_path TEXT,
  remember INTEGER NOT NULL DEFAULT 0,
  start_dir TEXT,
  term TEXT NOT NULL DEFAULT 'xterm-256color',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
INSERT INTO ssh_hosts_next
  (id, name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order, created_at, updated_at)
SELECT id, name, group_name, host, port, username, auth, key_path, remember, start_dir, term, sort_order, created_at, updated_at
FROM ssh_hosts;
DROP TABLE ssh_hosts;
ALTER TABLE ssh_hosts_next RENAME TO ssh_hosts;
CREATE INDEX IF NOT EXISTS idx_ssh_hosts_order ON ssh_hosts (group_name, sort_order, id);
CREATE TABLE IF NOT EXISTS ssh_jumps (
  host_id INTEGER NOT NULL REFERENCES ssh_hosts(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  jump_id INTEGER NOT NULL REFERENCES ssh_hosts(id) ON DELETE RESTRICT,
  PRIMARY KEY (host_id, position)
);
";

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
            jumps: Vec::new(),
            agent_forward: false,
        })
        .unwrap();
        let saved = store.save(&host, 10).unwrap();
        assert!(!saved.remember);
        assert!(!saved.agent_forward);
        assert_eq!(store.keepalive().unwrap(), 30);
        store.set_keepalive(0).unwrap();
        assert_eq!(store.keepalive().unwrap(), 0);
        assert!(store.set_keepalive(601).is_err());
        assert_eq!(saved.name, "build");
        assert_eq!(store.list().unwrap().len(), 1);
        let mut updated = host;
        updated.id = Some(saved.id);
        updated.name = "build-2".into();
        assert_eq!(store.save(&updated, 11).unwrap().name, "build-2");
        store.delete(saved.id, false).unwrap();
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
        assert!(!host.agent_forward);
        store.set_remember(host.id, true, 2).unwrap();
        assert!(store.get(host.id).unwrap().remember);
        let jump = normalize(HostDraft {
            id: None,
            name: "jump".into(),
            group_name: None,
            host: "jump.example".into(),
            port: 22,
            username: "dev".into(),
            auth: AuthKind::Agent,
            key_path: None,
            remember: true,
            start_dir: None,
            term: None,
            jumps: Vec::new(),
            agent_forward: true,
        })
        .unwrap();
        let jump = store.save(&jump, 3).unwrap();
        assert_eq!(jump.auth, AuthKind::Agent);
        assert!(!jump.remember);
        assert!(jump.agent_forward);
        let mut via = normalize(HostDraft {
            id: Some(host.id),
            name: "box".into(),
            group_name: None,
            host: "example.com".into(),
            port: 22,
            username: "dev".into(),
            auth: AuthKind::Password,
            key_path: None,
            remember: true,
            start_dir: None,
            term: None,
            jumps: vec![jump.id],
            agent_forward: false,
        })
        .unwrap();
        assert_eq!(store.save(&via, 4).unwrap().jumps, vec![jump.id]);
        via.jumps = vec![host.id];
        assert_eq!(store.save(&via, 5).unwrap_err().code, "jump_cycle");
        assert_eq!(
            store.delete(jump.id, false).unwrap_err().code,
            "jump_in_use"
        );
        store.delete(jump.id, true).unwrap();
        assert!(store.get(jump.id).is_err());
        let _ = std::fs::remove_file(path);
    }
}
