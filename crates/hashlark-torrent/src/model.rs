// SPDX-License-Identifier: GPL-3.0-or-later

//! JSON types of the torrent API (plan §3.2). Ids are info hashes as 40
//! lowercase hex characters; times are epoch milliseconds.

use std::path::PathBuf;

use hashlark_core::settings::SpeedLimits;
use serde::{Deserialize, Serialize};

/// How the user wants a torrent to run (plan §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// The queue decides (set by add, Resume and Start).
    #[default]
    Auto,
    /// Paused by the user; keeps its active slot.
    Paused,
    /// Stopped by the user; leaves the queue and never auto-starts.
    Stopped,
    /// Force-started: runs over the queue limits.
    Forced,
}

impl RunMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Forced => "forced",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "paused" => Self::Paused,
            "stopped" => Self::Stopped,
            "forced" => Self::Forced,
            _ => Self::Auto,
        }
    }
}

/// Where a torrent came from (the `source` column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Magnet,
    Torrent,
    Result,
    Created,
    Watch,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Magnet => "magnet",
            Self::Torrent => "torrent",
            Self::Result => "result",
            Self::Created => "created",
            Self::Watch => "watch",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "magnet" => Self::Magnet,
            "result" => Self::Result,
            "created" => Self::Created,
            "watch" => Self::Watch,
            _ => Self::Torrent,
        }
    }
}

/// What a torrent is doing now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TorrentState {
    /// A magnet still fetching its metadata.
    Metadata,
    /// Checking existing files.
    Checking,
    Downloading,
    Seeding,
    /// Waiting for a free slot in the queue.
    Queued,
    Paused,
    Stopped,
    /// Complete and not seeding because it was stopped.
    Finished,
    /// Complete and not seeding: the seed limit was reached.
    Done,
    /// Held back: the schedule says Off ("Scheduled: off").
    ScheduledOff,
    /// Held back: the schedule says Seed only ("Scheduled: seed only").
    ScheduledSeedOnly,
    /// Held back: paused while running on battery ("On battery").
    OnBattery,
    /// Held back: no allowed network ("Waiting for Wi-Fi").
    WaitingForNetwork,
    Error,
}

/// One row of the torrent list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TorrentSummary {
    /// Info hash, 40 lowercase hex characters.
    pub id: String,
    pub name: String,
    pub state: TorrentState,
    pub run_mode: RunMode,
    pub source: Origin,
    /// 0.0–1.0 of the selected files.
    pub progress: f64,
    /// Bytes of the selected files (0 until the metadata is known).
    pub size: u64,
    /// Bytes of the selected files we have.
    pub have: u64,
    /// Bytes downloaded over all runs.
    pub downloaded_total: u64,
    /// Bytes uploaded over all runs.
    pub uploaded_total: u64,
    pub ratio: f64,
    pub down_bps: u64,
    pub up_bps: u64,
    /// Seconds to completion at the current speed.
    pub eta_secs: Option<u64>,
    /// Connected peers.
    pub peers: u32,
    /// Connected peers that have the whole torrent.
    pub seeds: u32,
    pub queue_pos: i64,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub save_dir: PathBuf,
    pub added_at: i64,
    pub completed_at: Option<i64>,
    /// The seed limit was reached.
    pub done: bool,
    pub error: Option<String>,
}

/// One file of a torrent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TorrentFile {
    /// Index used by `files` in [`TorrentPatch`] and [`AddRequest`].
    pub index: usize,
    /// Path inside the torrent, `/`-separated.
    pub path: String,
    pub size: u64,
    pub have: u64,
    pub selected: bool,
}

/// Per-torrent seeding limits. `None` uses the global setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct SeedLimits {
    pub ratio: Option<f32>,
    pub minutes: Option<u32>,
}

/// Everything about one torrent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TorrentDetails {
    #[serde(flatten)]
    pub summary: TorrentSummary,
    pub files: Vec<TorrentFile>,
    /// Tracker URLs (librqbit reports no per-tracker status).
    pub trackers: Vec<String>,
    /// Per-torrent speed limits in KiB/s. `None` is unlimited.
    pub limits: SpeedLimits,
    /// Own connection limit. `None` uses the global per-torrent limit.
    pub peer_limit: Option<u32>,
    pub seed: SeedLimits,
    pub magnet: Option<String>,
    pub result_id: Option<String>,
    pub seeding_since: Option<i64>,
}

/// Where a torrent to add comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AddSource {
    /// A `magnet:` link. `x.pe` peer addresses are used as initial peers.
    Magnet(String),
    /// A `.torrent` file, base64-encoded.
    Torrent(String),
    /// A result of a recent search, or a favourite.
    ResultId(String),
}

/// Body of `add`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AddRequest {
    pub source: AddSource,
    /// Folder to save into; the default download folder when absent.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub save_dir: Option<PathBuf>,
    /// File indices to download; all when absent.
    #[serde(default)]
    pub files: Option<Vec<usize>>,
    /// Add in the `paused` run mode.
    #[serde(default)]
    pub paused: bool,
}

impl AddRequest {
    pub fn new(source: AddSource) -> Self {
        Self {
            source,
            save_dir: None,
            files: None,
            paused: false,
        }
    }
}

/// Result of `add`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AddResponse {
    pub torrent: TorrentSummary,
    /// The info hash was already in the client; nothing was added.
    pub already: bool,
}

/// Run-mode changes (plan §2: Pause ≠ Stop, Force start).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TorrentAction {
    Pause,
    Resume,
    Stop,
    Start,
    ForceStart,
}

impl TorrentAction {
    pub fn run_mode(self) -> RunMode {
        match self {
            Self::Pause => RunMode::Paused,
            Self::Resume | Self::Start => RunMode::Auto,
            Self::Stop => RunMode::Stopped,
            Self::ForceStart => RunMode::Forced,
        }
    }
}

/// A relative queue move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueueStep {
    Top,
    Up,
    Down,
    Bottom,
}

/// A queue move: a step, or an absolute 0-based index (drag reorder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum QueueMove {
    Step(QueueStep),
    Index(u32),
}

/// Body of `update`. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct TorrentPatch {
    pub action: Option<TorrentAction>,
    /// Selected file indices (at least one).
    pub files: Option<Vec<usize>>,
    /// Per-torrent speed limits; replaces both values.
    pub limits: Option<SpeedLimits>,
    /// Own connection limit, 10–2000; 0 returns to the global default.
    pub peer_limit: Option<u32>,
    /// Seeding limits; replaces both values.
    pub seed: Option<SeedLimits>,
    pub queue_pos: Option<QueueMove>,
}

/// An action on many torrents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BulkAction {
    Pause,
    Resume,
    Stop,
    Start,
    ForceStart,
    Remove {
        /// Also delete the downloaded files. Ignored for torrents created
        /// here: their files are the user's own sources.
        #[serde(default)]
        delete_files: bool,
    },
    /// Turn turtle mode (alternative speed) on or off; `ids` is ignored.
    Turtle(bool),
}

/// Body of `bulk`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkRequest {
    /// Torrents to act on; every torrent when absent.
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    pub action: BulkAction,
}

/// One failure of a bulk action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkFailure {
    pub id: String,
    pub message: String,
}

/// Result of `bulk`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkResult {
    pub ok: Vec<String>,
    pub failed: Vec<BulkFailure>,
}

/// One connected peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PeerInfo {
    pub addr: String,
    pub client: Option<String>,
    /// `tcp`, `utp` or `socks`.
    pub conn_kind: Option<String>,
    pub state: String,
    /// Bytes received from this peer in this session.
    pub downloaded: u64,
    /// Bytes sent to this peer in this session.
    pub uploaded: u64,
}

/// What the weekly schedule says now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMode {
    #[default]
    Normal,
    Alt,
    SeedOnly,
    Off,
}

/// Torrents per state, for the filter tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StateCounts {
    pub all: u32,
    pub downloading: u32,
    pub seeding: u32,
    pub queued: u32,
    pub paused: u32,
    pub stopped: u32,
    pub finished: u32,
    pub error: u32,
}

/// Session-wide numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionStats {
    /// The client is on and its session runs.
    pub running: bool,
    pub down_bps: u64,
    pub up_bps: u64,
    pub listen_port: Option<u16>,
    pub dht_nodes: Option<u64>,
    pub schedule_mode: ScheduleMode,
    pub turtle: bool,
    pub on_battery: bool,
    pub network_allowed: bool,
    pub peers_connected: u32,
    pub counts: StateCounts,
}

/// Battery state reported by the desktop shell or Android.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PowerState {
    pub on_battery: bool,
}

/// Body of `create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateRequest {
    /// File or folder to share.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub path: PathBuf,
    #[serde(default)]
    pub trackers: Vec<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub piece_length: Option<u32>,
}

/// Result of `create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreatedTorrent {
    pub torrent: TorrentSummary,
    pub magnet: String,
    pub already: bool,
}

/// One live-update event (`subscribe`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TorrentEvent {
    /// Sent every second.
    Snapshot {
        session: SessionStats,
        torrents: Vec<TorrentSummary>,
    },
    Completed {
        id: String,
        name: String,
    },
    Error {
        id: String,
        message: String,
    },
}
