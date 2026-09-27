//! Where the engine address and the player's token come from.
//!
//! Each setting is taken from the first of: a command-line flag, an
//! environment variable, then the stored configuration (the config file for
//! the address; the OS keychain, or a 0600 file where there is none, for the
//! token). There is no default address: the engine's operator gives it out.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const APP: &str = "text-adventure-terminal";
pub const ENV_URL: &str = "TA_ENGINE_URL";
pub const ENV_TOKEN: &str = "TA_TOKEN";
pub const ENV_CONFIG_DIR: &str = "TA_CONFIG_DIR";
pub const ENV_NO_KEYRING: &str = "TA_NO_KEYRING";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FileConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_url: Option<String>,
}

/// The directory holding `config.toml` (and the token file fallback).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(ENV_CONFIG_DIR).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::config_dir().map(|d| d.join(APP))
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

pub fn load_file(path: &Path) -> anyhow::Result<FileConfig> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("{} is not valid: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileConfig::default()),
        Err(e) => Err(anyhow::anyhow!("could not read {}: {e}", path.display())),
    }
}

pub fn save_file(path: &Path, config: &FileConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string(config)?)?;
    Ok(())
}

/// Resolve the engine address: flag, then environment, then config file.
pub fn resolve_url(flag: Option<&str>, env: Option<&str>, file: &FileConfig) -> Option<String> {
    [flag, env, file.engine_url.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

/// Where a stored token lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStore {
    Keychain,
    File,
}

impl std::fmt::Display for TokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Keychain => "the system keychain",
            Self::File => "a private file in the config directory",
        })
    }
}

fn keyring_enabled() -> bool {
    std::env::var_os(ENV_NO_KEYRING).is_none_or(|v| v.is_empty() || v == "0")
}

fn keyring_entry() -> Option<keyring::Entry> {
    if !keyring_enabled() {
        return None;
    }
    keyring::Entry::new(APP, "player-token").ok()
}

fn token_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("token"))
}

/// Store the token in the keychain, or a 0600 file where there is none.
pub fn store_token(token: &str) -> anyhow::Result<TokenStore> {
    if let Some(entry) = keyring_entry() {
        if entry.set_password(token).is_ok() && entry.get_password().ok().as_deref() == Some(token)
        {
            // A stale file copy would shadow nothing but should not linger.
            if let Some(path) = token_file() {
                let _ = fs::remove_file(path);
            }
            return Ok(TokenStore::Keychain);
        }
    }
    let path = token_file().ok_or_else(|| anyhow::anyhow!("no config directory on this system"))?;
    write_private(&path, token)?;
    Ok(TokenStore::File)
}

pub fn load_token() -> Option<String> {
    if let Some(entry) = keyring_entry() {
        if let Ok(token) = entry.get_password() {
            return Some(token);
        }
    }
    let path = token_file()?;
    fs::read_to_string(path)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

pub fn forget_token() -> anyhow::Result<()> {
    if let Some(entry) = keyring_entry() {
        let _ = entry.delete_credential();
    }
    if let Some(path) = token_file() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Resolve the token: flag, then environment, then the stored token.
pub fn resolve_token(flag: Option<&str>, env: Option<&str>) -> Option<String> {
    [flag, env]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(load_token)
}

fn write_private(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_beats_env_beats_file() {
        let file = FileConfig {
            engine_url: Some("http://file.example".into()),
        };
        assert_eq!(
            resolve_url(
                Some("http://flag.example"),
                Some("http://env.example"),
                &file
            )
            .as_deref(),
            Some("http://flag.example")
        );
        assert_eq!(
            resolve_url(None, Some("http://env.example"), &file).as_deref(),
            Some("http://env.example")
        );
        assert_eq!(
            resolve_url(None, Some("  "), &file).as_deref(),
            Some("http://file.example")
        );
        assert_eq!(resolve_url(None, None, &FileConfig::default()), None);
    }

    #[cfg(unix)]
    #[test]
    fn private_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        write_private(&path, "secret").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(fs::read_to_string(&path).unwrap(), "secret");
    }
}
