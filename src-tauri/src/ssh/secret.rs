use std::sync::OnceLock;

use keyring::{Entry, Error as KeyringError};
use zeroize::Zeroizing;

use super::error::SshError;

const SERVICE: &str = "pixo.ssh";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    Passphrase,
}

pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(probe)
}

pub fn get(host_id: i64, kind: SecretKind) -> Result<Option<Zeroizing<String>>, SshError> {
    match entry(host_id, kind)?.get_password() {
        Ok(secret) => Ok(Some(Zeroizing::new(secret))),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(map_keyring(error)),
    }
}

pub fn set(host_id: i64, kind: SecretKind, secret: &str) -> Result<(), SshError> {
    if secret.is_empty() || secret.len() > 1024 {
        return Err(SshError::new("invalid_input"));
    }
    let owned = Zeroizing::new(secret.to_string());
    entry(host_id, kind)?
        .set_password(owned.as_str())
        .map_err(map_keyring)
}

pub fn delete(host_id: i64, kind: SecretKind) -> Result<(), SshError> {
    match entry(host_id, kind)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(map_keyring(error)),
    }
}

pub fn delete_host(host_id: i64) -> Result<(), SshError> {
    delete(host_id, SecretKind::Password)?;
    delete(host_id, SecretKind::Passphrase)
}

pub(crate) fn account(host_id: i64, kind: SecretKind) -> String {
    match kind {
        SecretKind::Password => format!("{host_id}:password"),
        SecretKind::Passphrase => format!("{host_id}:passphrase"),
    }
}

fn entry(host_id: i64, kind: SecretKind) -> Result<Entry, SshError> {
    Entry::new(SERVICE, &account(host_id, kind)).map_err(map_keyring)
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
                    eprintln!("ssh keyring probe: {error}");
                    false
                }
            },
            Err(error) => {
                eprintln!("ssh keyring probe: {error}");
                false
            }
        }
    }
}

fn map_keyring(error: KeyringError) -> SshError {
    eprintln!("ssh keyring: {error}");
    match error {
        KeyringError::NoStorageAccess(_) => SshError::new("keyring_unavailable"),
        _ => SshError::new("storage"),
    }
}

#[cfg(test)]
mod tests {
    use super::{SecretKind, account};

    #[test]
    fn account_names_include_the_host_and_kind() {
        assert_eq!(account(7, SecretKind::Password), "7:password");
        assert_eq!(account(7, SecretKind::Passphrase), "7:passphrase");
    }
}
