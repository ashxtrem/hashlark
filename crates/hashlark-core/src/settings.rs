// SPDX-License-Identifier: GPL-3.0-or-later

//! User settings, stored as one JSON document in the `settings` table.
//!
//! Every field has a default, so settings saved by an older version load
//! cleanly after an upgrade.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};
use crate::store::Store;

const SETTINGS_KEY: &str = "app";

/// All user-configurable settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct Settings {
    pub search: SearchSettings,
    pub network: NetworkSettings,
    pub magnets: MagnetSettings,
    pub downloads: DownloadSettings,
    pub updates: UpdateSettings,
    pub ui: UiSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct SearchSettings {
    /// How long one provider may take, in seconds.
    pub provider_timeout_secs: u64,
    /// Maximum providers searched at once.
    pub max_concurrency: usize,
    /// How long identical searches are served from cache, in seconds. 0 disables caching.
    pub cache_ttl_secs: u64,
    pub save_history: bool,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            provider_timeout_secs: 10,
            max_concurrency: 16,
            cache_ttl_secs: 600,
            save_history: true,
        }
    }
}

impl SearchSettings {
    pub fn provider_timeout(&self) -> Duration {
        Duration::from_secs(self.provider_timeout_secs)
    }

    pub fn cache_ttl(&self) -> Duration {
        Duration::from_secs(self.cache_ttl_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct NetworkSettings {
    pub doh: DohSettings,
    /// Global proxy, e.g. `socks5h://127.0.0.1:9050` or `http://proxy:8080`.
    pub proxy: Option<Url>,
    pub tor: TorSettings,
    /// Requests per second allowed to one host.
    pub per_host_rate: u32,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            doh: DohSettings::default(),
            proxy: None,
            tor: TorSettings::default(),
            per_host_rate: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct DohSettings {
    pub enabled: bool,
    pub resolver: DohResolver,
    /// Custom DoH endpoint, used when `resolver` is `custom`.
    pub custom_url: Option<Url>,
    /// Fall back to the system resolver if DoH fails.
    pub fallback_to_system: bool,
}

impl Default for DohSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            resolver: DohResolver::Cloudflare,
            custom_url: None,
            fallback_to_system: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DohResolver {
    #[default]
    Cloudflare,
    Quad9,
    Google,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct TorSettings {
    /// Route every provider through Tor.
    pub enabled: bool,
    /// Use Hashlark's built-in Tor, or a Tor already running on this
    /// computer (at `socks_url`).
    pub mode: TorMode,
    /// SOCKS address of a running Tor (Tor Browser uses port 9150, the Tor
    /// service and Orbot 9050).
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub socks_url: Option<Url>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TorMode {
    #[default]
    Embedded,
    External,
}

impl Default for TorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: TorMode::Embedded,
            socks_url: Some("socks5h://127.0.0.1:9050".parse().expect("valid URL")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct MagnetSettings {
    /// Add the default trackers to magnets that already list their own.
    pub append_default_trackers: bool,
    /// URL of a plain-text tracker list (one per line), refreshed daily.
    pub trackers_url: Option<Url>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct DownloadSettings {
    /// Where `.torrent` files are saved. `None` means the OS downloads folder.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub torrent_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct UpdateSettings {
    /// Check GitHub Releases for new versions.
    pub check_for_updates: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_for_updates: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct UiSettings {
    pub theme: Theme,
    /// Set once the user has finished the first-run screen.
    pub first_run_done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Settings {
    /// Checks values that serde can't.
    pub fn validate(&self) -> Result<()> {
        let s = &self.search;
        if !(1..=120).contains(&s.provider_timeout_secs) {
            return Err(Error::Invalid(
                "search.provider_timeout_secs must be between 1 and 120".into(),
            ));
        }
        if !(1..=64).contains(&s.max_concurrency) {
            return Err(Error::Invalid(
                "search.max_concurrency must be between 1 and 64".into(),
            ));
        }
        if !(1..=20).contains(&self.network.per_host_rate) {
            return Err(Error::Invalid(
                "network.per_host_rate must be between 1 and 20".into(),
            ));
        }
        if let Some(proxy) = &self.network.proxy
            && !matches!(proxy.scheme(), "http" | "https" | "socks5" | "socks5h")
        {
            return Err(Error::Invalid(
                "network.proxy must be an http, https, socks5 or socks5h URL".into(),
            ));
        }
        let tor = &self.network.tor;
        if let Some(url) = &tor.socks_url
            && !matches!(url.scheme(), "socks5" | "socks5h")
        {
            return Err(Error::Invalid(
                "network.tor.socks_url must be a socks5h:// URL".into(),
            ));
        }
        if tor.enabled && tor.mode == TorMode::External && tor.socks_url.is_none() {
            return Err(Error::Invalid(
                "network.tor.socks_url is required when Tor is enabled".into(),
            ));
        }
        let doh = &self.network.doh;
        if doh.enabled && doh.resolver == DohResolver::Custom {
            match &doh.custom_url {
                Some(url) if url.scheme() == "https" => {}
                _ => {
                    return Err(Error::Invalid(
                        "network.doh.custom_url must be an https URL when the resolver is custom"
                            .into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub async fn load(store: &Store) -> Result<Self> {
        Ok(store.get_setting(SETTINGS_KEY).await?.unwrap_or_default())
    }

    pub async fn save(&self, store: &Store) -> Result<()> {
        self.validate()?;
        store.set_setting(SETTINGS_KEY, self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn defaults_then_round_trip() {
        let store = Store::open_in_memory().await.unwrap();
        let mut settings = Settings::load(&store).await.unwrap();
        assert_eq!(settings, Settings::default());
        assert!(settings.network.doh.enabled, "DoH is on by default");

        settings.search.provider_timeout_secs = 15;
        settings.save(&store).await.unwrap();
        assert_eq!(Settings::load(&store).await.unwrap(), settings);
    }

    #[test]
    fn partial_json_fills_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{ "search": { "max_concurrency": 4 } }"#).unwrap();
        assert_eq!(settings.search.max_concurrency, 4);
        assert_eq!(settings.search.provider_timeout_secs, 10);
    }

    #[test]
    fn validation_rejects_bad_values() {
        let mut s = Settings::default();
        s.network.proxy = Some("ftp://x".parse().unwrap());
        assert!(s.validate().is_err());

        let mut s = Settings::default();
        s.network.doh.resolver = DohResolver::Custom;
        assert!(s.validate().is_err(), "custom resolver needs a URL");
        s.network.doh.custom_url = Some("https://dns.example/dns-query".parse().unwrap());
        assert!(s.validate().is_ok());

        let mut s = Settings::default();
        s.search.provider_timeout_secs = 0;
        assert!(s.validate().is_err());
    }
}
