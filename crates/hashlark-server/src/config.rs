// SPDX-License-Identifier: GPL-3.0-or-later

//! `hashlark.toml`: settings for the headless server. Command-line flags and
//! `HASHLARK_*` environment variables override the file.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The config file. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    /// Address to listen on, e.g. `0.0.0.0:8787`.
    pub bind: Option<SocketAddr>,
    pub data_dir: Option<PathBuf>,
    pub tls: Option<TlsConfig>,
    /// `file` (default for servers) or `keychain`.
    pub secrets: Option<SecretsBackend>,
    /// Serve the web UI at `/` (default true).
    pub web_ui: Option<bool>,
    /// Serve the UI from this folder instead of the built-in copy.
    pub web_ui_dir: Option<PathBuf>,
    pub cors_origins: Vec<String>,
    pub definitions_dir: Option<PathBuf>,
    /// Log filter, e.g. `info` or `hashlark_core=debug,info`.
    pub log: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    pub cert: PathBuf,
    pub key: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretsBackend {
    File,
    Keychain,
}

impl FileConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config() {
        let example = include_str!("../../../docs/deploy/hashlark.example.toml");
        let config: FileConfig = toml::from_str(example).unwrap();
        assert_eq!(config.bind.unwrap().port(), 8787);
        assert_eq!(config.secrets, Some(SecretsBackend::File));
        assert!(config.tls.is_none());
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(toml::from_str::<FileConfig>("bnid = \"0.0.0.0:1\"").is_err());
    }
}
