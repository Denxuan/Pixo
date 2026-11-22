use serde::{Deserialize, Serialize};

use super::error::DockerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointKind {
    Local,
    Ssh,
    Tls,
    Tcp,
}

impl EndpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ssh => "ssh",
            Self::Tls => "tls",
            Self::Tcp => "tcp",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "ssh" => Some(Self::Ssh),
            "tls" => Some(Self::Tls),
            "tcp" => Some(Self::Tcp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub id: i64,
    pub name: String,
    pub kind: EndpointKind,
    pub socket_path: Option<String>,
    pub ssh_host_id: Option<i64>,
    pub remote_socket: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub ca_path: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub insecure: bool,
    pub source: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDraft {
    pub id: Option<i64>,
    pub name: String,
    pub kind: EndpointKind,
    pub socket_path: Option<String>,
    pub ssh_host_id: Option<i64>,
    pub remote_socket: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub ca_path: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    #[serde(default)]
    pub insecure: bool,
}

#[derive(Debug, Clone)]
pub struct NormalizedEndpoint {
    pub id: Option<i64>,
    pub name: String,
    pub kind: EndpointKind,
    pub socket_path: Option<String>,
    pub ssh_host_id: Option<i64>,
    pub remote_socket: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub ca_path: Option<String>,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub insecure: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerStatus {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineSummary {
    pub name: String,
    pub hostname: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub cpus: i64,
    pub memory: i64,
    pub containers: i64,
    pub images: i64,
    pub socket: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerRow {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: Vec<String>,
    pub created: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRow {
    pub id: String,
    pub tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRow {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRow {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub scope: String,
}

pub fn normalize(draft: EndpointDraft) -> Result<NormalizedEndpoint, DockerError> {
    let name = clean_name(&draft.name)?;
    match draft.kind {
        EndpointKind::Local => {
            let socket_path = match optional_text(draft.socket_path) {
                Some(path) if valid_socket_path(&path, cfg!(windows)) => Some(path),
                Some(_) => return Err(DockerError::new("invalid_input")),
                None => None,
            };
            let name = name.unwrap_or_else(|| "Local".to_string());
            Ok(blank(draft.id, name, EndpointKind::Local, socket_path, None, None, None, None, None, None, None, false))
        }
        EndpointKind::Ssh => {
            let Some(host_id) = draft.ssh_host_id.filter(|id| *id > 0) else {
                return Err(DockerError::new("invalid_input"));
            };
            let remote = optional_text(draft.remote_socket).unwrap_or_else(|| "/var/run/docker.sock".to_string());
            if !valid_remote_socket(&remote) {
                return Err(DockerError::new("invalid_input"));
            }
            let name = name.unwrap_or_else(|| "SSH".to_string());
            Ok(blank(
                draft.id,
                name,
                EndpointKind::Ssh,
                None,
                Some(host_id),
                Some(remote),
                None,
                None,
                None,
                None,
                None,
                false,
            ))
        }
        EndpointKind::Tls => {
            let host = required_host(draft.host)?;
            let port = draft.port.unwrap_or(2376);
            if port == 0 {
                return Err(DockerError::new("invalid_input"));
            }
            let ca_path = required_path(draft.ca_path)?;
            let cert_path = required_path(draft.cert_path)?;
            let key_path = required_path(draft.key_path)?;
            let name = name.unwrap_or_else(|| host.clone());
            Ok(blank(
                draft.id,
                name,
                EndpointKind::Tls,
                None,
                None,
                None,
                Some(host),
                Some(port),
                Some(ca_path),
                Some(cert_path),
                Some(key_path),
                false,
            ))
        }
        EndpointKind::Tcp => {
            if !draft.insecure {
                return Err(DockerError::new("insecure_required"));
            }
            let host = required_host(draft.host)?;
            let port = draft.port.unwrap_or(2375);
            if port == 0 {
                return Err(DockerError::new("invalid_input"));
            }
            let name = name.unwrap_or_else(|| host.clone());
            Ok(blank(
                draft.id,
                name,
                EndpointKind::Tcp,
                None,
                None,
                None,
                Some(host),
                Some(port),
                None,
                None,
                None,
                true,
            ))
        }
    }
}

pub fn valid_image_ref(value: &str) -> bool {
    let value = value.trim();
    (1..=255).contains(&value.len())
        && !value.contains("..")
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.starts_with(':')
        && !value.starts_with('@')
        && value
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '.' | '_' | '-' | '/' | ':' | '@'))
}

pub fn valid_resource_name(value: &str) -> bool {
    let value = value.trim();
    (1..=128).contains(&value.len())
        && value
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '.' | '-'))
}

fn blank(
    id: Option<i64>,
    name: String,
    kind: EndpointKind,
    socket_path: Option<String>,
    ssh_host_id: Option<i64>,
    remote_socket: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    ca_path: Option<String>,
    cert_path: Option<String>,
    key_path: Option<String>,
    insecure: bool,
) -> NormalizedEndpoint {
    NormalizedEndpoint {
        id,
        name,
        kind,
        socket_path,
        ssh_host_id,
        remote_socket,
        host,
        port,
        ca_path,
        cert_path,
        key_path,
        insecure,
    }
}

fn clean_name(value: &str) -> Result<Option<String>, DockerError> {
    let name = value.trim();
    if name.is_empty() {
        return Ok(None);
    }
    if name.chars().count() > 128 {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(Some(name.to_string()))
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    })
}

fn required_host(value: Option<String>) -> Result<String, DockerError> {
    let Some(host) = optional_text(value) else {
        return Err(DockerError::new("invalid_input"));
    };
    if host.len() > 255
        || host
            .chars()
            .any(|char| char.is_whitespace() || matches!(char, '/' | ',' | '@' | '#' | '\\'))
    {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(host.trim_matches(|char| char == '[' || char == ']').to_string())
}

fn required_path(value: Option<String>) -> Result<String, DockerError> {
    let Some(path) = optional_text(value) else {
        return Err(DockerError::new("invalid_input"));
    };
    if path.len() > 1024 || path.chars().any(|char| char == '\0' || char.is_control()) {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(path)
}

pub fn valid_socket_path(path: &str, windows: bool) -> bool {
    if path.is_empty() || path.len() > 1024 || path.chars().any(|char| char == '\0' || char == '\n' || char == '\r') {
        return false;
    }
    if windows {
        let lower = path.to_ascii_lowercase();
        lower.starts_with("//./pipe/") || lower.starts_with(r"\\.\pipe\")
    } else {
        path.starts_with('/')
    }
}

pub fn valid_remote_socket(path: &str) -> bool {
    valid_socket_path(path, false) && !path.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::{EndpointDraft, EndpointKind, normalize, valid_image_ref, valid_socket_path};

    fn draft(kind: EndpointKind) -> EndpointDraft {
        EndpointDraft {
            id: None,
            name: "  ".into(),
            kind,
            socket_path: None,
            ssh_host_id: None,
            remote_socket: None,
            host: None,
            port: None,
            ca_path: None,
            cert_path: None,
            key_path: None,
            insecure: false,
        }
    }

    #[test]
    fn plain_tcp_requires_an_explicit_warning() {
        let mut input = draft(EndpointKind::Tcp);
        input.host = Some("10.0.0.8".into());
        assert_eq!(normalize(input).unwrap_err().code, "insecure_required");
    }

    #[test]
    fn plain_tcp_keeps_the_warning_when_confirmed() {
        let mut input = draft(EndpointKind::Tcp);
        input.host = Some(" [10.0.0.8] ".into());
        input.insecure = true;
        let endpoint = normalize(input).unwrap();
        assert_eq!(endpoint.host.as_deref(), Some("10.0.0.8"));
        assert_eq!(endpoint.port, Some(2375));
        assert!(endpoint.insecure);
        assert_eq!(endpoint.name, "10.0.0.8");
    }

    #[test]
    fn ssh_defaults_the_remote_socket() {
        let mut input = draft(EndpointKind::Ssh);
        input.ssh_host_id = Some(4);
        input.name = "edge".into();
        let endpoint = normalize(input).unwrap();
        assert_eq!(endpoint.remote_socket.as_deref(), Some("/var/run/docker.sock"));
        assert_eq!(endpoint.ssh_host_id, Some(4));
    }

    #[test]
    fn image_refs_reject_spaces_and_traversal() {
        assert!(valid_image_ref("nginx:latest"));
        assert!(valid_image_ref("ghcr.io/org/app@sha256:abcdef"));
        assert!(!valid_image_ref("nginx latest"));
        assert!(!valid_image_ref("../nginx"));
        assert!(!valid_image_ref(""));
    }

    #[test]
    fn socket_paths_follow_the_platform_shape() {
        assert!(valid_socket_path("/var/run/docker.sock", false));
        assert!(!valid_socket_path("docker.sock", false));
        assert!(valid_socket_path(r"\\.\pipe\docker_engine", true));
        assert!(valid_socket_path("//./pipe/docker_engine", true));
        assert!(!valid_socket_path("/var/run/docker.sock", true));
    }
}
