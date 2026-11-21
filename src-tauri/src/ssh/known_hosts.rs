use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use hmac::{Hmac, Mac};
use russh::keys::ssh_key::known_hosts::{Entry, HostPatterns, KnownHosts, Marker};
use russh::keys::{HashAlg, PublicKey};
use sha1::Sha1;

use super::error::SshError;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChange {
    pub algorithm: String,
    pub fingerprint: String,
    pub previous: String,
    pub source: &'static str,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Trusted,
    Unknown {
        algorithm: String,
        fingerprint: String,
    },
    Changed(KeyChange),
}

pub struct KnownStore {
    path: PathBuf,
    user_path: Option<PathBuf>,
}

impl KnownStore {
    pub fn new(path: PathBuf, user_path: Option<PathBuf>) -> Self {
        Self { path, user_path }
    }

    pub fn decide(&self, host: &str, port: u16, key: &PublicKey) -> Result<Decision, SshError> {
        let mut files = vec![("pixo", load_entries(&self.path))];
        if let Some(path) = &self.user_path {
            files.push(("user", load_entries(path)));
        }
        let borrowed = files
            .iter()
            .map(|(source, entries)| (*source, entries.as_slice()))
            .collect::<Vec<_>>();
        Ok(decide_entries(&borrowed, host, port, key))
    }

    pub fn trust(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), SshError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(storage)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true).read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.path).map_err(storage)?;
        let needs_newline = file.metadata().map(|meta| meta.len() > 0).unwrap_or(false)
            && !ends_with_newline(&mut file);
        let encoded = key.to_openssh().map_err(storage)?;
        let marker = lookup_name(host, port);
        if needs_newline {
            write!(file, "\n").map_err(storage)?;
        }
        writeln!(file, "{marker} {}", encoded.trim()).map_err(storage)?;
        Ok(())
    }

    pub fn forget(&self, host: &str, port: u16) -> Result<(), SshError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(storage(error)),
        };
        let mut kept = Vec::new();
        for line in text.lines() {
            if !line_matches(line, host, port) {
                kept.push(line);
            }
        }
        let mut output = kept.join("\n");
        if !output.is_empty() {
            output.push('\n');
        }
        fs::write(&self.path, output).map_err(storage)
    }
}

pub fn lookup_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

pub fn decide_entries(
    files: &[(&str, &[Entry])],
    host: &str,
    port: u16,
    key: &PublicKey,
) -> Decision {
    let lookup = lookup_name(host, port);
    let algorithm = key.algorithm().to_string();
    let current = fingerprint(key);
    let mut mismatch: Option<KeyChange> = None;
    for (source, entries) in files {
        for entry in *entries {
            if entry.marker() == Some(&Marker::CertAuthority) || !entry_matches(entry, &lookup) {
                continue;
            }
            let revoked = entry.marker() == Some(&Marker::Revoked);
            if entry.public_key() == key {
                if revoked {
                    return Decision::Changed(KeyChange {
                        algorithm: algorithm.clone(),
                        fingerprint: current.clone(),
                        previous: current.clone(),
                        source: source_name(source),
                        revoked: true,
                    });
                }
                return Decision::Trusted;
            }
            if mismatch.is_none() && entry.public_key().algorithm() == key.algorithm() {
                mismatch = Some(KeyChange {
                    algorithm: algorithm.clone(),
                    fingerprint: current.clone(),
                    previous: fingerprint(entry.public_key()),
                    source: source_name(source),
                    revoked: false,
                });
            }
        }
    }
    match mismatch {
        Some(change) => Decision::Changed(change),
        None => Decision::Unknown {
            algorithm,
            fingerprint: current,
        },
    }
}

fn source_name(source: &str) -> &'static str {
    if source == "user" { "user" } else { "pixo" }
}

fn entry_matches(entry: &Entry, lookup: &str) -> bool {
    match entry.host_patterns() {
        HostPatterns::Patterns(patterns) => pattern_list_matches(patterns, lookup),
        HostPatterns::HashedName { salt, hash } => hashed_matches(salt, hash, lookup),
    }
}

fn pattern_list_matches(patterns: &[String], lookup: &str) -> bool {
    let mut matched = false;
    for pattern in patterns {
        let (negative, body) = match pattern.strip_prefix('!') {
            Some(body) => (true, body),
            None => (false, pattern.as_str()),
        };
        if glob_match(body, lookup) {
            if negative {
                return false;
            }
            matched = true;
        }
    }
    matched
}

fn hashed_matches(salt: &[u8], expected: &[u8; 20], lookup: &str) -> bool {
    let Ok(mut mac) = HmacSha1::new_from_slice(salt) else {
        return false;
    };
    mac.update(lookup.as_bytes());
    mac.verify_slice(expected).is_ok()
}

fn glob_match(pattern: &str, text: &str) -> bool {
    fn rec(pattern: &[u8], text: &[u8]) -> bool {
        let mut pattern_index = 0;
        let mut text_index = 0;
        while pattern_index < pattern.len() {
            if pattern[pattern_index] == b'*' {
                if pattern_index + 1 == pattern.len() {
                    return true;
                }
                while text_index <= text.len() {
                    if rec(&pattern[pattern_index + 1..], &text[text_index..]) {
                        return true;
                    }
                    if text_index == text.len() {
                        return false;
                    }
                    text_index += 1;
                }
                return false;
            }
            if text_index == text.len() {
                return false;
            }
            if pattern[pattern_index] != b'?' && pattern[pattern_index] != text[text_index] {
                return false;
            }
            pattern_index += 1;
            text_index += 1;
        }
        text_index == text.len()
    }
    rec(pattern.as_bytes(), text.as_bytes())
}

fn line_matches(line: &str, host: &str, port: u16) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    let Ok(entry) = trimmed.parse::<Entry>() else {
        return false;
    };
    entry_matches(&entry, &lookup_name(host, port))
}

fn load_entries(path: &Path) -> Vec<Entry> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    KnownHosts::new(&text).filter_map(Result::ok).collect()
}

fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

fn ends_with_newline(file: &mut std::fs::File) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(len) = file.seek(SeekFrom::End(0)) else {
        return true;
    };
    if len == 0 {
        return true;
    }
    if file.seek(SeekFrom::End(-1)).is_err() {
        return true;
    }
    let mut byte = [0];
    file.read_exact(&mut byte).is_ok() && byte[0] == b'\n'
}

fn storage(error: impl std::fmt::Display) -> SshError {
    eprintln!("ssh known_hosts: {error}");
    SshError::new("storage")
}

pub fn user_known_hosts_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".ssh").join("known_hosts"))
}

#[cfg(test)]
mod tests {
    use super::{Decision, decide_entries, lookup_name};
    use hmac::{Hmac, Mac};
    use russh::keys::PublicKey;
    use russh::keys::ssh_key::encoding::base64::{Base64, Encoding};
    use russh::keys::ssh_key::known_hosts::Entry;
    use sha1::Sha1;

    const KEY_A: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti";
    const KEY_B: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB9dG4kjRhQTtWTVzd2t27+t0DEHBPW7iOD23TUiYLio";

    fn key(text: &str) -> PublicKey {
        PublicKey::from_openssh(text).unwrap()
    }

    fn entry(line: &str) -> Entry {
        line.parse().unwrap()
    }

    #[test]
    fn port_and_glob_and_negation() {
        let key = key(KEY_A);
        let plain = entry(&format!("example.com {KEY_A}"));
        assert!(matches!(
            decide_entries(&[("pixo", &[plain.clone()])], "example.com", 22, &key),
            Decision::Trusted
        ));
        assert!(matches!(
            decide_entries(&[("pixo", &[plain])], "example.com", 2222, &key),
            Decision::Unknown { .. }
        ));
        let port = entry(&format!("[example.com]:2222 {KEY_A}"));
        assert!(matches!(
            decide_entries(&[("pixo", &[port])], "example.com", 2222, &key),
            Decision::Trusted
        ));
        let glob = entry(&format!("!bad.example.com,*.example.com {KEY_A}"));
        assert!(matches!(
            decide_entries(&[("pixo", &[glob.clone()])], "good.example.com", 22, &key),
            Decision::Trusted
        ));
        assert!(matches!(
            decide_entries(&[("pixo", &[glob])], "bad.example.com", 22, &key),
            Decision::Unknown { .. }
        ));
        assert_eq!(lookup_name("example.com", 2222), "[example.com]:2222");
    }

    #[test]
    fn hashed_name_and_changed_key() {
        let key = key(KEY_A);
        let salt = b"pixo-salt-value!!";
        let mut mac = Hmac::<Sha1>::new_from_slice(salt).unwrap();
        mac.update(b"example.com");
        let hash = mac.finalize().into_bytes();
        let line = format!(
            "|1|{}|{} {KEY_A}",
            Base64::encode_string(salt),
            Base64::encode_string(&hash)
        );
        let hashed = entry(&line);
        assert!(matches!(
            decide_entries(&[("user", &[hashed.clone()])], "example.com", 22, &key),
            Decision::Trusted
        ));
        assert!(matches!(
            decide_entries(&[("user", &[hashed])], "other.example", 22, &key),
            Decision::Unknown { .. }
        ));
        let other = entry(&format!("example.com {KEY_B}"));
        match decide_entries(&[("pixo", &[other])], "example.com", 22, &key) {
            Decision::Changed(change) => {
                assert_eq!(change.source, "pixo");
                assert!(!change.revoked);
                assert_ne!(change.previous, change.fingerprint);
            }
            other => panic!("expected a changed key, got {other:?}"),
        }
    }

    #[test]
    fn revoked_key_blocks_the_match() {
        let key = key(KEY_A);
        let revoked = entry(&format!("@revoked example.com {KEY_A}"));
        match decide_entries(&[("user", &[revoked])], "example.com", 22, &key) {
            Decision::Changed(change) => assert!(change.revoked),
            other => panic!("expected a revoked key, got {other:?}"),
        }
    }

    #[test]
    fn trust_and_forget_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "pixo-ssh-known-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let store = super::KnownStore::new(dir.join("known_hosts"), None);
        let key = key(KEY_A);
        assert!(matches!(
            store.decide("example.com", 22, &key).unwrap(),
            Decision::Unknown { .. }
        ));
        store.trust("example.com", 22, &key).unwrap();
        assert!(matches!(
            store.decide("example.com", 22, &key).unwrap(),
            Decision::Trusted
        ));
        store.forget("example.com", 22).unwrap();
        assert!(matches!(
            store.decide("example.com", 22, &key).unwrap(),
            Decision::Unknown { .. }
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
