use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::error::DockerError;
use super::model::{EndpointDraft, EndpointKind, NormalizedEndpoint, normalize};
use super::sockets::home_dir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPreview {
    pub index: u32,
    pub name: String,
    pub kind: String,
    pub detail: String,
    pub warning: Option<String>,
    pub importable: bool,
    pub current: bool,
}

#[derive(Debug, Clone)]
pub struct ImportableContext {
    pub draft: EndpointDraft,
    pub warning: Option<String>,
    pub importable: bool,
    pub current: bool,
}

pub fn load_previews(
    ssh_hosts: &[(i64, String, String, u16)],
) -> Result<Vec<ContextPreview>, DockerError> {
    Ok(load_contexts(ssh_hosts)?
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let detail = detail_of(&item.draft);
            ContextPreview {
                index: index as u32,
                name: item.draft.name,
                kind: item.draft.kind.as_str().to_string(),
                detail,
                warning: item.warning,
                importable: item.importable,
                current: item.current,
            }
        })
        .collect())
}

pub fn selected_contexts(
    indexes: &[u32],
    ssh_hosts: &[(i64, String, String, u16)],
) -> Result<Vec<NormalizedEndpoint>, DockerError> {
    if indexes.len() > 200 {
        return Err(DockerError::new("invalid_input"));
    }
    let contexts = load_contexts(ssh_hosts)?;
    let mut chosen = Vec::new();
    for index in indexes {
        let Some(item) = contexts.get(*index as usize) else {
            return Err(DockerError::new("invalid_input"));
        };
        if !item.importable {
            return Err(DockerError::new("invalid_input"));
        }
        chosen.push(normalize(item.draft.clone())?);
    }
    if chosen.is_empty() {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(chosen)
}

fn load_contexts(
    ssh_hosts: &[(i64, String, String, u16)],
) -> Result<Vec<ImportableContext>, DockerError> {
    let Some(root) = docker_config_dir() else {
        return Ok(Vec::new());
    };
    let current = current_context_name(&root);
    let meta_root = root.join("contexts").join("meta");
    let mut names = match fs::read_dir(&meta_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            eprintln!("docker context: {error}");
            return Err(DockerError::new("storage"));
        }
    }
    .filter_map(|entry| entry.ok())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| !name.is_empty() && name.len() <= 128 && !name.contains(['/', '\\', '\0']))
    .take(200)
    .collect::<Vec<_>>();
    names.sort();
    let mut items = Vec::new();
    for name in names {
        let meta_path = meta_root.join(&name).join("meta.json");
        let text = match fs::read_to_string(&meta_path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(_) => {
                items.push(skipped(&name, "unsupported"));
                continue;
            }
        };
        match parse_meta(&value, ssh_hosts) {
            Some(mut item) => {
                item.current = current.as_deref() == Some(item.draft.name.as_str());
                items.push(item);
            }
            None => items.push(skipped(&name, "unsupported")),
        }
    }
    items.sort_by(|left, right| left.draft.name.cmp(&right.draft.name));
    Ok(items)
}

pub fn parse_meta(
    value: &Value,
    ssh_hosts: &[(i64, String, String, u16)],
) -> Option<ImportableContext> {
    let name = value
        .get("Name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() || name.chars().count() > 128 {
        return None;
    }
    let host = value
        .pointer("/Endpoints/docker/Host")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if host.is_empty() {
        return None;
    }
    let mut draft = EndpointDraft {
        id: None,
        name: name.to_string(),
        kind: EndpointKind::Local,
        socket_path: None,
        ssh_host_id: None,
        remote_socket: None,
        host: None,
        port: None,
    };
    let mut warning = None;
    let mut importable = true;
    if let Some(path) = host.strip_prefix("unix://") {
        draft.kind = EndpointKind::Local;
        draft.socket_path = Some(path.to_string());
        if cfg!(windows) {
            warning = Some("unsupported".to_string());
            importable = false;
        }
    } else if let Some(path) = host.strip_prefix("npipe://") {
        draft.kind = EndpointKind::Local;
        let pipe = path
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .unwrap_or("docker_engine");
        draft.socket_path = Some(format!(r"\\.\pipe\{pipe}"));
        if cfg!(unix) {
            warning = Some("unsupported".to_string());
            importable = false;
        }
    } else if let Some(rest) = host.strip_prefix("ssh://") {
        draft.kind = EndpointKind::Ssh;
        let (user, address, port) = parse_ssh_authority(rest)?;
        draft.host = Some(address.clone());
        draft.port = Some(port);
        if user.is_empty() {
            warning = Some("ssh_unmatched".to_string());
            importable = false;
        } else if let Some((id, _, _, _)) =
            ssh_hosts
                .iter()
                .find(|(_, known_user, known_host, known_port)| {
                    known_user == &user && known_host == &address && *known_port == port
                })
        {
            draft.ssh_host_id = Some(*id);
            draft.remote_socket = Some("/var/run/docker.sock".into());
        } else {
            warning = Some("ssh_unmatched".to_string());
            importable = false;
        }
    } else {
        return None;
    }
    Some(ImportableContext {
        draft,
        warning,
        importable,
        current: false,
    })
}

fn skipped(name: &str, warning: &str) -> ImportableContext {
    ImportableContext {
        draft: EndpointDraft {
            id: None,
            name: name.to_string(),
            kind: EndpointKind::Local,
            socket_path: None,
            ssh_host_id: None,
            remote_socket: None,
            host: None,
            port: None,
        },
        warning: Some(warning.to_string()),
        importable: false,
        current: false,
    }
}

fn detail_of(draft: &EndpointDraft) -> String {
    match draft.kind {
        EndpointKind::Local => draft.socket_path.clone().unwrap_or_default(),
        EndpointKind::Ssh => format!(
            "{}{}",
            draft.host.clone().unwrap_or_default(),
            draft
                .port
                .map(|port| format!(":{port}"))
                .unwrap_or_default()
        ),
    }
}

pub fn docker_config_dir() -> Option<PathBuf> {
    if let Some(config) = std::env::var_os("DOCKER_CONFIG") {
        let path = PathBuf::from(config);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    home_dir().map(|home| home.join(".docker"))
}

fn current_context_name(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("config.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("currentContext")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn parse_ssh_authority(value: &str) -> Option<(String, String, u16)> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return None;
    }
    let (user, hostport) = match value.rsplit_once('@') {
        Some((user, hostport)) if !user.is_empty() && !user.contains('@') => {
            (user.to_string(), hostport)
        }
        _ => (String::new(), value),
    };
    let (host, port) = split_host_port(hostport, 22)?;
    Some((user, host, port))
}

fn split_host_port(value: &str, default_port: u16) -> Option<(String, u16)> {
    if let Some(rest) = value.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        if host.is_empty() {
            return None;
        }
        let port = if port.is_empty() {
            default_port
        } else {
            port.parse().ok()?
        };
        return Some((host.to_string(), port));
    }
    if let Some((host, port)) = value.rsplit_once(':') {
        if !host.is_empty() && !host.contains(':') && !port.is_empty() {
            return Some((host.to_string(), port.parse().ok()?));
        }
    }
    if value.is_empty() || value.contains(':') {
        return None;
    }
    Some((value.to_string(), default_port))
}

#[cfg(test)]
mod tests {
    use super::parse_meta;

    #[test]
    fn ssh_context_without_a_saved_host_is_shown_but_not_imported() {
        let value = serde_json::json!({
            "Name": "edge",
            "Endpoints": { "docker": { "Host": "ssh://dev@10.0.0.8:2222" } }
        });
        let parsed = parse_meta(&value, &[]).unwrap();
        assert_eq!(parsed.warning.as_deref(), Some("ssh_unmatched"));
        assert!(!parsed.importable);
        assert_eq!(parsed.draft.host.as_deref(), Some("10.0.0.8"));
        assert_eq!(parsed.draft.port, Some(2222));
    }

    #[test]
    fn unix_context_keeps_its_socket_and_tcp_context_is_skipped() {
        let unix = serde_json::json!({
            "Name": "desktop",
            "Endpoints": { "docker": { "Host": "unix:///Users/dev/.docker/run/docker.sock" } }
        });
        let parsed = parse_meta(&unix, &[]).unwrap();
        assert_eq!(
            parsed.draft.socket_path.as_deref(),
            Some("/Users/dev/.docker/run/docker.sock")
        );
        assert!(parsed.importable);

        let tcp = serde_json::json!({
            "Name": "lab",
            "Endpoints": { "docker": { "Host": "tcp://10.1.1.8:2375" } }
        });
        assert!(parse_meta(&tcp, &[]).is_none());
    }

    #[test]
    fn kubernetes_only_context_is_skipped() {
        let value = serde_json::json!({
            "Name": "k8s",
            "Endpoints": { "kubernetes": { "Host": "https://example" } }
        });
        assert!(parse_meta(&value, &[]).is_none());
    }
}
