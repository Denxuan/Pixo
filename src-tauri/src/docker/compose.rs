use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::error::DockerError;

const MAX_COMPOSE_BYTES: u64 = 1024 * 1024;
const MAX_SERVICES: usize = 200;
const PROJECT_LABEL: &str = "com.docker.compose.project";
const CONFIG_LABEL: &str = "com.docker.compose.project.config_files";
const WORKDIR_LABEL: &str = "com.docker.compose.project.working_dir";
const SERVICE_LABEL: &str = "com.docker.compose.service";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeService {
    pub name: String,
    pub image: String,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposePreview {
    pub name: String,
    pub config_path: String,
    pub working_dir: String,
    pub services: Vec<ComposeService>,
}

#[derive(Debug, Clone)]
pub struct LabeledContainer {
    pub labels: HashMap<String, String>,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognizedProject {
    pub name: String,
    pub config_path: String,
    pub working_dir: String,
    pub services: Vec<String>,
    pub running: i64,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct SavedCompose {
    pub id: i64,
    pub name: String,
    pub config_path: String,
    pub working_dir: String,
    pub services: Vec<ComposeService>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeProject {
    pub id: Option<i64>,
    pub name: String,
    pub config_path: String,
    pub working_dir: String,
    pub services: Vec<ComposeService>,
    pub running: i64,
    pub total: i64,
    pub imported: bool,
    pub origin: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeDraft {
    pub name: String,
    pub services: Vec<ComposeService>,
}

pub fn render_compose(draft: &ComposeDraft) -> Result<String, DockerError> {
    let name = draft.name.trim();
    if !valid_project_name(name) || draft.services.is_empty() || draft.services.len() > MAX_SERVICES
    {
        return Err(DockerError::new("invalid_input"));
    }
    let mut seen = BTreeSet::new();
    let mut services = serde_json::Map::new();
    for service in &draft.services {
        let service_name = service.name.trim();
        let image = service.image.trim();
        if !valid_service_name(service_name)
            || !valid_image(image)
            || !seen.insert(service_name.to_string())
        {
            return Err(DockerError::new("invalid_input"));
        }
        let mut ports = Vec::new();
        for port in &service.ports {
            let port = port.trim();
            if port.is_empty() {
                continue;
            }
            if !valid_port(port) || ports.len() == 20 {
                return Err(DockerError::new("invalid_input"));
            }
            ports.push(Value::String(port.to_string()));
        }
        let mut body = serde_json::Map::new();
        body.insert("image".to_string(), Value::String(image.to_string()));
        if !ports.is_empty() {
            body.insert("ports".to_string(), Value::Array(ports));
        }
        services.insert(service_name.to_string(), Value::Object(body));
    }
    let mut root = serde_json::Map::new();
    root.insert("name".to_string(), Value::String(name.to_string()));
    root.insert("services".to_string(), Value::Object(services));
    let text = serde_yml::to_string(&Value::Object(root))
        .map_err(|_| DockerError::new("invalid_input"))?;
    if text.len() > MAX_COMPOSE_BYTES as usize {
        return Err(DockerError::new("invalid_input"));
    }
    let _ = parse_compose(&text, Path::new("compose.yaml"))?;
    Ok(text)
}

pub fn install_project_file(
    directory: &Path,
    text: &str,
) -> Result<std::path::PathBuf, DockerError> {
    if text.len() > MAX_COMPOSE_BYTES as usize || text.trim().is_empty() {
        return Err(DockerError::new("invalid_input"));
    }
    if !directory.is_dir() {
        return Err(DockerError::new("not_found"));
    }
    let destination = directory.join("docker-compose.yml");
    if destination.exists() {
        return Err(DockerError::new("compose_exists"));
    }
    write_compose_file(&destination, text)?;
    Ok(destination)
}

pub fn install_host_compose(
    directory: &Path,
    text: &str,
) -> Result<std::path::PathBuf, DockerError> {
    if directory.as_os_str().is_empty() {
        return Err(DockerError::new("invalid_input"));
    }
    if directory.exists() {
        if !directory.is_dir() {
            return Err(DockerError::new("invalid_input"));
        }
    } else if fs::create_dir_all(directory).is_err() {
        return Err(DockerError::new("compose_dir"));
    }
    install_project_file(directory, text)
}

#[derive(Clone, Copy)]
pub enum HostPathStyle {
    Unix,
    Windows,
}

pub fn host_directory(value: &str, style: HostPathStyle) -> Result<String, DockerError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 1024
        || value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r'))
    {
        return Err(DockerError::new("invalid_input"));
    }
    match style {
        HostPathStyle::Unix => normalize_unix_dir(value),
        HostPathStyle::Windows => normalize_windows_dir(value),
    }
}

pub fn shell_quote(value: &str) -> Result<String, DockerError> {
    if value.is_empty()
        || value.len() > 1024
        || value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r'))
    {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeAction {
    Build,
    Up,
    Down,
    Logs,
}

impl ComposeAction {
    pub fn parse(value: &str) -> Result<Self, DockerError> {
        match value {
            "build" => Ok(Self::Build),
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            "logs" => Ok(Self::Logs),
            _ => Err(DockerError::new("invalid_input")),
        }
    }

    pub fn failure_code(self) -> &'static str {
        match self {
            Self::Build => "compose_build_failed",
            Self::Up => "compose_up_failed",
            Self::Down => "compose_down_failed",
            Self::Logs => "compose_logs_failed",
        }
    }
}

#[derive(Debug)]
pub struct ComposeLocation {
    pub directory: String,
    pub file: String,
}

pub fn compose_location(
    config_path: &str,
    working_dir: &str,
    style: HostPathStyle,
) -> Result<ComposeLocation, DockerError> {
    let raw = config_path.split(',').next().unwrap_or("").trim();
    if raw.is_empty() || raw.starts_with("engine://") {
        return Err(DockerError::new("compose_file_missing"));
    }
    let file = host_file(raw, style)?;
    let directory = if working_dir.trim().is_empty() {
        parent_directory(&file, style)?
    } else {
        host_directory(working_dir.trim(), style)?
    };
    Ok(ComposeLocation { directory, file })
}

pub fn compose_remote_command(
    action: ComposeAction,
    name: &str,
    location: &ComposeLocation,
) -> Result<String, DockerError> {
    if !valid_project_name(name) {
        return Err(DockerError::new("invalid_input"));
    }
    let mut command = format!(
        "cd {} && docker compose --project-name {} --file {}",
        shell_quote(&location.directory)?,
        shell_quote(name)?,
        shell_quote(&location.file)?,
    );
    for arg in action_args(action) {
        command.push(' ');
        command.push_str(&shell_quote(arg)?);
    }
    Ok(command)
}

fn action_args(action: ComposeAction) -> &'static [&'static str] {
    match action {
        ComposeAction::Build => &["build"],
        ComposeAction::Up => &["up", "--detach"],
        ComposeAction::Down => &["down"],
        ComposeAction::Logs => &["logs", "--tail", "200", "--follow", "--no-color"],
    }
}

fn host_file(value: &str, style: HostPathStyle) -> Result<String, DockerError> {
    if value.len() > 1024 || value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r')) {
        return Err(DockerError::new("invalid_input"));
    }
    let (parent, name) = split_file(value).ok_or_else(|| DockerError::new("invalid_input"))?;
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err(DockerError::new("invalid_input"));
    }
    let _ = host_directory(parent, style)?;
    match style {
        HostPathStyle::Unix if value.contains('\\') || !value.starts_with('/') => {
            Err(DockerError::new("invalid_input"))
        }
        HostPathStyle::Windows if value.ends_with(['\\', '/']) => {
            Err(DockerError::new("invalid_input"))
        }
        _ => Ok(value.to_string()),
    }
}

fn parent_directory(file: &str, style: HostPathStyle) -> Result<String, DockerError> {
    let (parent, _) = split_file(file).ok_or_else(|| DockerError::new("invalid_input"))?;
    host_directory(parent, style)
}

fn split_file(value: &str) -> Option<(&str, &str)> {
    let index = value.rfind(['/', '\\'])?;
    let parent = &value[..index];
    let name = &value[index + 1..];
    if parent.is_empty() || name.is_empty() {
        None
    } else {
        Some((parent, name))
    }
}

fn normalize_unix_dir(value: &str) -> Result<String, DockerError> {
    if !value.starts_with('/') || value.contains('\\') {
        return Err(DockerError::new("invalid_input"));
    }
    let trimmed = value.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(DockerError::new("invalid_input"));
    }
    if trimmed
        .split('/')
        .skip(1)
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(trimmed.to_string())
}

fn normalize_windows_dir(value: &str) -> Result<String, DockerError> {
    let bytes = value.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    let unc = value.starts_with("\\\\") && !value.starts_with("\\\\.\\");
    if !drive && !unc {
        return Err(DockerError::new("invalid_input"));
    }
    let trimmed = value.trim_end_matches(['\\', '/']);
    if drive && trimmed.len() <= 2 {
        return Err(DockerError::new("invalid_input"));
    }
    if unc && trimmed.matches('\\').count() < 3 {
        return Err(DockerError::new("invalid_input"));
    }
    if trimmed
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .any(|part| part == "." || part == "..")
    {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(trimmed.to_string())
}

pub fn docker_host_value(kind: &str, address: &str) -> Result<String, DockerError> {
    match kind {
        "local" => {
            if address.contains('\\')
                || address.contains("//./pipe/")
                || address.contains(r"\\.\pipe\")
            {
                let name = address
                    .rsplit(['\\', '/'])
                    .find(|part| !part.is_empty())
                    .ok_or_else(|| DockerError::new("invalid_input"))?;
                Ok(format!("npipe:////./pipe/{name}"))
            } else if let Some(path) = address.strip_prefix("unix://") {
                if path.is_empty() {
                    Err(DockerError::new("invalid_input"))
                } else {
                    Ok(address.to_string())
                }
            } else if address.starts_with('/') {
                Ok(format!("unix://{address}"))
            } else {
                Err(DockerError::new("invalid_input"))
            }
        }
        "ssh" => Err(DockerError::new("compose_start_unsupported")),
        _ => Err(DockerError::new("invalid_input")),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeProjectRequest {
    pub name: String,
    pub directory: String,
    pub source: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub yaml: String,
    #[serde(default)]
    pub start: bool,
}

pub fn write_compose_file(path: &Path, text: &str) -> Result<(), DockerError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("yaml") && !extension.eq_ignore_ascii_case("yml") {
        return Err(DockerError::new("invalid_input"));
    }
    if text.len() > MAX_COMPOSE_BYTES as usize || path.is_dir() {
        return Err(DockerError::new("invalid_input"));
    }
    let parent = path
        .parent()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| DockerError::new("invalid_input"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("compose.yaml");
    let temporary = parent.join(format!(".{file_name}.pixo.tmp"));
    if fs::write(&temporary, text).is_err() {
        return Err(DockerError::new("invalid_input"));
    }
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(DockerError::new("invalid_input"));
    }
    Ok(())
}

pub(crate) fn valid_project_name(value: &str) -> bool {
    let count = value.chars().count();
    (1..=128).contains(&count)
        && !value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
}

fn valid_service_name(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    value.len() <= 63 && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn valid_image(value: &str) -> bool {
    let count = value.chars().count();
    (1..=255).contains(&count)
        && !value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
}

fn valid_port(value: &str) -> bool {
    let mut parts = value.split('/');
    let address = parts.next().unwrap_or("");
    let protocol = parts.next();
    if parts.next().is_some() {
        return false;
    }
    if let Some(protocol) = protocol {
        if protocol != "tcp" && protocol != "udp" {
            return false;
        }
    }
    let segments: Vec<&str> = address
        .split(':')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() || segments.len() > 3 || segments.len() != address.split(':').count() {
        return false;
    }
    segments.iter().enumerate().all(|(index, segment)| {
        if index == 0 && segment.contains('.') {
            return segment.split('.').all(|part| {
                !part.is_empty() && part.len() <= 3 && part.chars().all(|ch| ch.is_ascii_digit())
            });
        }
        !segment.is_empty() && segment.len() <= 5 && segment.chars().all(|ch| ch.is_ascii_digit())
    })
}

pub fn load_compose_file(path: &Path) -> Result<ComposePreview, DockerError> {
    let meta = fs::metadata(path).map_err(|_| DockerError::new("not_found"))?;
    if !meta.is_file() || meta.len() > MAX_COMPOSE_BYTES {
        return Err(DockerError::new("invalid_input"));
    }
    let text = fs::read_to_string(path).map_err(|_| DockerError::new("invalid_input"))?;
    let mut preview = parse_compose(&text, path)?;
    preview.config_path = path.to_string_lossy().into_owned();
    preview.working_dir = path
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(preview)
}

pub fn parse_compose(text: &str, path: &Path) -> Result<ComposePreview, DockerError> {
    let value: Value = serde_yml::from_str(text).map_err(|_| DockerError::new("not_compose"))?;
    let Some(root) = value.as_object() else {
        return Err(DockerError::new("not_compose"));
    };
    let services_value = root.get("services").cloned();
    let has_include = root.contains_key("include");
    let has_version = root
        .get("version")
        .and_then(Value::as_str)
        .is_some_and(compose_version);
    if services_value.is_none() && !has_include && !has_version {
        return Err(DockerError::new("not_compose"));
    }
    let services = match services_value {
        Some(Value::Object(map)) => read_services(&map)?,
        Some(_) => return Err(DockerError::new("not_compose")),
        None => Vec::new(),
    };
    let declared = root.get("name").and_then(Value::as_str);
    Ok(ComposePreview {
        name: project_name(declared, path),
        config_path: path.to_string_lossy().into_owned(),
        working_dir: String::new(),
        services,
    })
}

pub fn recognize_projects(containers: &[LabeledContainer]) -> Vec<RecognizedProject> {
    let mut grouped: BTreeMap<String, ProjectBuild> = BTreeMap::new();
    for container in containers {
        let Some(name) = container
            .labels
            .get(PROJECT_LABEL)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if name.chars().count() > 128 {
            continue;
        }
        let entry = grouped.entry(name.to_string()).or_default();
        entry.total += 1;
        if container.running {
            entry.running += 1;
        }
        if entry.config_path.is_empty() {
            entry.config_path = clip(
                container
                    .labels
                    .get(CONFIG_LABEL)
                    .map(String::as_str)
                    .unwrap_or(""),
            );
        }
        if entry.working_dir.is_empty() {
            entry.working_dir = clip(
                container
                    .labels
                    .get(WORKDIR_LABEL)
                    .map(String::as_str)
                    .unwrap_or(""),
            );
        }
        if let Some(service) = container
            .labels
            .get(SERVICE_LABEL)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            entry.services.insert(clip(service));
        }
    }
    grouped
        .into_iter()
        .map(|(name, build)| RecognizedProject {
            name,
            config_path: build.config_path,
            working_dir: build.working_dir,
            services: build.services.into_iter().collect(),
            running: build.running,
            total: build.total,
        })
        .collect()
}

pub fn merge_projects(
    saved: &[SavedCompose],
    recognized: &[RecognizedProject],
) -> Vec<ComposeProject> {
    let mut used = vec![false; recognized.len()];
    let mut rows = Vec::new();
    for item in saved {
        let matched = recognized
            .iter()
            .enumerate()
            .find(|(index, project)| !used[*index] && same_project(item, project));
        if let Some((index, project)) = matched {
            used[index] = true;
            rows.push(ComposeProject {
                id: Some(item.id),
                name: item.name.clone(),
                config_path: prefer(&item.config_path, &project.config_path),
                working_dir: prefer(&item.working_dir, &project.working_dir),
                services: if item.services.is_empty() {
                    names_only(&project.services)
                } else {
                    item.services.clone()
                },
                running: project.running,
                total: project.total,
                imported: true,
                origin: "both".to_string(),
            });
        } else {
            rows.push(ComposeProject {
                id: Some(item.id),
                name: item.name.clone(),
                config_path: item.config_path.clone(),
                working_dir: item.working_dir.clone(),
                services: item.services.clone(),
                running: 0,
                total: 0,
                imported: true,
                origin: "file".to_string(),
            });
        }
    }
    for (project, seen) in recognized.iter().zip(used) {
        if seen {
            continue;
        }
        rows.push(ComposeProject {
            id: None,
            name: project.name.clone(),
            config_path: project.config_path.clone(),
            working_dir: project.working_dir.clone(),
            services: names_only(&project.services),
            running: project.running,
            total: project.total,
            imported: false,
            origin: "engine".to_string(),
        });
    }
    rows.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    rows
}

pub fn project_to_keep(
    labels: &HashMap<String, String>,
    recognized: &[RecognizedProject],
    saved: &[SavedCompose],
    hidden: &[(String, String)],
) -> Option<RecognizedProject> {
    let name = labels
        .get(PROJECT_LABEL)
        .map(|value| value.trim())
        .filter(|value| valid_project_name(value))?;
    let known = recognized
        .iter()
        .find(|item| item.name.eq_ignore_ascii_case(name));
    let config_path = known
        .map(|item| item.config_path.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            labels
                .get(CONFIG_LABEL)
                .map(|value| clip(value.trim()))
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default();
    let file = config_path.split(',').next().unwrap_or("").trim();
    if file.is_empty() || file.starts_with("engine://") {
        return None;
    }
    if is_hidden(hidden, name, &config_path) || is_hidden(hidden, name, file) {
        return None;
    }
    let already_saved = saved.iter().any(|item| {
        let usable = !item.config_path.is_empty() && !item.config_path.starts_with("engine://");
        usable
            && (item.name.eq_ignore_ascii_case(name)
                || item.config_path == config_path
                || item.config_path == file)
    });
    if already_saved {
        return None;
    }
    let working_dir = known
        .map(|item| item.working_dir.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            labels
                .get(WORKDIR_LABEL)
                .map(|value| clip(value.trim()))
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default();
    let mut services = known.map(|item| item.services.clone()).unwrap_or_default();
    if let Some(service) = labels
        .get(SERVICE_LABEL)
        .map(|value| clip(value.trim()))
        .filter(|value| !value.is_empty())
    {
        if !services
            .iter()
            .any(|item| item.eq_ignore_ascii_case(&service))
        {
            services.push(service);
        }
    }
    Some(RecognizedProject {
        name: known
            .map(|item| item.name.clone())
            .unwrap_or_else(|| name.to_string()),
        config_path,
        working_dir,
        services,
        running: 0,
        total: 0,
    })
}

pub fn is_hidden(hidden: &[(String, String)], name: &str, config_path: &str) -> bool {
    hidden.iter().any(|(hidden_name, hidden_path)| {
        let name_match = !hidden_name.is_empty() && hidden_name.eq_ignore_ascii_case(name);
        let path_match = !hidden_path.is_empty()
            && !config_path.is_empty()
            && !hidden_path.starts_with("engine://")
            && !config_path.starts_with("engine://")
            && hidden_path == config_path;
        name_match || path_match
    })
}

fn same_project(saved: &SavedCompose, project: &RecognizedProject) -> bool {
    let same_name = !saved.name.is_empty() && saved.name.eq_ignore_ascii_case(&project.name);
    let same_path = !saved.config_path.is_empty()
        && !project.config_path.is_empty()
        && saved.config_path == project.config_path;
    same_name || same_path
}

fn prefer(saved: &str, live: &str) -> String {
    if !saved.is_empty() && !saved.starts_with("engine://") {
        saved.to_string()
    } else if !live.is_empty() {
        live.to_string()
    } else {
        saved.to_string()
    }
}

fn names_only(names: &[String]) -> Vec<ComposeService> {
    names
        .iter()
        .map(|name| ComposeService {
            name: name.clone(),
            image: String::new(),
            ports: Vec::new(),
        })
        .collect()
}

#[derive(Default)]
struct ProjectBuild {
    config_path: String,
    working_dir: String,
    services: BTreeSet<String>,
    running: i64,
    total: i64,
}

fn read_services(map: &serde_json::Map<String, Value>) -> Result<Vec<ComposeService>, DockerError> {
    if map.len() > MAX_SERVICES {
        return Err(DockerError::new("invalid_input"));
    }
    let mut services = Vec::new();
    for (name, value) in map {
        let name = clip(name.trim());
        if name.is_empty() {
            continue;
        }
        let Some(body) = value.as_object() else {
            return Err(DockerError::new("not_compose"));
        };
        let parsed: ServiceBody = serde_json::from_value(Value::Object(body.clone()))
            .map_err(|_| DockerError::new("not_compose"))?;
        services.push(ComposeService {
            name,
            image: clip(parsed.image.unwrap_or_default().trim()),
            ports: parsed.ports.map(PortsField::labels).unwrap_or_default(),
        });
    }
    Ok(services)
}

fn project_name(declared: Option<&str>, path: &Path) -> String {
    if let Some(name) = declared.map(str::trim).filter(|name| !name.is_empty()) {
        return clip(name);
    }
    let fallback = path
        .parent()
        .and_then(Path::file_name)
        .or_else(|| path.file_stem())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "compose".to_string());
    clip(fallback.trim())
}

fn compose_version(value: &str) -> bool {
    let value = value.trim();
    value == "2" || value == "3" || value.starts_with("2.") || value.starts_with("3.")
}

fn clip(value: &str) -> String {
    value.chars().take(256).collect()
}

#[derive(Debug, Deserialize)]
struct ServiceBody {
    image: Option<String>,
    ports: Option<PortsField>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PortsField {
    Many(Vec<PortSpec>),
    One(PortSpec),
}

impl PortsField {
    fn labels(self) -> Vec<String> {
        match self {
            Self::Many(items) => items.into_iter().filter_map(|item| item.label()).collect(),
            Self::One(item) => item.label().into_iter().collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PortSpec {
    Text(String),
    Number(u64),
    Map(BTreeMap<String, Value>),
}

impl PortSpec {
    fn label(self) -> Option<String> {
        let label = match self {
            Self::Text(value) => value.trim().to_string(),
            Self::Number(value) => value.to_string(),
            Self::Map(map) => {
                let target = json_text(map.get("target"));
                let published = json_text(map.get("published"));
                let protocol = json_text(map.get("protocol")).unwrap_or_else(|| "tcp".to_string());
                match (published, target) {
                    (Some(published), Some(target)) => format!("{published}->{target}/{protocol}"),
                    (None, Some(target)) => format!("{target}/{protocol}"),
                    (Some(published), None) => format!("{published}/{protocol}"),
                    (None, None) => return None,
                }
            }
        };
        if label.is_empty() {
            None
        } else {
            Some(clip(&label))
        }
    }
}

fn json_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_services_ports_and_directory_name() {
        let text = r#"
services:
  web:
    image: nginx:latest
    ports:
      - "8080:80"
      - target: 53
        published: "1053"
        protocol: udp
  worker:
    image: busybox
"#;
        let preview = parse_compose(text, Path::new("/work/demo/compose.yaml")).unwrap();
        assert_eq!(preview.name, "demo");
        assert_eq!(preview.services.len(), 2);
        assert_eq!(preview.services[0].image, "nginx:latest");
        assert_eq!(
            preview.services[0].ports,
            vec!["8080:80".to_string(), "1053->53/udp".to_string()]
        );
    }

    #[test]
    fn renders_a_compose_file_that_parses_back() {
        let draft = ComposeDraft {
            name: "demo".to_string(),
            services: vec![ComposeService {
                name: "web".to_string(),
                image: "nginx:latest".to_string(),
                ports: vec!["8080:80".to_string(), "  ".to_string()],
            }],
        };
        let text = render_compose(&draft).unwrap();
        let preview = parse_compose(&text, Path::new("/work/demo/compose.yaml")).unwrap();
        assert_eq!(preview.name, "demo");
        assert_eq!(preview.services[0].name, "web");
        assert_eq!(preview.services[0].image, "nginx:latest");
        assert_eq!(preview.services[0].ports, vec!["8080:80".to_string()]);
    }

    #[test]
    fn rejects_a_compose_draft_with_a_bad_service_name() {
        let draft = ComposeDraft {
            name: "demo".to_string(),
            services: vec![ComposeService {
                name: "-web".to_string(),
                image: "nginx".to_string(),
                ports: Vec::new(),
            }],
        };
        assert_eq!(render_compose(&draft).unwrap_err().code, "invalid_input");
    }

    #[test]
    fn installs_docker_compose_yml_and_refuses_a_second_copy() {
        let dir = std::env::temp_dir().join(format!("pixo-compose-project-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let text = "name: demo\nservices:\n  web:\n    image: nginx:latest\n";
        let path = install_project_file(&dir, text).unwrap();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("docker-compose.yml")
        );
        assert_eq!(
            install_project_file(&dir, text).unwrap_err().code,
            "compose_exists"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn formats_compose_docker_hosts() {
        assert_eq!(
            docker_host_value("local", "/var/run/docker.sock").unwrap(),
            "unix:///var/run/docker.sock"
        );
        assert_eq!(
            docker_host_value("local", r"\\.\pipe\docker_engine").unwrap(),
            "npipe:////./pipe/docker_engine"
        );
        assert_eq!(
            docker_host_value("ssh", "ignored").unwrap_err().code,
            "compose_start_unsupported"
        );
    }

    #[test]
    fn writes_a_compose_file_and_replaces_an_existing_one() {
        let dir = std::env::temp_dir().join(format!("pixo-compose-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("compose.yaml");
        write_compose_file(&path, "name: demo\nservices: {}\n").unwrap();
        write_compose_file(&path, "name: next\nservices: {}\n").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("name: next"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_a_file_that_is_not_compose() {
        let error = parse_compose(
            "title: notes\nitems:\n  - a\n",
            Path::new("/tmp/notes.yaml"),
        )
        .unwrap_err();
        assert_eq!(error.code, "not_compose");
    }

    #[test]
    fn include_only_file_is_still_compose() {
        let preview = parse_compose(
            "include:\n  - path: extra.yaml\n",
            Path::new("/work/app/compose.yaml"),
        )
        .unwrap();
        assert_eq!(preview.name, "app");
        assert!(preview.services.is_empty());
    }

    #[test]
    fn groups_running_containers_by_compose_project() {
        let projects = recognize_projects(&[
            labeled("web", "running", true),
            labeled("worker", "running", false),
            LabeledContainer {
                labels: HashMap::new(),
                running: true,
            },
        ]);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "demo");
        assert_eq!(
            projects[0].services,
            vec!["web".to_string(), "worker".to_string()]
        );
        assert_eq!(projects[0].running, 1);
        assert_eq!(projects[0].total, 2);
        assert_eq!(projects[0].config_path, "/work/demo/compose.yaml");
    }

    #[test]
    fn accepts_a_server_directory_and_rejects_a_relative_one() {
        assert_eq!(
            host_directory("/opt/apps/demo/", HostPathStyle::Unix).unwrap(),
            "/opt/apps/demo"
        );
        assert_eq!(
            host_directory(r"D:\compose\demo\", HostPathStyle::Windows).unwrap(),
            r"D:\compose\demo"
        );
        assert_eq!(
            host_directory("opt/demo", HostPathStyle::Unix)
                .unwrap_err()
                .code,
            "invalid_input"
        );
        assert_eq!(
            host_directory("/opt/../etc", HostPathStyle::Unix)
                .unwrap_err()
                .code,
            "invalid_input"
        );
        assert_eq!(
            host_directory("/", HostPathStyle::Unix).unwrap_err().code,
            "invalid_input"
        );
    }

    #[test]
    fn quotes_a_remote_compose_path() {
        assert_eq!(shell_quote("/opt/my app").unwrap(), "'/opt/my app'");
        assert_eq!(shell_quote("/opt/o'brien").unwrap(), "'/opt/o'\\''brien'");
    }

    #[test]
    fn locates_a_compose_file_and_builds_a_remote_command() {
        let location = compose_location(
            "/opt/demo/docker-compose.yml",
            "/opt/demo/",
            HostPathStyle::Unix,
        )
        .unwrap();
        assert_eq!(location.directory, "/opt/demo");
        assert_eq!(location.file, "/opt/demo/docker-compose.yml");
        let command = compose_remote_command(ComposeAction::Up, "demo", &location).unwrap();
        assert_eq!(
            command,
            "cd '/opt/demo' && docker compose --project-name 'demo' --file '/opt/demo/docker-compose.yml' 'up' '--detach'"
        );
        let down = compose_remote_command(ComposeAction::Down, "demo", &location).unwrap();
        assert!(down.ends_with("'down'"));
    }

    #[test]
    fn derives_the_directory_and_rejects_a_missing_compose_file() {
        let location = compose_location("/opt/demo/compose.yaml", "", HostPathStyle::Unix).unwrap();
        assert_eq!(location.directory, "/opt/demo");
        assert_eq!(
            compose_location("engine://demo", "/opt/demo", HostPathStyle::Unix)
                .unwrap_err()
                .code,
            "compose_file_missing"
        );
        assert_eq!(
            compose_location("demo/compose.yaml", "", HostPathStyle::Unix)
                .unwrap_err()
                .code,
            "invalid_input"
        );
        assert_eq!(
            compose_location("/opt/../etc/compose.yaml", "", HostPathStyle::Unix)
                .unwrap_err()
                .code,
            "invalid_input"
        );
    }

    #[test]
    fn accepts_a_windows_compose_file() {
        let location = compose_location(
            r"D:\apps\demo\docker-compose.yml",
            r"D:\apps\demo\",
            HostPathStyle::Windows,
        )
        .unwrap();
        assert_eq!(location.directory, r"D:\apps\demo");
        assert_eq!(location.file, r"D:\apps\demo\docker-compose.yml");
        let command = compose_remote_command(ComposeAction::Logs, "demo", &location).unwrap();
        assert!(command.contains("'logs' '--tail' '200' '--follow' '--no-color'"));
    }

    #[test]
    fn creates_the_host_directory_before_writing_compose() {
        let dir = std::env::temp_dir().join(format!(
            "pixo-compose-host-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        let text = "name: demo\nservices:\n  web:\n    image: nginx:latest\n";
        let path = install_host_compose(&dir, text).unwrap();
        assert!(path.ends_with("docker-compose.yml"));
        assert_eq!(
            install_host_compose(&dir, text).unwrap_err().code,
            "compose_exists"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_a_container_keeps_its_compose_project() {
        let mut labels = HashMap::new();
        labels.insert(PROJECT_LABEL.to_string(), "redis".to_string());
        labels.insert(SERVICE_LABEL.to_string(), "redis".to_string());
        labels.insert(
            CONFIG_LABEL.to_string(),
            "/opt/zdx/redis/docker-compose.yml".to_string(),
        );
        labels.insert(WORKDIR_LABEL.to_string(), "/opt/zdx/redis".to_string());
        let kept = project_to_keep(&labels, &[], &[], &[]).unwrap();
        assert_eq!(kept.name, "redis");
        assert_eq!(kept.config_path, "/opt/zdx/redis/docker-compose.yml");
        assert_eq!(kept.working_dir, "/opt/zdx/redis");
        assert_eq!(kept.services, vec!["redis".to_string()]);

        let recognized = vec![RecognizedProject {
            name: "rocketmq".to_string(),
            config_path: "/opt/zdx/rocketmq/docker-compose.yaml".to_string(),
            working_dir: "/opt/zdx/rocketmq".to_string(),
            services: vec![
                "broker".to_string(),
                "namesrv".to_string(),
                "rocketmq-dashboard".to_string(),
            ],
            running: 3,
            total: 3,
        }];
        labels.insert(PROJECT_LABEL.to_string(), "rocketmq".to_string());
        labels.insert(SERVICE_LABEL.to_string(), "broker".to_string());
        let kept = project_to_keep(&labels, &recognized, &[], &[]).unwrap();
        assert_eq!(kept.services.len(), 3);

        let saved = vec![SavedCompose {
            id: 1,
            name: "redis".to_string(),
            config_path: "/opt/zdx/redis/docker-compose.yml".to_string(),
            working_dir: "/opt/zdx/redis".to_string(),
            services: Vec::new(),
        }];
        labels.insert(PROJECT_LABEL.to_string(), "redis".to_string());
        assert!(project_to_keep(&labels, &[], &saved, &[]).is_none());
        let hidden = vec![("redis".to_string(), String::new())];
        assert!(project_to_keep(&labels, &[], &[], &hidden).is_none());
        labels.remove(PROJECT_LABEL);
        assert!(project_to_keep(&labels, &[], &[], &[]).is_none());
    }

    #[test]
    fn hiding_a_project_matches_its_name_or_file_and_ignores_others() {
        let hidden = vec![
            ("demo".to_string(), String::new()),
            (String::new(), "/work/demo/compose.yaml".to_string()),
        ];
        assert!(is_hidden(&hidden, "Demo", ""));
        assert!(is_hidden(&hidden, "other", "/work/demo/compose.yaml"));
        assert!(!is_hidden(&hidden, "other", "/work/other/compose.yaml"));
        assert!(!is_hidden(&hidden, "other", "engine://other"));
    }

    fn labeled(service: &str, _state: &str, running: bool) -> LabeledContainer {
        let mut labels = HashMap::new();
        labels.insert(PROJECT_LABEL.to_string(), "demo".to_string());
        labels.insert(SERVICE_LABEL.to_string(), service.to_string());
        labels.insert(
            CONFIG_LABEL.to_string(),
            "/work/demo/compose.yaml".to_string(),
        );
        labels.insert(WORKDIR_LABEL.to_string(), "/work/demo".to_string());
        LabeledContainer { labels, running }
    }
}
