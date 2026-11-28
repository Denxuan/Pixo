use serde::{Deserialize, Serialize};

use super::error::DockerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointKind {
    Local,
    Ssh,
}

impl EndpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ssh => "ssh",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "ssh" => Some(Self::Ssh),
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
            Ok(blank(
                draft.id,
                name,
                EndpointKind::Local,
                socket_path,
                None,
                None,
                None,
                None,
            ))
        }
        EndpointKind::Ssh => {
            let Some(host_id) = draft.ssh_host_id.filter(|id| *id > 0) else {
                return Err(DockerError::new("invalid_input"));
            };
            let remote = optional_text(draft.remote_socket)
                .unwrap_or_else(|| "/var/run/docker.sock".to_string());
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
        && value.chars().all(|char| {
            char.is_ascii_alphanumeric() || matches!(char, '.' | '_' | '-' | '/' | ':' | '@')
        })
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

pub fn valid_socket_path(path: &str, windows: bool) -> bool {
    if path.is_empty()
        || path.len() > 1024
        || path
            .chars()
            .any(|char| char == '\0' || char == '\n' || char == '\r')
    {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedPort {
    pub host: u16,
    pub container: u16,
    pub udp: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerUsage {
    pub cpu_percent: Option<f64>,
    pub memory_bytes: u64,
    pub memory_limit: Option<u64>,
}

pub fn valid_container_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (1..=128).contains(&value.len())
        && first.is_ascii_alphanumeric()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
}

pub fn split_command(value: &str) -> Result<Vec<String>, DockerError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if value.len() > 512 || value.chars().any(|ch| ch.is_control()) {
        return Err(DockerError::new("invalid_input"));
    }
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in value.chars() {
        match quote {
            Some(mark) if ch == mark => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    push_arg(&mut args, &mut current)?;
                }
            }
            None => current.push(ch),
        }
    }
    if quote.is_some() || (!current.is_empty() && push_arg(&mut args, &mut current).is_err()) {
        return Err(DockerError::new("invalid_input"));
    }
    if args.len() > 32 {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(args)
}

fn push_arg(args: &mut Vec<String>, current: &mut String) -> Result<(), DockerError> {
    if current.chars().count() > 256 {
        return Err(DockerError::new("invalid_input"));
    }
    args.push(std::mem::take(current));
    Ok(())
}

pub fn parse_publish_list(value: &str) -> Result<Vec<PublishedPort>, DockerError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if value.len() > 800 {
        return Err(DockerError::new("invalid_input"));
    }
    let mut ports = Vec::new();
    for part in value.split([',', ' ', '\n', '\t']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if ports.len() == 20 {
            return Err(DockerError::new("invalid_input"));
        }
        ports.push(parse_publish(part)?);
    }
    Ok(ports)
}

fn parse_publish(part: &str) -> Result<PublishedPort, DockerError> {
    let (body, udp) = if let Some(body) = part.strip_suffix("/udp") {
        (body, true)
    } else if let Some(body) = part.strip_suffix("/tcp") {
        (body, false)
    } else {
        (part, false)
    };
    let (host, container) = if let Some((host, container)) = body.rsplit_once(':') {
        (parse_port(host)?, parse_port(container)?)
    } else {
        let port = parse_port(body)?;
        (port, port)
    };
    Ok(PublishedPort {
        host,
        container,
        udp,
    })
}

fn parse_port(value: &str) -> Result<u16, DockerError> {
    if value.is_empty() || value.len() > 5 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(DockerError::new("invalid_input"));
    }
    let port = value
        .parse::<u16>()
        .map_err(|_| DockerError::new("invalid_input"))?;
    if port == 0 {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::{
        EndpointDraft, EndpointKind, normalize, parse_publish_list, split_command, valid_image_ref,
        valid_socket_path,
    };

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
        }
    }

    #[test]
    fn ssh_defaults_the_remote_socket() {
        let mut input = draft(EndpointKind::Ssh);
        input.ssh_host_id = Some(4);
        input.name = "edge".into();
        let endpoint = normalize(input).unwrap();
        assert_eq!(
            endpoint.remote_socket.as_deref(),
            Some("/var/run/docker.sock")
        );
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

    #[test]
    fn publish_list_and_command_split() {
        let ports = parse_publish_list("8080:80, 53:53/udp 443").unwrap();
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].host, 8080);
        assert_eq!(ports[0].container, 80);
        assert!(!ports[0].udp);
        assert!(ports[1].udp);
        assert_eq!(ports[2].host, 443);
        assert_eq!(ports[2].container, 443);
        assert!(parse_publish_list("0:80").is_err());
        assert_eq!(
            split_command(r#"sh -c "echo hi""#).unwrap(),
            vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()]
        );
        assert!(split_command("\"unterminated").is_err());
        assert!(split_command("").unwrap().is_empty());
    }
}
