use std::path::{Path, PathBuf};

use russh::keys::{self, PrivateKey};

const DEFAULT_IDENTITIES: &[&str] = &[
    "id_rsa",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_ed25519",
    "id_ed25519_sk",
    "id_mldsa44_ed25519",
];

pub enum LoadedKey {
    Ready(PrivateKey),
    Encrypted,
    Failed,
}

pub fn load_private_key(path: &str, passphrase: Option<&str>) -> LoadedKey {
    let file = Path::new(path);
    if !file.is_file() {
        return LoadedKey::Failed;
    }
    match keys::load_secret_key(file, passphrase) {
        Ok(key) => LoadedKey::Ready(key),
        Err(keys::Error::KeyIsEncrypted) if passphrase.is_none() => LoadedKey::Encrypted,
        Err(error) => {
            eprintln!("ssh key: {error}");
            LoadedKey::Failed
        }
    }
}

pub fn default_identity_files() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    DEFAULT_IDENTITIES
        .iter()
        .map(|name| home.join(".ssh").join(name))
        .filter(|path| path.is_file())
        .collect()
}

pub fn identity_files_in(home: &Path) -> Vec<PathBuf> {
    DEFAULT_IDENTITIES
        .iter()
        .map(|name| home.join(".ssh").join(name))
        .filter(|path| path.is_file())
        .collect()
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

#[cfg(test)]
mod tests {
    use super::identity_files_in;

    #[test]
    fn default_identities_follow_openssh_order() {
        let home = std::env::temp_dir().join(format!("pixo-ssh-ident-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/id_ed25519"), "key").unwrap();
        std::fs::write(home.join(".ssh/id_rsa"), "key").unwrap();
        let names = identity_files_in(&home)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["id_rsa".to_string(), "id_ed25519".to_string()]);
        let _ = std::fs::remove_dir_all(home);
    }
}
