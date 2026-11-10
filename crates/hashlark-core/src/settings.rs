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
    pub torrents: TorrentSettings,
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

/// Settings of the built-in torrent client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct TorrentSettings {
    /// Master switch for the built-in client.
    pub enabled: bool,
    /// Where downloads are saved. `None` means the OS downloads folder.
    /// Separate from `downloads.torrent_dir`, which holds `.torrent` files.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub download_dir: Option<PathBuf>,
    /// Ask where to save each time a torrent is added.
    pub ask_on_add: bool,
    /// The user has accepted the first-use notice.
    pub notice_accepted: bool,
    /// Listen port. 0 means pick one on first start and save it.
    pub listen_port: u16,
    pub upnp: bool,
    pub dht: bool,
    pub lsd: bool,
    pub utp: bool,
    /// Normal speed limits.
    pub limits: SpeedLimits,
    /// Limits used by turtle mode and the schedule's `a` cells.
    pub alt_limits: SpeedLimits,
    /// Turtle mode: apply `alt_limits` now.
    pub turtle: bool,
    /// Weekly schedule of speed modes.
    pub schedule: Option<WeekGrid>,
    pub max_active_downloads: u16,
    pub max_active_seeds: u16,
    pub peer_limit_global: u32,
    pub peer_limit_torrent: u32,
    pub keep_awake: KeepAwake,
    pub on_battery: OnBattery,
    /// Stop seeding at this share ratio. `None` seeds forever.
    pub seed_ratio: Option<f32>,
    /// Stop seeding after this many minutes. `None` means no time limit.
    pub seed_minutes: Option<u32>,
    /// SOCKS5 proxy for torrent traffic.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub proxy: Option<Url>,
    /// IP blocklist to download and apply.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub blocklist_url: Option<Url>,
    /// Folder watched for `.torrent` files to add.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub watch_dir: Option<PathBuf>,
    /// Android: only download on unmetered networks.
    pub unmetered_only: bool,
    /// Desktop: closing the window keeps the client running in the tray.
    pub close_to_tray: bool,
}

impl Default for TorrentSettings {
    fn default() -> Self {
        let android = cfg!(target_os = "android");
        Self {
            enabled: true,
            download_dir: None,
            ask_on_add: false,
            notice_accepted: false,
            listen_port: 0,
            upnp: true,
            dht: true,
            lsd: true,
            utp: true,
            limits: SpeedLimits::default(),
            alt_limits: SpeedLimits {
                down_kib: Some(1024),
                up_kib: Some(256),
            },
            turtle: false,
            schedule: None,
            max_active_downloads: 3,
            max_active_seeds: 5,
            peer_limit_global: if android { 400 } else { 1000 },
            peer_limit_torrent: if android { 100 } else { 200 },
            keep_awake: KeepAwake::Downloading,
            on_battery: OnBattery::AltSpeed,
            seed_ratio: Some(if android { 0.0 } else { 1.0 }),
            seed_minutes: None,
            proxy: None,
            blocklist_url: None,
            watch_dir: None,
            unmetered_only: true,
            close_to_tray: true,
        }
    }
}

/// Speed limits in KiB/s. `None` means unlimited.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct SpeedLimits {
    pub down_kib: Option<u32>,
    pub up_kib: Option<u32>,
}

/// When the device is kept awake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum KeepAwake {
    Off,
    #[default]
    Downloading,
    Active,
}

/// What to do when the device runs on battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OnBattery {
    Nothing,
    #[default]
    AltSpeed,
    Pause,
}

/// A week of speed modes, one cell per hour: 168 characters, Monday 00:00
/// first. `n` normal, `a` alternative limits, `s` seed only, `o` off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct WeekGrid(pub String);

impl WeekGrid {
    /// Cells in a week.
    pub const LEN: usize = 168;

    /// True when the grid is exactly 168 cells of `n`, `a`, `s` or `o`.
    pub fn is_valid(&self) -> bool {
        self.0.len() == Self::LEN
            && self
                .0
                .bytes()
                .all(|b| matches!(b, b'n' | b'a' | b's' | b'o'))
    }

    /// The cell at hour `i` of the week, or `None` past the end.
    pub fn cell(&self, i: usize) -> Option<char> {
        self.0.as_bytes().get(i).map(|&b| b as char)
    }
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
        self.validate_torrents()
    }

    fn validate_torrents(&self) -> Result<()> {
        let t = &self.torrents;
        if let Some(proxy) = &t.proxy
            && !matches!(proxy.scheme(), "socks5" | "socks5h")
        {
            return Err(Error::Invalid(
                "torrents.proxy must be a socks5 or socks5h URL".into(),
            ));
        }
        if t.listen_port != 0 && t.listen_port < 1024 {
            return Err(Error::Invalid(
                "torrents.listen_port must be 0 or between 1024 and 65535".into(),
            ));
        }
        if let Some(ratio) = t.seed_ratio
            && !(0.0..=100.0).contains(&ratio)
        {
            return Err(Error::Invalid(
                "torrents.seed_ratio must be between 0 and 100".into(),
            ));
        }
        if let Some(grid) = &t.schedule
            && !grid.is_valid()
        {
            return Err(Error::Invalid(
                "torrents.schedule must be 168 characters of n, a, s or o".into(),
            ));
        }
        if !(1..=100).contains(&t.max_active_downloads) {
            return Err(Error::Invalid(
                "torrents.max_active_downloads must be between 1 and 100".into(),
            ));
        }
        if !(1..=100).contains(&t.max_active_seeds) {
            return Err(Error::Invalid(
                "torrents.max_active_seeds must be between 1 and 100".into(),
            ));
        }
        if !(10..=2000).contains(&t.peer_limit_torrent) {
            return Err(Error::Invalid(
                "torrents.peer_limit_torrent must be between 10 and 2000".into(),
            ));
        }
        if !(20..=10000).contains(&t.peer_limit_global) {
            return Err(Error::Invalid(
                "torrents.peer_limit_global must be between 20 and 10000".into(),
            ));
        }
        if t.peer_limit_global < t.peer_limit_torrent {
            return Err(Error::Invalid(
                "torrents.peer_limit_global must be at least torrents.peer_limit_torrent".into(),
            ));
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

    #[test]
    fn torrent_defaults() {
        let t = TorrentSettings::default();
        assert!(t.enabled && t.upnp && t.dht && t.lsd && t.utp);
        assert!(t.unmetered_only && t.close_to_tray);
        assert!(!t.ask_on_add && !t.notice_accepted && !t.turtle);
        assert_eq!(t.listen_port, 0);
        assert_eq!(t.limits, SpeedLimits::default());
        assert_eq!(t.alt_limits.down_kib, Some(1024));
        assert_eq!(t.alt_limits.up_kib, Some(256));
        assert_eq!((t.max_active_downloads, t.max_active_seeds), (3, 5));
        assert_eq!(t.keep_awake, KeepAwake::Downloading);
        assert_eq!(t.on_battery, OnBattery::AltSpeed);
        assert_eq!(t.seed_minutes, None);
        assert!(t.schedule.is_none() && t.proxy.is_none() && t.download_dir.is_none());
        if cfg!(target_os = "android") {
            assert_eq!((t.peer_limit_global, t.peer_limit_torrent), (400, 100));
            assert_eq!(t.seed_ratio, Some(0.0));
        } else {
            assert_eq!((t.peer_limit_global, t.peer_limit_torrent), (1000, 200));
            assert_eq!(t.seed_ratio, Some(1.0));
        }
        assert!(Settings::default().validate().is_ok());
    }

    #[test]
    fn torrents_missing_or_partial_loads() {
        let s: Settings = serde_json::from_str(r#"{ "ui": {} }"#).unwrap();
        assert_eq!(s.torrents, TorrentSettings::default());

        let s: Settings = serde_json::from_str(
            r#"{ "torrents": { "listen_port": 51413, "keep_awake": "active", "on_battery": "pause" } }"#,
        )
        .unwrap();
        assert_eq!(s.torrents.listen_port, 51413);
        assert_eq!(s.torrents.keep_awake, KeepAwake::Active);
        assert_eq!(s.torrents.on_battery, OnBattery::Pause);
        assert_eq!(s.torrents.max_active_downloads, 3);
        assert_eq!(
            serde_json::to_value(KeepAwake::Off).unwrap(),
            serde_json::json!("off")
        );
        assert_eq!(
            serde_json::to_value(OnBattery::AltSpeed).unwrap(),
            serde_json::json!("alt_speed")
        );
    }

    #[test]
    fn torrent_validation_rejects_bad_values() {
        fn with(f: impl FnOnce(&mut TorrentSettings)) -> Settings {
            let mut s = Settings::default();
            f(&mut s.torrents);
            s
        }

        assert!(
            with(|t| t.proxy = Some("http://x:1".parse().unwrap()))
                .validate()
                .is_err()
        );
        assert!(
            with(|t| t.proxy = Some("socks5://127.0.0.1:9050".parse().unwrap()))
                .validate()
                .is_ok()
        );

        assert!(with(|t| t.listen_port = 80).validate().is_err());
        assert!(with(|t| t.listen_port = 1024).validate().is_ok());

        assert!(with(|t| t.seed_ratio = Some(101.0)).validate().is_err());
        assert!(with(|t| t.seed_ratio = Some(-1.0)).validate().is_err());
        assert!(with(|t| t.seed_ratio = Some(100.0)).validate().is_ok());
        assert!(with(|t| t.seed_ratio = None).validate().is_ok());

        assert!(
            with(|t| t.schedule = Some(WeekGrid("n".repeat(167))))
                .validate()
                .is_err()
        );
        assert!(
            with(|t| t.schedule = Some(WeekGrid("x".repeat(168))))
                .validate()
                .is_err()
        );
        assert!(
            with(|t| t.schedule = Some(WeekGrid("nasonasonasonaso".repeat(11))))
                .validate()
                .is_err()
        );
        let grid = WeekGrid("nasooasn".repeat(21));
        assert_eq!(grid.0.len(), 168);
        assert_eq!(grid.cell(1), Some('a'));
        assert_eq!(grid.cell(168), None);
        assert!(with(|t| t.schedule = Some(grid)).validate().is_ok());

        assert!(with(|t| t.max_active_downloads = 0).validate().is_err());
        assert!(with(|t| t.max_active_seeds = 101).validate().is_err());

        assert!(with(|t| t.peer_limit_torrent = 9).validate().is_err());
        assert!(with(|t| t.peer_limit_torrent = 2001).validate().is_err());
        assert!(with(|t| t.peer_limit_global = 19).validate().is_err());
        assert!(with(|t| t.peer_limit_global = 10001).validate().is_err());
        assert!(
            with(|t| {
                t.peer_limit_torrent = 500;
                t.peer_limit_global = 400;
            })
            .validate()
            .is_err(),
            "global must not be below per-torrent"
        );
    }
}
