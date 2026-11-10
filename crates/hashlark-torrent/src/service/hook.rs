// SPDX-License-Identifier: GPL-3.0-or-later

//! The policy hook: where the pure policies (`src/policy/*`) meet the
//! service. It builds the context from the settings, the device state and
//! the injected clock, applies the effective session rate limits, and marks
//! torrents that reached their seed limit.

use hashlark_core::settings::{SpeedLimits, TorrentSettings};

use super::{Ctx, Running, TorrentService, limits_config};
use crate::clock::Moment;
use crate::model::{RunMode, ScheduleMode};
use crate::policy::power::{BatteryEffect, battery_effect};
use crate::policy::schedule::{effective_limits, mode_at};
use crate::policy::seeding;
use crate::store;

// The policy: facts + settings + context in, the desired state of every
// torrent out. The tick and every operation apply the result with
// `pause` / `unpause`.
pub(crate) use crate::policy::{
    Desired, PlanCtx, TorrentFacts, peer_share, plan as desired_states,
};

impl TorrentService {
    /// The service's clock reading (the system clock unless a test set one).
    pub(crate) fn now(&self) -> Moment {
        let clock = self.inner.clock.lock().expect("lock").clone();
        clock()
    }

    /// Battery and network state reported from outside.
    pub(crate) fn device(&self) -> Ctx {
        *self.inner.ctx.lock().expect("lock")
    }

    /// What the weekly schedule says at `now`.
    pub(crate) fn schedule_mode(&self, settings: &TorrentSettings) -> ScheduleMode {
        mode_at(settings.schedule.as_ref(), self.now().local)
    }

    /// The policy context at `now`, and the session speed limits in effect.
    pub(crate) fn context(
        &self,
        settings: &TorrentSettings,
        now: Moment,
    ) -> (PlanCtx, SpeedLimits) {
        let device = self.device();
        let battery = battery_effect(device.on_battery, settings.on_battery);
        let schedule_mode = mode_at(settings.schedule.as_ref(), now.local);
        let ctx = PlanCtx {
            network_allowed: device.network_allowed,
            schedule_mode,
            on_battery_pause: battery == BatteryEffect::Pause,
            now_ms: now.ms,
        };
        (ctx, effective_limits(settings, schedule_mode, battery))
    }

    /// Sets the session rate limits when they differ from the last ones.
    pub(crate) fn apply_limits(&self, run: &Running, limits: SpeedLimits) {
        {
            let mut st = run.state.lock().expect("lock");
            if st.applied_limits == Some(limits) {
                return;
            }
            st.applied_limits = Some(limits);
        }
        let config = limits_config(limits);
        run.session.ratelimits.set_download_bps(config.download_bps);
        run.session.ratelimits.set_upload_bps(config.upload_bps);
        tracing::debug!(?limits, "session speed limits changed");
    }

    /// The session speed limits in effect now, once the client runs.
    pub fn applied_limits(&self) -> Option<SpeedLimits> {
        self.running()?.state.lock().expect("lock").applied_limits
    }

    /// Sets `done` on `auto` torrents that reached their seed limit, and
    /// clears it when the limit no longer applies (a higher limit, a new
    /// minutes value) or the torrent is force-started. Persists what
    /// changed. The caller holds `op_lock`.
    pub(crate) async fn mark_done(
        &self,
        run: &Running,
        facts: &[TorrentFacts],
        settings: &TorrentSettings,
        now_ms: i64,
    ) {
        let changed: Vec<store::Row> = {
            let mut st = run.state.lock().expect("lock");
            facts
                .iter()
                .filter_map(|f| {
                    let reached = match f.run_mode {
                        RunMode::Auto if f.finished => seeding::limit_reached(f, settings, now_ms),
                        // A forced torrent seeds past its limit: not done.
                        RunMode::Forced => false,
                        _ => return None,
                    };
                    let row = st.rows.get_mut(&f.id)?;
                    (row.done != reached).then(|| {
                        row.done = reached;
                        row.clone()
                    })
                })
                .collect()
        };
        for row in changed {
            if let Err(e) = store::update(self.engine().store(), &row).await {
                tracing::warn!(error = %e, "could not save torrent");
            }
        }
    }
}
