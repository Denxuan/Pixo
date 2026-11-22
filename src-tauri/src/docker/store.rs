use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::compose::{ComposeService, SavedCompose};
use super::error::DockerError;
use super::model::{Endpoint, EndpointKind, NormalizedEndpoint};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS docker_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS docker_endpoints (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('local', 'ssh', 'tls', 'tcp')),
  socket_path TEXT,
  ssh_host_id INTEGER,
  remote_socket TEXT,
  host TEXT,
  port INTEGER,
  ca_path TEXT,
  cert_path TEXT,
  key_path TEXT,
  insecure INTEGER NOT NULL DEFAULT 0,
  source TEXT NOT NULL DEFAULT 'manual',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, DockerError> {
        let conn = Connection::open(path).map_err(storage)?;
        conn.busy_timeout(std::time::Duration::from_millis(2_000))
            .map_err(storage)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON")
            .map_err(storage)?;
        conn.execute_batch(SCHEMA).map_err(storage)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap_or(0);
        if version < 2 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS docker_compose_projects (
                   id INTEGER PRIMARY KEY,
                   endpoint_id INTEGER NOT NULL,
                   name TEXT NOT NULL,
                   config_path TEXT NOT NULL,
                   working_dir TEXT NOT NULL,
                   services TEXT NOT NULL,
                   created_at INTEGER NOT NULL,
                   UNIQUE(endpoint_id, config_path)
                 );",
            )
            .map_err(storage)?;
            conn.pragma_update(None, "user_version", 2).map_err(storage)?;
        }
        Ok(Self { conn })
    }

    pub fn enabled(&self) -> Result<bool, DockerError> {
        self.bool_setting("enabled", false)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), DockerError> {
        self.conn
            .execute(
                "INSERT INTO docker_settings (key, value) VALUES ('enabled', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![if enabled { "true" } else { "false" }],
            )
            .map(|_| ())
            .map_err(storage)
    }

    pub fn list(&self) -> Result<Vec<Endpoint>, DockerError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, name, kind, socket_path, ssh_host_id, remote_socket, host, port, ca_path, cert_path, key_path, insecure, source, sort_order
                 FROM docker_endpoints
                 ORDER BY sort_order, id",
            )
            .map_err(storage)?;
        let rows = statement.query_map([], row_to_endpoint).map_err(storage)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage)
    }

    pub fn get(&self, id: i64) -> Result<Endpoint, DockerError> {
        self.conn
            .query_row(
                "SELECT id, name, kind, socket_path, ssh_host_id, remote_socket, host, port, ca_path, cert_path, key_path, insecure, source, sort_order
                 FROM docker_endpoints WHERE id = ?1",
                params![id],
                row_to_endpoint,
            )
            .optional()
            .map_err(storage)?
            .ok_or_else(|| DockerError::new("not_found"))
    }

    pub fn save(&self, endpoint: &NormalizedEndpoint, source: &str, now: i64) -> Result<Endpoint, DockerError> {
        if source != "manual" && source != "context" {
            return Err(DockerError::new("invalid_input"));
        }
        let port = endpoint.port.map(i64::from);
        let insecure = if endpoint.insecure { 1 } else { 0 };
        if let Some(id) = endpoint.id {
            let existing = self.get(id)?;
            let changed = self
                .conn
                .execute(
                    "UPDATE docker_endpoints
                     SET name = ?1, kind = ?2, socket_path = ?3, ssh_host_id = ?4, remote_socket = ?5,
                         host = ?6, port = ?7, ca_path = ?8, cert_path = ?9, key_path = ?10, insecure = ?11, updated_at = ?12
                     WHERE id = ?13",
                    params![
                        endpoint.name,
                        endpoint.kind.as_str(),
                        endpoint.socket_path,
                        endpoint.ssh_host_id,
                        endpoint.remote_socket,
                        endpoint.host,
                        port,
                        endpoint.ca_path,
                        endpoint.cert_path,
                        endpoint.key_path,
                        insecure,
                        now,
                        id,
                    ],
                )
                .map_err(storage)?;
            if changed == 0 {
                return Err(DockerError::new("not_found"));
            }
            let _ = existing;
            self.get(id)
        } else {
            self.conn
                .execute(
                    "INSERT INTO docker_endpoints (
                        name, kind, socket_path, ssh_host_id, remote_socket, host, port, ca_path, cert_path, key_path, insecure, source, sort_order, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                    params![
                        endpoint.name,
                        endpoint.kind.as_str(),
                        endpoint.socket_path,
                        endpoint.ssh_host_id,
                        endpoint.remote_socket,
                        endpoint.host,
                        port,
                        endpoint.ca_path,
                        endpoint.cert_path,
                        endpoint.key_path,
                        insecure,
                        source,
                        now,
                        now,
                    ],
                )
                .map_err(storage)?;
            self.get(self.conn.last_insert_rowid())
        }
    }

    pub fn delete(&self, id: i64) -> Result<(), DockerError> {
        let changed = self
            .conn
            .execute("DELETE FROM docker_endpoints WHERE id = ?1", params![id])
            .map_err(storage)?;
        if changed == 0 {
            return Err(DockerError::new("not_found"));
        }
        self.conn
            .execute("DELETE FROM docker_compose_projects WHERE endpoint_id = ?1", params![id])
            .map_err(storage)?;
        Ok(())
    }

    pub fn list_compose(&self, endpoint_id: i64) -> Result<Vec<SavedCompose>, DockerError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, name, config_path, working_dir, services
                 FROM docker_compose_projects
                 WHERE endpoint_id = ?1
                 ORDER BY name, id",
            )
            .map_err(storage)?;
        let rows = statement
            .query_map(params![endpoint_id], |row| {
                let services: String = row.get(4)?;
                Ok(SavedCompose {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    config_path: row.get(2)?,
                    working_dir: row.get(3)?,
                    services: serde_json::from_str(&services).unwrap_or_default(),
                })
            })
            .map_err(storage)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage)
    }

    pub fn upsert_compose(
        &self,
        endpoint_id: i64,
        name: &str,
        config_path: &str,
        working_dir: &str,
        services: &[ComposeService],
        now: i64,
    ) -> Result<SavedCompose, DockerError> {
        let services_json = serde_json::to_string(services).map_err(|_| DockerError::new("invalid_input"))?;
        self.conn
            .execute(
                "INSERT INTO docker_compose_projects (endpoint_id, name, config_path, working_dir, services, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(endpoint_id, config_path) DO UPDATE SET
                   name = excluded.name,
                   working_dir = excluded.working_dir,
                   services = excluded.services",
                params![endpoint_id, name, config_path, working_dir, services_json, now],
            )
            .map_err(storage)?;
        self.conn
            .query_row(
                "SELECT id, name, config_path, working_dir, services FROM docker_compose_projects WHERE endpoint_id = ?1 AND config_path = ?2",
                params![endpoint_id, config_path],
                |row| {
                    let services: String = row.get(4)?;
                    Ok(SavedCompose {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        config_path: row.get(2)?,
                        working_dir: row.get(3)?,
                        services: serde_json::from_str(&services).unwrap_or_default(),
                    })
                },
            )
            .map_err(storage)
    }

    pub fn delete_compose(&self, endpoint_id: i64, id: i64) -> Result<(), DockerError> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM docker_compose_projects WHERE endpoint_id = ?1 AND id = ?2",
                params![endpoint_id, id],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(DockerError::new("not_found"));
        }
        Ok(())
    }

    fn bool_setting(&self, key: &str, default: bool) -> Result<bool, DockerError> {
        let value = self
            .conn
            .query_row("SELECT value FROM docker_settings WHERE key = ?1", params![key], |row| row.get::<_, String>(0))
            .optional()
            .map_err(storage)?;
        Ok(match value.as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        })
    }
}

fn row_to_endpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<Endpoint> {
    let kind = row.get::<_, String>(2)?;
    let kind = EndpointKind::parse(&kind).ok_or_else(|| rusqlite::Error::InvalidColumnType(2, "kind".into(), rusqlite::types::Type::Text))?;
    let port = row.get::<_, Option<i64>>(7)?;
    Ok(Endpoint {
        id: row.get(0)?,
        name: row.get(1)?,
        kind,
        socket_path: row.get(3)?,
        ssh_host_id: row.get(4)?,
        remote_socket: row.get(5)?,
        host: row.get(6)?,
        port: port.and_then(|value| u16::try_from(value).ok()),
        ca_path: row.get(8)?,
        cert_path: row.get(9)?,
        key_path: row.get(10)?,
        insecure: row.get::<_, i64>(11)? != 0,
        source: row.get(12)?,
        sort_order: row.get(13)?,
    })
}

fn storage(error: impl std::fmt::Display) -> DockerError {
    eprintln!("docker storage: {error}");
    DockerError::new("storage")
}
