// SPDX-License-Identifier: GPL-3.0-or-later

//! Torrent policies (plan §3.3): pure functions over a snapshot. No I/O and
//! no clock reads; the caller passes `now` in [`PlanCtx`].
//!
//! - [`queue`]: run modes, slots and the context order;
//! - [`seeding`]: when a finished torrent stops seeding;
//! - [`schedule`]: the weekly grid and the speed limits in effect;
//! - [`power`]: what running on battery does;
//! - [`network`]: the Android Wi-Fi gate.

pub(crate) mod network;
pub(crate) mod power;
pub(crate) mod queue;
pub(crate) mod schedule;
pub(crate) mod seeding;

use crate::model::{RunMode, ScheduleMode, SeedLimits};

pub(crate) use queue::{peer_share, plan};

/// What the policy knows about one torrent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TorrentFacts {
    pub id: String,
    pub run_mode: RunMode,
    pub queue_pos: i64,
    /// 100% of the selected files. A magnet without metadata is not.
    pub finished: bool,
    /// Our cumulative upload and download counters.
    pub uploaded_total: u64,
    pub downloaded_total: u64,
    /// Bytes of the selected files.
    pub size: u64,
    pub seeding_since: Option<i64>,
    /// The torrent's own seeding limits; `None` fields use the global ones.
    pub seed: SeedLimits,
    pub error: bool,
}

/// Context of one policy run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlanCtx {
    /// False on Android while only a metered network is available.
    pub network_allowed: bool,
    pub schedule_mode: ScheduleMode,
    /// Running on battery with the `pause` setting.
    pub on_battery_pause: bool,
    /// Epoch milliseconds.
    pub now_ms: i64,
}

impl PlanCtx {
    /// Nothing holds torrents back.
    #[cfg(test)]
    pub fn open(now_ms: i64) -> Self {
        Self {
            network_allowed: true,
            schedule_mode: ScheduleMode::Normal,
            on_battery_pause: false,
            now_ms,
        }
    }
}

/// The policy's decision for one torrent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Desired {
    /// Transfer.
    Run,
    /// An incomplete torrent waiting for a download slot.
    QueuedDownload,
    /// A complete torrent waiting for a seed slot.
    QueuedSeed,
    Paused,
    Stopped,
    /// The schedule says Off, or Seed only for a download.
    Scheduled,
    OnBattery,
    NetworkWait,
    /// The seed limit was reached.
    Done,
}

impl Desired {
    /// True when the torrent should transfer.
    pub fn runs(self) -> bool {
        self == Self::Run
    }
}

/// What the context holds back, in its order (plan §3.3): the network gate,
/// then the battery pause, then the schedule. It applies to every run mode,
/// forced torrents included.
pub(crate) fn hold(facts: &TorrentFacts, ctx: &PlanCtx) -> Option<Desired> {
    network::gate(ctx.network_allowed)
        .or_else(|| power::gate(ctx.on_battery_pause))
        .or_else(|| schedule::gate(ctx.schedule_mode, !facts.finished))
}
