use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::error::SshError;

const GRANT_TTL: Duration = Duration::from_secs(600);
const MAX_ROOTS: usize = 500;

struct Grant {
    paths: Vec<PathBuf>,
    at: Instant,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadPick {
    pub token: String,
    pub names: Vec<String>,
}

pub fn issue(paths: &[PathBuf]) -> Result<UploadPick, SshError> {
    if paths.len() > MAX_ROOTS {
        return Err(SshError::new("too_many"));
    }
    let mut kept = Vec::new();
    let mut names = Vec::new();
    for path in paths {
        if !usable(path) {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        let name = name.to_string_lossy();
        if name.is_empty() || name.contains('\0') {
            continue;
        }
        names.push(name.into_owned());
        kept.push(path.clone());
    }
    if kept.is_empty() {
        return Err(SshError::new("invalid_input"));
    }
    let token = random_id();
    let mut map = lock(grants());
    expire(&mut map);
    map.insert(
        token.clone(),
        Grant {
            paths: kept,
            at: Instant::now(),
        },
    );
    Ok(UploadPick { token, names })
}

pub fn take(token: &str) -> Result<Vec<PathBuf>, SshError> {
    if token.len() != 32 || !token.chars().all(|char| char.is_ascii_hexdigit()) {
        return Err(SshError::new("token_expired"));
    }
    let mut map = lock(grants());
    expire(&mut map);
    map.remove(token)
        .map(|grant| grant.paths)
        .ok_or_else(|| SshError::new("token_expired"))
}

pub fn release(token: &str) {
    let _ = take(token);
}

pub fn nonce() -> String {
    random_id()[..8].to_string()
}

fn usable(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    meta.is_file() || meta.is_dir() || meta.file_type().is_symlink()
}

fn expire(map: &mut HashMap<String, Grant>) {
    let now = Instant::now();
    map.retain(|_, grant| now.saturating_duration_since(grant.at) < GRANT_TTL);
}

fn grants() -> &'static Mutex<HashMap<String, Grant>> {
    static GRANTS: OnceLock<Mutex<HashMap<String, Grant>>> = OnceLock::new();
    GRANTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn random_id() -> String {
    let state = RandomState::new();
    let mut left = state.build_hasher();
    left.write_u64(0x7069_786f_6472_6f70);
    let mut right = state.build_hasher();
    right.write_u64(0x7368_5f74_6f6b_656e);
    format!("{:016x}{:016x}", left.finish(), right.finish())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::{issue, release, take};

    #[test]
    fn token_is_single_use() {
        let dir = std::env::temp_dir().join(format!("pixo-ssh-grant-{}", super::nonce()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.txt");
        std::fs::write(&file, b"hello").unwrap();
        let pick = issue(&[file]).unwrap();
        assert_eq!(pick.names, vec!["note.txt".to_string()]);
        assert!(take(&pick.token).is_ok());
        assert!(take(&pick.token).is_err());
        release("not-a-token");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
