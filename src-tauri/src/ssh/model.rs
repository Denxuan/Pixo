use serde::{Deserialize, Serialize};

use super::error::SshError;
use super::shell::shell_single_quote;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    Password,
    Key,
}

impl AuthKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "password" => Some(Self::Password),
            "key" => Some(Self::Key),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: i64,
    pub name: String,
    pub group_name: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthKind,
    pub key_path: Option<String>,
    pub remember: bool,
    pub start_dir: Option<String>,
    pub term: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDraft {
    pub id: Option<i64>,
    pub name: String,
    pub group_name: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthKind,
    pub key_path: Option<String>,
    #[serde(default)]
    pub remember: bool,
    pub start_dir: Option<String>,
    pub term: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NormalizedHost {
    pub id: Option<i64>,
    pub name: String,
    pub group_name: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthKind,
    pub key_path: Option<String>,
    pub remember: bool,
    pub start_dir: Option<String>,
    pub term: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshStatus {
    pub enabled: bool,
    pub keyring_available: bool,
}

pub fn normalize(draft: HostDraft) -> Result<NormalizedHost, SshError> {
    let host = normalize_host(&draft.host)?;
    let username = draft.username.trim();
    if username.is_empty()
        || username.chars().count() > 128
        || username.chars().any(char::is_whitespace)
    {
        return Err(SshError::new("invalid_input"));
    }
    if draft.port == 0 {
        return Err(SshError::new("invalid_input"));
    }
    let name = {
        let trimmed = draft.name.trim();
        let name = if trimmed.is_empty() {
            host.as_str()
        } else {
            trimmed
        };
        if name.chars().count() > 128 {
            return Err(SshError::new("invalid_input"));
        }
        name.to_string()
    };
    let group_name = match draft.group_name.as_deref().map(str::trim) {
        Some(group) if !group.is_empty() => {
            if group.chars().count() > 64 {
                return Err(SshError::new("invalid_input"));
            }
            Some(group.to_string())
        }
        _ => None,
    };
    let key_path = match draft.auth {
        AuthKind::Password => None,
        AuthKind::Key => {
            let path = draft.key_path.as_deref().map(str::trim).unwrap_or("");
            if path.is_empty() || path.len() > 1024 || path.chars().any(|char| char == '\0') {
                return Err(SshError::new("invalid_input"));
            }
            Some(path.to_string())
        }
    };
    let start_dir = match draft.start_dir.as_deref().map(str::trim) {
        Some(dir) if !dir.is_empty() => {
            shell_single_quote(dir)?;
            Some(dir.to_string())
        }
        _ => None,
    };
    let term = match draft.term.as_deref().map(str::trim) {
        Some(term) if !term.is_empty() => {
            if !is_term(term) {
                return Err(SshError::new("invalid_input"));
            }
            term.to_string()
        }
        _ => "xterm-256color".to_string(),
    };
    Ok(NormalizedHost {
        id: draft.id,
        name,
        group_name,
        host,
        port: draft.port,
        username: username.to_string(),
        auth: draft.auth,
        key_path,
        remember: draft.remember,
        start_dir,
        term,
    })
}

pub fn normalize_host(value: &str) -> Result<String, SshError> {
    let host = value.trim().trim_matches(|char| char == '[' || char == ']');
    if host.is_empty()
        || host.len() > 255
        || host
            .chars()
            .any(|char| char.is_whitespace() || matches!(char, '/' | ',' | '@' | '#' | '\\'))
    {
        return Err(SshError::new("invalid_input"));
    }
    Ok(host.to_string())
}

fn is_term(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= 64
        && first.is_ascii_alphanumeric()
        && chars.all(|char| char.is_ascii_alphanumeric() || matches!(char, '-' | '_' | '.' | '+'))
}

pub fn auth_to_db(auth: AuthKind) -> &'static str {
    auth.as_str()
}

pub fn auth_from_db(value: &str) -> Result<AuthKind, SshError> {
    AuthKind::parse(value).ok_or_else(|| SshError::new("storage"))
}

#[cfg(test)]
mod tests {
    use super::{AuthKind, HostDraft, normalize};

    fn draft() -> HostDraft {
        HostDraft {
            id: None,
            name: "  ".into(),
            group_name: Some("  ".into()),
            host: " [example.com] ".into(),
            port: 22,
            username: "root".into(),
            auth: AuthKind::Password,
            key_path: None,
            remember: false,
            start_dir: Some(" /var/tmp ".into()),
            term: None,
        }
    }

    #[test]
    fn defaults_name_term_and_strips_brackets() {
        let host = normalize(draft()).unwrap();
        assert_eq!(host.name, "example.com");
        assert_eq!(host.host, "example.com");
        assert_eq!(host.term, "xterm-256color");
        assert_eq!(host.group_name, None);
        assert_eq!(host.start_dir.as_deref(), Some("/var/tmp"));
        assert!(!host.remember);
    }

    #[test]
    fn key_auth_requires_a_path() {
        let mut draft = draft();
        draft.auth = AuthKind::Key;
        assert!(normalize(draft).is_err());
    }
}
