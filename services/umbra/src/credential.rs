//! Provider credentials.
//!
//! A key is a file mode `0600` (or `0400`) with no group or other bits, or a
//! systemd `LoadCredential` file under `CREDENTIALS_DIRECTORY`. The value is
//! never written into `Debug` or `Display`.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret([redacted])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

pub fn redact(secret: &str, text: &str) -> String {
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, "[redacted]")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialFormat {
    ApiKey,
    ChatgptSubscription,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CredentialSource {
    File {
        path: PathBuf,
        format: CredentialFormat,
    },
    Systemd {
        name: String,
        format: CredentialFormat,
    },
}

impl CredentialSource {
    pub fn format(&self) -> CredentialFormat {
        match self {
            Self::File { format, .. } | Self::Systemd { format, .. } => *format,
        }
    }
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential io: {0}")]
    Io(#[from] std::io::Error),
    #[error("credential: {0}")]
    Invalid(String),
}

pub fn validate_systemd_name(name: &str) -> Result<(), CredentialError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'-'
        })
        && !name.contains("..");
    if ok {
        Ok(())
    } else {
        Err(CredentialError::Invalid(
            "systemd credential name must be a single path segment".into(),
        ))
    }
}

pub fn systemd_path(directory: &Path, name: &str) -> Result<PathBuf, CredentialError> {
    validate_systemd_name(name)?;
    Ok(directory.join(name))
}

/// `CREDENTIALS_DIRECTORY` is set by systemd for `LoadCredential=`.
pub fn systemd_credential_path(name: &str) -> Result<PathBuf, CredentialError> {
    let directory = std::env::var_os("CREDENTIALS_DIRECTORY").ok_or_else(|| {
        CredentialError::Invalid(
            "CREDENTIALS_DIRECTORY is unset; add LoadCredential= to umbrad.service".into(),
        )
    })?;
    systemd_path(Path::new(&directory), name)
}

pub fn owner_only(mode: u32) -> bool {
    let bits = mode & 0o777;
    bits & 0o077 == 0 && bits & 0o400 != 0
}

pub fn read_owner_only(path: &Path) -> Result<Vec<u8>, CredentialError> {
    let meta = fs::metadata(path).map_err(|err| {
        CredentialError::Invalid(format!("cannot read {}: {err}", path.display()))
    })?;
    if !owner_only(meta.permissions().mode()) {
        return Err(CredentialError::Invalid(format!(
            "{} is not owner-only (mode {:o}); refusing to load it",
            path.display(),
            meta.permissions().mode() & 0o777
        )));
    }
    Ok(fs::read(path)?)
}

pub fn load_bytes(source: &CredentialSource) -> Result<Vec<u8>, CredentialError> {
    let path = match source {
        CredentialSource::File { path, .. } => path.clone(),
        CredentialSource::Systemd { name, .. } => systemd_credential_path(name)?,
    };
    read_owner_only(&path)
}

/// Write `bytes` as mode `0600` and refuse to leave a looser file behind.
pub fn install_secret(path: &Path, bytes: &[u8]) -> Result<(), CredentialError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    let mode = fs::metadata(&tmp)?.permissions().mode();
    if !owner_only(mode) {
        let _ = fs::remove_file(&tmp);
        return Err(CredentialError::Invalid(
            "secret file was not owner-only after create; removed it".into(),
        ));
    }
    fs::rename(&tmp, path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub fn shred(path: &Path) -> Result<(), CredentialError> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.is_file() {
            let mut file = OpenOptions::new().write(true).open(path)?;
            let zeros = vec![0u8; meta.len() as usize];
            file.write_all(&zeros)?;
            file.sync_all()?;
        }
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_does_not_reveal_the_value() {
        let secret = Secret::new("sk-test-CANARYKEY");
        assert!(!format!("{secret:?}").contains("CANARYKEY"));
        assert!(!format!("{secret}").contains("CANARYKEY"));
        assert_eq!(
            redact(secret.expose(), "bearer sk-test-CANARYKEY"),
            "bearer [redacted]"
        );
    }

    #[test]
    fn rejects_group_readable_files_and_writes_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("xai.key");
        install_secret(&path, b"sk-test-CANARYKEY\n").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let parent_mode = fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(parent_mode, 0o700);
        let loaded = read_owner_only(&path).unwrap();
        assert_eq!(loaded, b"sk-test-CANARYKEY\n");

        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let err = read_owner_only(&path).unwrap_err();
        assert!(err.to_string().contains("owner-only"));
    }

    #[test]
    fn systemd_name_rejects_path_separators() {
        assert!(validate_systemd_name("../xai").is_err());
        assert!(validate_systemd_name("dj-provider-xai").is_ok());
        let path = systemd_path(Path::new("/run/credentials"), "dj-provider-xai").unwrap();
        assert_eq!(path, PathBuf::from("/run/credentials/dj-provider-xai"));
    }
}
