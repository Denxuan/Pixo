use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::SshError;

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub alias: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: Option<String>,
    pub jumps: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPreview {
    pub index: u32,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: String,
    pub key_path: Option<String>,
    pub jump_names: Vec<String>,
    pub warnings: Vec<String>,
    pub selected: bool,
}

pub fn preview(entries: &[ConfigEntry], existing: &[(String, u16, String)]) -> Vec<ConfigPreview> {
    let known = existing
        .iter()
        .map(|(host, port, user)| (host.to_ascii_lowercase(), *port, user.clone()))
        .collect::<HashSet<_>>();
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let mut warnings = entry.warnings.clone();
            if known.contains(&(entry.host.to_ascii_lowercase(), entry.port, entry.username.clone())) {
                warnings.push("duplicate".to_string());
            }
            let selected = entry.username.trim().is_empty() == false
                && !warnings.iter().any(|warning| warning == "duplicate");
            ConfigPreview {
                index: index as u32,
                name: entry.alias.clone(),
                host: entry.host.clone(),
                port: entry.port,
                username: entry.username.clone(),
                auth: if entry.key_path.is_some() { "key" } else { "password" }.to_string(),
                key_path: entry.key_path.clone(),
                jump_names: entry.jumps.clone(),
                warnings,
                selected,
            }
        })
        .collect()
}

pub fn expand_selection(entries: &[ConfigEntry], selected: &[u32]) -> Vec<u32> {
    let by_alias = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.alias.to_ascii_lowercase(), index as u32))
        .collect::<HashMap<_, _>>();
    let mut chosen = selected.iter().copied().filter(|index| (*index as usize) < entries.len()).collect::<HashSet<_>>();
    let mut pending = chosen.iter().copied().collect::<Vec<_>>();
    while let Some(index) = pending.pop() {
        for jump in &entries[index as usize].jumps {
            if let Some(target) = by_alias.get(&jump.to_ascii_lowercase()) {
                if chosen.insert(*target) {
                    pending.push(*target);
                }
            }
        }
    }
    let mut indexes = chosen.into_iter().collect::<Vec<_>>();
    indexes.sort_unstable();
    indexes
}

pub fn load_user_config() -> Result<Vec<ConfigEntry>, SshError> {
    let Some(home) = home_dir() else {
        return Err(SshError::new("config_unreadable"));
    };
    let path = home.join(".ssh").join("config");
    if !path.is_file() {
        return Err(SshError::new("config_missing"));
    }
    let user = local_user();
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    read_file(&path, &home, &user, 0, &mut seen, &mut entries)?;
    Ok(entries)
}

fn read_file(
    path: &Path,
    home: &Path,
    user: &str,
    depth: usize,
    seen: &mut HashSet<PathBuf>,
    entries: &mut Vec<ConfigEntry>,
) -> Result<(), SshError> {
    if depth > MAX_DEPTH || !seen.insert(path.to_path_buf()) {
        return Ok(());
    }
    let text = std::fs::read_to_string(path).map_err(|_| SshError::new("config_unreadable"))?;
    if text.len() as u64 > MAX_BYTES {
        return Err(SshError::new("config_unreadable"));
    }
    let mut current: Option<ConfigEntry> = None;
    let mut unsupported = Vec::new();
    for raw in joined_lines(&text) {
        let line = strip_comment(&raw);
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_keyword(line) else {
            continue;
        };
        if key.eq_ignore_ascii_case("host") {
            push_entry(entries, current.take(), &mut unsupported);
            if value.split_whitespace().any(|part| part.contains('*') || part.contains('?') || part.contains('!'))
                || value.split_whitespace().count() != 1
            {
                continue;
            }
            current = Some(ConfigEntry {
                alias: value.to_string(),
                host: value.to_string(),
                port: 22,
                username: String::new(),
                key_path: None,
                jumps: Vec::new(),
                warnings: Vec::new(),
            });
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if key.eq_ignore_ascii_case("hostname") {
            entry.host = value.to_string();
        } else if key.eq_ignore_ascii_case("user") {
            entry.username = value.to_string();
        } else if key.eq_ignore_ascii_case("port") {
            entry.port = match value.parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => 22,
            };
        } else if key.eq_ignore_ascii_case("identityfile") && entry.key_path.is_none() {
            let expanded = expand_path(value, home, user, &entry.host);
            if !Path::new(&expanded).is_file() {
                entry.warnings.push("identity_missing".to_string());
            }
            entry.key_path = Some(expanded);
        } else if key.eq_ignore_ascii_case("proxyjump") {
            entry.jumps = value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty() && *part != "none")
                .map(strip_jump)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect();
        } else if key.eq_ignore_ascii_case("include") {
            push_entry(entries, current.take(), &mut unsupported);
            if value.contains('*') || value.contains('?') {
                continue;
            }
            let included = include_path(path, home, value);
            read_file(&included, home, user, depth + 1, seen, entries)?;
        } else {
            unsupported.push(key.to_string());
        }
    }
    push_entry(entries, current, &mut unsupported);
    if depth == 0 {
        resolve_jumps(entries);
    }
    Ok(())
}

fn resolve_jumps(entries: &mut [ConfigEntry]) {
    let aliases = entries
        .iter()
        .map(|entry| entry.alias.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    for entry in entries {
        if entry.jumps.is_empty() {
            continue;
        }
        if entry
            .jumps
            .iter()
            .any(|jump| !aliases.contains(&jump.to_ascii_lowercase()))
        {
            entry.jumps.clear();
            entry.warnings.push("jump_missing".to_string());
        }
    }
}

fn push_entry(entries: &mut Vec<ConfigEntry>, entry: Option<ConfigEntry>, unsupported: &mut Vec<String>) {
    let Some(mut entry) = entry else {
        unsupported.clear();
        return;
    };
    if entries.iter().any(|existing| existing.alias.eq_ignore_ascii_case(&entry.alias)) {
        unsupported.clear();
        return;
    }
    if entry.username.trim().is_empty() {
        entry.username = local_user();
    }
    if !unsupported.is_empty() {
        let detail = unsupported.join(", ");
        entry.warnings.push(format!("unsupported:{detail}"));
    }
    unsupported.clear();
    entries.push(entry);
}

fn joined_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut pending = String::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if let Some(prefix) = trimmed.strip_suffix('\\') {
            pending.push_str(prefix.trim_end());
            pending.push(' ');
            continue;
        }
        pending.push_str(trimmed);
        lines.push(std::mem::take(&mut pending));
    }
    if !pending.is_empty() {
        lines.push(pending);
    }
    lines
}

fn strip_comment(line: &str) -> &str {
    let mut quote = false;
    for (index, ch) in line.char_indices() {
        if ch == '"' {
            quote = !quote;
        } else if ch == '#' && !quote {
            return line[..index].trim();
        }
    }
    line.trim()
}

fn split_keyword(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let mut parts = line.splitn(2, char::is_whitespace);
    let key = parts.next()?.trim();
    let value = unquote(parts.next()?.trim());
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

fn unquote(value: &str) -> &str {
    value.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(value)
}

fn strip_jump(value: &str) -> &str {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(value).trim();
    }
    let host = value.rsplit_once('@').map(|(_, host)| host).unwrap_or(value);
    host.split(':').next().unwrap_or(host).trim()
}

fn expand_path(value: &str, home: &Path, user: &str, host: &str) -> String {
    let mut expanded = value.to_string();
    if let Some(rest) = expanded.strip_prefix("~/") {
        expanded = home.join(rest).to_string_lossy().into_owned();
    }
    expanded = expanded.replace("%%", "\u{1}");
    expanded = expanded.replace("%d", &home.to_string_lossy());
    expanded = expanded.replace("%u", user);
    expanded = expanded.replace("%h", host);
    expanded.replace('\u{1}', "%")
}

fn include_path(from: &Path, home: &Path, value: &str) -> PathBuf {
    let expanded = expand_path(value, home, &local_user(), "");
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        from.parent().unwrap_or(from).join(path)
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
}

fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_wildcards_and_links_jumps() {
        let home = std::env::temp_dir().join(format!("pixo-ssh-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/id_ed25519"), "key").unwrap();
        std::fs::write(
            home.join(".ssh/config"),
            "\
Host *
  ForwardAgent yes
Host bastion
  HostName 10.0.0.1
  User root
  IdentityFile ~/.ssh/id_ed25519
Host app
  HostName app.internal
  User deploy
  Port 2200
  ProxyJump bastion
  LocalForward 8080 localhost:80
Host other
  ProxyJump missing
",
        )
        .unwrap();
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        read_file(&home.join(".ssh/config"), &home, "denxuan", 0, &mut seen, &mut entries).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].alias, "bastion");
        assert!(entries[0].key_path.as_deref().unwrap().ends_with("id_ed25519"));
        assert_eq!(entries[1].jumps, vec!["bastion".to_string()]);
        assert!(entries[1].warnings.iter().any(|warning| warning.starts_with("unsupported:")));
        assert!(entries[2].warnings.iter().any(|warning| warning == "jump_missing"));
        assert!(entries[2].jumps.is_empty());
        let selected = expand_selection(&entries, &[1]);
        assert_eq!(selected, vec![0, 1]);
        let _ = std::fs::remove_dir_all(home);
    }
}
