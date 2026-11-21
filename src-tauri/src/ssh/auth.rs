use std::path::Path;

use russh::keys::{self, PrivateKey};

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
