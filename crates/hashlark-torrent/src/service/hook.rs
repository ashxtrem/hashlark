// SPDX-License-Identifier: GPL-3.0-or-later

//! The policy hook: where T4's queue, seeding, schedule, power and network
//! policies (`src/policy/*`) plug into the tick.

use hashlark_core::settings::TorrentSettings;

use crate::model::RunMode;

/// What the policy knows about one torrent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TorrentFacts {
    pub id: String,
    pub run_mode: RunMode,
    pub queue_pos: i64,
    /// Metadata is known.
    pub has_metadata: bool,
    pub finished: bool,
    /// The seed limit was reached.
    pub done: bool,
    pub uploaded_total: u64,
    pub downloaded_total: u64,
    /// Bytes of the selected files.
    pub size: u64,
    pub seeding_since: Option<i64>,
    pub error: bool,
}

/// Context of one policy run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PolicyCtx {
    pub network_allowed: bool,
    pub on_battery: bool,
}

/// The policy's decision for one torrent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Desired {
    Run,
    Paused,
    Stopped,
}

/// The hook where the queue, seeding, schedule, power and network policies
/// plug in (T4, `src/policy/*`): facts + settings in, the desired state of
/// every torrent out. The tick and every operation apply the result with
/// `pause` / `unpause`.
///
/// For now a pass-through: `auto` and `forced` run, `paused` and `stopped`
/// don't.
pub(crate) fn desired_states(
    facts: &[TorrentFacts],
    _settings: &TorrentSettings,
    _ctx: &PolicyCtx,
) -> Vec<(String, Desired)> {
    facts
        .iter()
        .map(|f| {
            let want = match f.run_mode {
                RunMode::Auto | RunMode::Forced => Desired::Run,
                RunMode::Paused => Desired::Paused,
                RunMode::Stopped => Desired::Stopped,
            };
            (f.id.clone(), want)
        })
        .collect()
}

/// Per-torrent connection limit sharing the global cap (plan §2):
/// `min(own, max(20, global / running))`.
pub(crate) fn peer_share(global: u32, own: u32, running: usize) -> u32 {
    let running = u32::try_from(running.max(1)).unwrap_or(u32::MAX);
    own.min((global / running).max(20))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_share_formula() {
        assert_eq!(peer_share(1000, 200, 3), 200);
        assert_eq!(peer_share(1000, 200, 100), 20);
        assert_eq!(peer_share(1000, 200, 0), 200);
        assert_eq!(peer_share(1000, 50, 2), 50);
    }

    #[test]
    fn pass_through_policy() {
        let fact = |id: &str, run_mode| TorrentFacts {
            id: id.into(),
            run_mode,
            queue_pos: 0,
            has_metadata: true,
            finished: false,
            done: false,
            uploaded_total: 0,
            downloaded_total: 0,
            size: 0,
            seeding_since: None,
            error: false,
        };
        let facts = [
            fact("a", RunMode::Auto),
            fact("p", RunMode::Paused),
            fact("s", RunMode::Stopped),
            fact("f", RunMode::Forced),
        ];
        let ctx = PolicyCtx {
            network_allowed: true,
            on_battery: false,
        };
        let got = desired_states(&facts, &TorrentSettings::default(), &ctx);
        let want = [
            Desired::Run,
            Desired::Paused,
            Desired::Stopped,
            Desired::Run,
        ];
        assert_eq!(got.iter().map(|g| g.1).collect::<Vec<_>>(), want);
    }
}
