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
    let has_version = root.get("version").and_then(Value::as_str).is_some_and(compose_version);
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
        let Some(name) = container.labels.get(PROJECT_LABEL).map(|value| value.trim()).filter(|value| !value.is_empty()) else {
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
            entry.config_path = clip(container.labels.get(CONFIG_LABEL).map(String::as_str).unwrap_or(""));
        }
        if entry.working_dir.is_empty() {
            entry.working_dir = clip(container.labels.get(WORKDIR_LABEL).map(String::as_str).unwrap_or(""));
        }
        if let Some(service) = container.labels.get(SERVICE_LABEL).map(|value| value.trim()).filter(|value| !value.is_empty()) {
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

pub fn merge_projects(saved: &[SavedCompose], recognized: &[RecognizedProject]) -> Vec<ComposeProject> {
    let mut used = vec![false; recognized.len()];
    let mut rows = Vec::new();
    for item in saved {
        let matched = recognized.iter().enumerate().find(|(index, project)| {
            !used[*index] && same_project(item, project)
        });
        if let Some((index, project)) = matched {
            used[index] = true;
            rows.push(ComposeProject {
                id: Some(item.id),
                name: item.name.clone(),
                config_path: prefer(&item.config_path, &project.config_path),
                working_dir: prefer(&item.working_dir, &project.working_dir),
                services: if item.services.is_empty() { names_only(&project.services) } else { item.services.clone() },
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
        let parsed: ServiceBody = serde_json::from_value(Value::Object(body.clone())).map_err(|_| DockerError::new("not_compose"))?;
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
        if label.is_empty() { None } else { Some(clip(&label)) }
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
        assert_eq!(preview.services[0].ports, vec!["8080:80".to_string(), "1053->53/udp".to_string()]);
    }

    #[test]
    fn rejects_a_file_that_is_not_compose() {
        let error = parse_compose("title: notes\nitems:\n  - a\n", Path::new("/tmp/notes.yaml")).unwrap_err();
        assert_eq!(error.code, "not_compose");
    }

    #[test]
    fn include_only_file_is_still_compose() {
        let preview = parse_compose("include:\n  - path: extra.yaml\n", Path::new("/work/app/compose.yaml")).unwrap();
        assert_eq!(preview.name, "app");
        assert!(preview.services.is_empty());
    }

    #[test]
    fn groups_running_containers_by_compose_project() {
        let projects = recognize_projects(&[
            labeled("web", "running", true),
            labeled("worker", "running", false),
            LabeledContainer { labels: HashMap::new(), running: true },
        ]);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "demo");
        assert_eq!(projects[0].services, vec!["web".to_string(), "worker".to_string()]);
        assert_eq!(projects[0].running, 1);
        assert_eq!(projects[0].total, 2);
        assert_eq!(projects[0].config_path, "/work/demo/compose.yaml");
    }

    fn labeled(service: &str, _state: &str, running: bool) -> LabeledContainer {
        let mut labels = HashMap::new();
        labels.insert(PROJECT_LABEL.to_string(), "demo".to_string());
        labels.insert(SERVICE_LABEL.to_string(), service.to_string());
        labels.insert(CONFIG_LABEL.to_string(), "/work/demo/compose.yaml".to_string());
        labels.insert(WORKDIR_LABEL.to_string(), "/work/demo".to_string());
        LabeledContainer { labels, running }
    }
}
