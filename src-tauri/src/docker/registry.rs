use std::sync::OnceLock;

use bollard::auth::DockerCredentials;
use keyring::{Entry, Error as KeyringError};
use zeroize::Zeroizing;

use super::error::DockerError;

const SERVICE: &str = "pixo.docker";

pub fn registry_host(reference: &str) -> String {
    let name = reference.split('@').next().unwrap_or(reference).trim();
    let Some((first, _)) = name.split_once('/') else {
        return "docker.io".to_string();
    };
    if first.eq_ignore_ascii_case("localhost") || first.contains('.') || first.contains(':') {
        first.to_string()
    } else {
        "docker.io".to_string()
    }
}

pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(probe)
}

pub fn load(server: &str) -> Result<Option<(String, Zeroizing<String>)>, DockerError> {
    let raw = match entry(server)?.get_password() {
        Ok(secret) => Zeroizing::new(secret),
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(error) => {
            eprintln!("docker keyring: {error}");
            return Err(DockerError::new("keyring_unavailable"));
        }
    };
    let Some((username, password)) = raw.split_once('\n') else {
        return Ok(None);
    };
    if username.is_empty() || password.is_empty() {
        return Ok(None);
    }
    Ok(Some((
        username.to_string(),
        Zeroizing::new(password.to_string()),
    )))
}

pub fn save(server: &str, username: &str, password: &str) -> Result<(), DockerError> {
    if !valid_username(username) || password.is_empty() || password.len() > 1024 {
        return Err(DockerError::new("invalid_input"));
    }
    if !available() {
        return Err(DockerError::new("keyring_unavailable"));
    }
    let stored = format!("{username}\n{password}");
    entry(server)?.set_password(&stored).map_err(|error| {
        eprintln!("docker keyring: {error}");
        DockerError::new("keyring_unavailable")
    })
}

pub fn credentials(
    reference: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Option<DockerCredentials>, DockerError> {
    let server = registry_host(reference);
    if let (Some(username), Some(password)) = (username, password) {
        let username = username.trim();
        if username.is_empty() && password.is_empty() {
            return saved(&server);
        }
        if !valid_username(username) || password.is_empty() {
            return Err(DockerError::new("invalid_input"));
        }
        return Ok(Some(header(&server, username, password)));
    }
    saved(&server)
}

fn saved(server: &str) -> Result<Option<DockerCredentials>, DockerError> {
    Ok(load(server)?.map(|(username, password)| header(server, &username, password.as_str())))
}

fn header(server: &str, username: &str, password: &str) -> DockerCredentials {
    DockerCredentials {
        username: Some(username.to_string()),
        password: Some(password.to_string()),
        serveraddress: Some(server.to_string()),
        ..DockerCredentials::default()
    }
}

fn valid_username(username: &str) -> bool {
    !username.is_empty()
        && username.len() <= 128
        && !username
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
}

fn entry(server: &str) -> Result<Entry, DockerError> {
    Entry::new(SERVICE, &format!("registry:{server}")).map_err(|error| {
        eprintln!("docker keyring: {error}");
        DockerError::new("keyring_unavailable")
    })
}

fn probe() -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        match Entry::new(SERVICE, "availability") {
            Ok(entry) => match entry.get_password() {
                Ok(_) | Err(KeyringError::NoEntry) => true,
                Err(error) => {
                    eprintln!("docker keyring probe: {error}");
                    false
                }
            },
            Err(error) => {
                eprintln!("docker keyring probe: {error}");
                false
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::registry_host;

    #[test]
    fn registry_host_follows_the_first_slash() {
        assert_eq!(registry_host("nginx:latest"), "docker.io");
        assert_eq!(registry_host("library/nginx"), "docker.io");
        assert_eq!(registry_host("ghcr.io/org/app:1"), "ghcr.io");
        assert_eq!(registry_host("localhost:5000/app"), "localhost:5000");
        assert_eq!(registry_host("ghcr.io/org/app@sha256:abcd"), "ghcr.io");
    }
}
