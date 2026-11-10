// SPDX-License-Identifier: GPL-3.0-or-later

//! When a finished torrent stops seeding (plan §3.3, Seeding).

use hashlark_core::settings::TorrentSettings;

use super::TorrentFacts;

/// `uploaded_total / max(downloaded_total, selected size)`; 0 while both
/// are 0.
pub(crate) fn ratio(f: &TorrentFacts) -> f64 {
    let denominator = f.downloaded_total.max(f.size);
    if denominator == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = f.uploaded_total as f64 / denominator as f64;
    ratio
}

/// True when a finished torrent has reached its seed limit: the share ratio
/// or the minutes of seeding, whichever comes first. A per-torrent value
/// wins over the global one; `None` is no limit. A ratio of 0 is reached as
/// soon as the download completes.
pub(crate) fn limit_reached(f: &TorrentFacts, cfg: &TorrentSettings, now_ms: i64) -> bool {
    if !f.finished {
        return false;
    }
    if let Some(limit) = f.seed.ratio.or(cfg.seed_ratio)
        && ratio(f) >= f64::from(limit)
    {
        return true;
    }
    if let Some(minutes) = f.seed.minutes.or(cfg.seed_minutes)
        && let Some(since) = f.seeding_since
    {
        return now_ms.saturating_sub(since) >= i64::from(minutes) * 60_000;
    }
    false
}

#[cfg(test)]
mod tests {
    use hashlark_core::settings::TorrentSettings;

    use super::*;
    use crate::model::{RunMode, SeedLimits};
    use crate::policy::{Desired, PlanCtx, plan};

    fn facts(up: u64, down: u64, size: u64) -> TorrentFacts {
        TorrentFacts {
            id: "t".into(),
            run_mode: RunMode::Auto,
            queue_pos: 0,
            finished: true,
            uploaded_total: up,
            downloaded_total: down,
            size,
            seeding_since: Some(0),
            seed: SeedLimits::default(),
            error: false,
            running: false,
        }
    }

    fn cfg(ratio: Option<f32>, minutes: Option<u32>) -> TorrentSettings {
        TorrentSettings {
            seed_ratio: ratio,
            seed_minutes: minutes,
            ..TorrentSettings::default()
        }
    }

    #[test]
    fn ratio_uses_the_larger_of_downloaded_and_size() {
        assert!((ratio(&facts(50, 100, 100)) - 0.5).abs() < 1e-9);
        // Created or re-checked torrents: nothing downloaded, size counts.
        assert!((ratio(&facts(200, 0, 100)) - 2.0).abs() < 1e-9);
        // Downloaded more than the size (re-downloaded pieces).
        assert!((ratio(&facts(100, 200, 100)) - 0.5).abs() < 1e-9);
        assert!(ratio(&facts(0, 0, 0)).abs() < 1e-9);
    }

    #[test]
    fn stops_at_the_global_ratio() {
        let c = cfg(Some(1.0), None);
        assert!(!limit_reached(&facts(99, 100, 100), &c, 0));
        assert!(limit_reached(&facts(100, 100, 100), &c, 0));
        assert!(limit_reached(&facts(150, 100, 100), &c, 0));
    }

    #[test]
    fn no_limit_never_stops() {
        let c = cfg(None, None);
        assert!(!limit_reached(&facts(10_000, 100, 100), &c, i64::MAX));
    }

    #[test]
    fn ratio_zero_stops_at_completion() {
        let c = cfg(Some(0.0), None);
        assert!(limit_reached(&facts(0, 100, 100), &c, 0));
        assert!(limit_reached(&facts(0, 0, 0), &c, 0));
        // Not before it completes.
        let mut f = facts(0, 50, 100);
        f.finished = false;
        assert!(!limit_reached(&f, &c, 0));
    }

    #[test]
    fn stops_after_the_minutes() {
        let c = cfg(None, Some(10));
        let mut f = facts(0, 100, 100);
        f.seeding_since = Some(1_000);
        assert!(!limit_reached(&f, &c, 1_000 + 10 * 60_000 - 1));
        assert!(limit_reached(&f, &c, 1_000 + 10 * 60_000));
        // Seeding hasn't started.
        f.seeding_since = None;
        assert!(!limit_reached(&f, &c, i64::MAX));
    }

    #[test]
    fn whichever_comes_first() {
        let c = cfg(Some(2.0), Some(10));
        let mut f = facts(250, 100, 100);
        f.seeding_since = Some(0);
        assert!(limit_reached(&f, &c, 1));
        let f = facts(10, 100, 100);
        assert!(limit_reached(&f, &c, 10 * 60_000));
    }

    #[test]
    fn per_torrent_override_wins() {
        let c = cfg(Some(1.0), Some(10));
        let mut f = facts(100, 100, 100);
        f.seed = SeedLimits {
            ratio: Some(3.0),
            minutes: None,
        };
        // The global minutes still apply (override per field).
        assert!(!limit_reached(&f, &c, 1));
        f.seed.minutes = Some(1_000);
        assert!(!limit_reached(&f, &c, 10 * 60_000));
        // A lower override stops earlier than the global ratio.
        let c = cfg(Some(5.0), None);
        f.seed = SeedLimits {
            ratio: Some(0.5),
            minutes: None,
        };
        assert!(limit_reached(&f, &c, 0));
    }

    #[test]
    fn an_override_cannot_remove_the_global_limit_with_none() {
        // `None` in a torrent means "use the global value".
        let c = cfg(Some(1.0), None);
        let f = facts(100, 100, 100);
        assert!(limit_reached(&f, &c, 0));
    }

    #[test]
    fn a_stopped_finished_torrent_does_not_seed() {
        let c = cfg(None, None);
        let mut f = facts(0, 100, 100);
        f.run_mode = RunMode::Stopped;
        let got = plan(&[f], &c, &PlanCtx::open(0));
        assert_eq!(got, vec![("t".to_string(), Desired::Stopped)]);
    }

    #[test]
    fn a_torrent_at_its_limit_is_done_not_seeding() {
        let c = cfg(Some(1.0), None);
        let got = plan(&[facts(100, 100, 100)], &c, &PlanCtx::open(0));
        assert_eq!(got[0].1, Desired::Done);
        let got = plan(&[facts(10, 100, 100)], &c, &PlanCtx::open(0));
        assert_eq!(got[0].1, Desired::Run);
    }
}
