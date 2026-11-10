// SPDX-License-Identifier: GPL-3.0-or-later

//! The queue (plan §3.3): which torrents run, from their run modes, the slot
//! limits and the context.

use hashlark_core::settings::TorrentSettings;

use super::{Desired, PlanCtx, TorrentFacts, hold, seeding};
use crate::model::RunMode;

/// How much lower a queued seed's ratio must be to take a running seed's
/// slot.
const SEED_SWAP_MARGIN: f64 = 0.05;

/// Decides what every torrent should do. The result has one entry per fact,
/// in the same order.
///
/// - The context comes first and holds every torrent, in this order: the
///   network gate, the battery pause, the schedule (Off holds all, Seed only
///   holds downloads). Forced torrents are held too: Force start skips the
///   queue limits, not the context.
/// - Then the run modes: `stopped` leaves the queue; `paused` keeps its slot;
///   `forced` runs over the limits and doesn't count against them; `auto`
///   torrents take the remaining slots.
/// - Incomplete `auto` torrents (magnets fetching metadata included) run in
///   `queue_pos` order, up to `max_active_downloads` minus the paused
///   incomplete ones.
/// - Complete `auto` torrents under their seed limit seed, up to
///   `max_active_seeds` minus the paused complete ones, lowest ratio first;
///   a running seed keeps its slot unless a queued one's ratio is lower by
///   more than `SEED_SWAP_MARGIN`. Those at their limit are `Done`.
/// - A forced torrent keeps running past its seed limit.
/// - A torrent with an error takes no slot.
pub(crate) fn plan(
    facts: &[TorrentFacts],
    cfg: &TorrentSettings,
    ctx: &PlanCtx,
) -> Vec<(String, Desired)> {
    let mut want = vec![Desired::Paused; facts.len()];
    let mut order: Vec<usize> = (0..facts.len()).collect();
    order.sort_by_key(|&i| (facts[i].queue_pos, i));

    let (mut paused_downloads, mut paused_seeds) = (0usize, 0usize);
    let mut downloads = Vec::new();
    let mut seeds = Vec::new();
    for &i in &order {
        let f = &facts[i];
        if let Some(held) = hold(f, ctx) {
            want[i] = held;
        } else if f.error {
            want[i] = Desired::Stopped;
        } else {
            match f.run_mode {
                RunMode::Stopped => want[i] = Desired::Stopped,
                RunMode::Forced => want[i] = Desired::Run,
                RunMode::Paused => {
                    want[i] = Desired::Paused;
                    if f.finished {
                        paused_seeds += 1;
                    } else {
                        paused_downloads += 1;
                    }
                }
                RunMode::Auto if !f.finished => downloads.push(i),
                RunMode::Auto if seeding::limit_reached(f, cfg, ctx.now_ms) => {
                    want[i] = Desired::Done;
                }
                RunMode::Auto => seeds.push(i),
            }
        }
    }

    let download_slots = usize::from(cfg.max_active_downloads).saturating_sub(paused_downloads);
    for (n, &i) in downloads.iter().enumerate() {
        want[i] = if n < download_slots {
            Desired::Run
        } else {
            Desired::QueuedDownload
        };
    }

    // Lowest ratio first, with running seeds ahead by `SEED_SWAP_MARGIN` so
    // two close ratios don't swap slots every tick; the stable sort keeps
    // queue order among equals.
    let key = |i: usize| {
        let ratio = seeding::ratio(&facts[i]);
        if facts[i].running {
            ratio - SEED_SWAP_MARGIN
        } else {
            ratio
        }
    };
    seeds.sort_by(|&a, &b| key(a).total_cmp(&key(b)));
    let seed_slots = usize::from(cfg.max_active_seeds).saturating_sub(paused_seeds);
    for (n, &i) in seeds.iter().enumerate() {
        want[i] = if n < seed_slots {
            Desired::Run
        } else {
            Desired::QueuedSeed
        };
    }

    facts
        .iter()
        .zip(want)
        .map(|(f, d)| (f.id.clone(), d))
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
    use crate::model::{ScheduleMode, SeedLimits};

    fn fact(id: &str, pos: i64, mode: RunMode, finished: bool) -> TorrentFacts {
        TorrentFacts {
            id: id.into(),
            run_mode: mode,
            queue_pos: pos,
            finished,
            uploaded_total: 0,
            downloaded_total: 0,
            size: 100,
            seeding_since: None,
            seed: SeedLimits::default(),
            error: false,
            running: false,
        }
    }

    fn dl(id: &str, pos: i64) -> TorrentFacts {
        fact(id, pos, RunMode::Auto, false)
    }

    fn seed(id: &str, pos: i64, up: u64) -> TorrentFacts {
        TorrentFacts {
            uploaded_total: up,
            downloaded_total: 100,
            ..fact(id, pos, RunMode::Auto, true)
        }
    }

    fn cfg(downloads: u16, seeds: u16) -> TorrentSettings {
        TorrentSettings {
            max_active_downloads: downloads,
            max_active_seeds: seeds,
            seed_ratio: None,
            ..TorrentSettings::default()
        }
    }

    fn run(facts: &[TorrentFacts], cfg: &TorrentSettings) -> Vec<Desired> {
        plan(facts, cfg, &PlanCtx::open(0))
            .into_iter()
            .map(|(_, d)| d)
            .collect()
    }

    use Desired::{
        Done, NetworkWait, OnBattery, Paused, QueuedDownload, QueuedSeed, Run, Scheduled, Stopped,
    };

    #[test]
    fn download_limit() {
        let facts = [dl("a", 0), dl("b", 1), dl("c", 2)];
        assert_eq!(run(&facts, &cfg(2, 5)), [Run, Run, QueuedDownload]);
        assert_eq!(run(&facts, &cfg(3, 5)), [Run, Run, Run]);
    }

    #[test]
    fn order_by_queue_pos_not_input_order() {
        let facts = [dl("a", 2), dl("b", 0), dl("c", 1)];
        assert_eq!(run(&facts, &cfg(2, 5)), [QueuedDownload, Run, Run]);
    }

    #[test]
    fn output_keeps_ids_in_input_order() {
        let facts = [dl("a", 2), dl("b", 0)];
        let got = plan(&facts, &cfg(1, 1), &PlanCtx::open(0));
        assert_eq!(got[0].0, "a");
        assert_eq!(got[1].0, "b");
    }

    #[test]
    fn metadata_fetch_counts_as_a_download() {
        // A magnet without metadata is simply not finished.
        let facts = [dl("magnet", 0), dl("b", 1)];
        assert_eq!(run(&facts, &cfg(1, 5)), [Run, QueuedDownload]);
    }

    #[test]
    fn a_paused_torrent_holds_its_slot() {
        let facts = [fact("p", 0, RunMode::Paused, false), dl("a", 1), dl("b", 2)];
        assert_eq!(run(&facts, &cfg(2, 5)), [Paused, Run, QueuedDownload]);
        // Seeds and downloads have separate slots.
        let facts = [fact("p", 0, RunMode::Paused, true), dl("a", 1)];
        assert_eq!(run(&facts, &cfg(1, 1)), [Paused, Run]);
    }

    #[test]
    fn a_stopped_torrent_frees_its_slot_and_never_starts() {
        let facts = [
            fact("s", 0, RunMode::Stopped, false),
            dl("a", 1),
            dl("b", 2),
        ];
        assert_eq!(run(&facts, &cfg(2, 5)), [Stopped, Run, Run]);
        // However much room there is.
        assert_eq!(run(&facts[..1], &cfg(100, 100)), [Stopped]);
    }

    #[test]
    fn a_forced_torrent_runs_over_the_limit_and_does_not_count() {
        let facts = [fact("f", 0, RunMode::Forced, false), dl("a", 1), dl("b", 2)];
        assert_eq!(run(&facts, &cfg(1, 5)), [Run, Run, QueuedDownload]);
        let facts = [fact("f", 0, RunMode::Forced, true), seed("a", 1, 0)];
        assert_eq!(run(&facts, &cfg(1, 1)), [Run, Run]);
    }

    #[test]
    fn a_forced_torrent_ignores_the_seed_limit() {
        let c = TorrentSettings {
            seed_ratio: Some(1.0),
            ..cfg(1, 1)
        };
        let mut f = seed("f", 0, 1000);
        f.run_mode = RunMode::Forced;
        assert_eq!(run(&[f, seed("a", 1, 1000)], &c), [Run, Done]);
    }

    #[test]
    fn seeds_run_lowest_ratio_first() {
        let facts = [seed("a", 0, 300), seed("b", 1, 100), seed("c", 2, 200)];
        assert_eq!(run(&facts, &cfg(1, 2)), [QueuedSeed, Run, Run]);
        // Equal ratios keep queue order.
        let facts = [seed("a", 1, 100), seed("b", 0, 100)];
        assert_eq!(run(&facts, &cfg(1, 1)), [QueuedSeed, Run]);
    }

    #[test]
    fn a_running_seed_keeps_its_slot_within_the_margin() {
        // Ratios 1.00 (running) and 0.97 (queued): within the margin.
        let mut a = seed("a", 0, 100);
        a.running = true;
        let b = seed("b", 1, 97);
        assert_eq!(run(&[a.clone(), b], &cfg(1, 1)), [Run, QueuedSeed]);
        // Next tick, nothing moved: still no swap.
        let b = seed("b", 1, 97);
        assert_eq!(run(&[a.clone(), b], &cfg(1, 1)), [Run, QueuedSeed]);
        // 0.90 is lower by more than the margin: it takes the slot.
        let b = seed("b", 1, 90);
        assert_eq!(run(&[a, b], &cfg(1, 1)), [QueuedSeed, Run]);
    }

    #[test]
    fn seeds_and_downloads_have_their_own_limits() {
        let facts = [dl("d", 0), seed("s", 1, 0), dl("e", 2)];
        assert_eq!(run(&facts, &cfg(1, 1)), [Run, Run, QueuedDownload]);
    }

    #[test]
    fn finished_at_the_limit_is_done_and_frees_the_slot() {
        let c = TorrentSettings {
            seed_ratio: Some(1.0),
            ..cfg(1, 1)
        };
        let facts = [seed("a", 0, 100), seed("b", 1, 10)];
        assert_eq!(run(&facts, &c), [Done, Run]);
    }

    #[test]
    fn an_errored_torrent_takes_no_slot() {
        let mut bad = dl("bad", 0);
        bad.error = true;
        assert_eq!(run(&[bad, dl("a", 1)], &cfg(1, 1)), [Stopped, Run]);
    }

    fn ctx(network: bool, battery: bool, mode: ScheduleMode) -> PlanCtx {
        PlanCtx {
            network_allowed: network,
            schedule_mode: mode,
            on_battery_pause: battery,
            now_ms: 0,
        }
    }

    fn mixed() -> Vec<TorrentFacts> {
        vec![
            dl("d", 0),
            seed("s", 1, 0),
            fact("f", 2, RunMode::Forced, false),
            fact("p", 3, RunMode::Paused, false),
            fact("x", 4, RunMode::Stopped, false),
        ]
    }

    fn all(c: &PlanCtx) -> Vec<Desired> {
        plan(&mixed(), &cfg(3, 5), c)
            .into_iter()
            .map(|(_, d)| d)
            .collect()
    }

    #[test]
    fn context_order() {
        // 1. No network beats everything below it.
        let c = ctx(false, true, ScheduleMode::Off);
        assert!(all(&c).iter().all(|d| *d == NetworkWait));
        // 2. Then the battery pause.
        let c = ctx(true, true, ScheduleMode::Off);
        assert!(all(&c).iter().all(|d| *d == OnBattery));
        // 3. Then schedule Off, forced torrents included.
        let c = ctx(true, false, ScheduleMode::Off);
        assert!(all(&c).iter().all(|d| *d == Scheduled));
        // 4. Seed only holds downloads only.
        let c = ctx(true, false, ScheduleMode::SeedOnly);
        assert_eq!(all(&c), [Scheduled, Run, Scheduled, Scheduled, Scheduled]);
        // Alternative speed and normal hold nothing.
        for mode in [ScheduleMode::Normal, ScheduleMode::Alt] {
            let c = ctx(true, false, mode);
            assert_eq!(all(&c), [Run, Run, Run, Paused, Stopped]);
        }
    }

    #[test]
    fn peer_share_formula() {
        assert_eq!(peer_share(1000, 200, 3), 200);
        assert_eq!(peer_share(1000, 200, 100), 20);
        assert_eq!(peer_share(1000, 200, 0), 200);
        assert_eq!(peer_share(1000, 50, 2), 50);
        assert_eq!(peer_share(1000, 200, 10), 100);
    }
}
