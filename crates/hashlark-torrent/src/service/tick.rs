// SPDX-License-Identifier: GPL-3.0-or-later

//! The 1 s tick: cumulative totals, completion and error events, and the
//! snapshot broadcast.

use std::sync::Arc;
use std::time::Instant;

use hashlark_core::Result;
use librqbit::ManagedTorrent;

use super::{FLUSH_EVERY, Running, State, TorrentService};
use crate::model::TorrentEvent;
use crate::store::{self, Row};

/// Cumulative up/down totals: librqbit's counters restart at 0 whenever a
/// torrent goes live, so we add up what each live period moved.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Counter {
    pub down: u64,
    pub up: u64,
    last_down: u64,
    last_up: u64,
}

impl Counter {
    pub fn from_row(row: &Row) -> Self {
        Self {
            down: row.downloaded_total,
            up: row.uploaded_total,
            ..Self::default()
        }
    }

    /// Adds what moved since the last look. `None` = not live: the next
    /// live period starts from 0.
    pub fn observe(&mut self, live: Option<(u64, u64)>) {
        match live {
            Some((down, up)) => {
                self.down += down.checked_sub(self.last_down).unwrap_or(down);
                self.up += up.checked_sub(self.last_up).unwrap_or(up);
                self.last_down = down;
                self.last_up = up;
            }
            None => self.reset_live(),
        }
    }

    pub fn reset_live(&mut self) {
        self.last_down = 0;
        self.last_up = 0;
    }
}

/// Live counters (fetched, uploaded) of a torrent, if it is live.
pub(crate) fn live_counters(handle: &ManagedTorrent) -> Option<(u64, u64)> {
    handle.live().map(|live| {
        let s = live.stats_snapshot();
        (s.fetched_bytes, s.uploaded_bytes)
    })
}

impl TorrentService {
    pub(super) async fn tick(&self, run: &Arc<Running>) {
        let _op = run.op_lock.lock().await;
        let (dirty, events) = self.observe(run);
        let store = self.inner.engine.store();
        for row in dirty {
            if let Err(e) = store::update(store, &row).await {
                tracing::warn!(error = %e, "could not save torrent");
            }
        }
        for event in events {
            let _ = self.inner.events.send(event);
        }
        self.reconcile_locked(run).await;
        let due = run
            .state
            .lock()
            .expect("lock")
            .last_flush
            .is_none_or(|t| t.elapsed() >= FLUSH_EVERY);
        if due && let Err(e) = self.flush(run).await {
            tracing::warn!(error = %e, "could not save torrent totals");
        }
        if self.inner.events.receiver_count() > 0 {
            let snapshot = TorrentEvent::Snapshot {
                session: self.session_stats(run),
                torrents: self.summaries(run),
            };
            let _ = self.inner.events.send(snapshot);
        }
    }

    /// Updates the counters from the live torrents.
    pub(crate) fn observe_counters(&self, run: &Running) {
        let handles = self.handles(run);
        let mut st = run.state.lock().expect("lock");
        for (hash, handle) in &handles {
            if let Some(c) = st.counters.get_mut(hash) {
                c.observe(live_counters(handle));
            }
        }
    }

    /// Updates counters, completion times and names; returns changed rows
    /// and the events to send.
    fn observe(&self, run: &Running) -> (Vec<Row>, Vec<TorrentEvent>) {
        self.observe_counters(run);
        let handles = self.handles(run);
        let now = self.now().ms;
        let mut dirty = Vec::new();
        let mut events = Vec::new();
        let mut st = run.state.lock().expect("lock");
        let State {
            rows,
            errors,
            reported,
            incomplete,
            ..
        } = &mut *st;
        for (hash, row) in rows.iter_mut() {
            let mut error = errors.get(hash).cloned();
            if let Some(handle) = handles.get(hash) {
                let stats = handle.stats();
                let mut changed = false;
                if let Some(name) = handle.name()
                    && row.name != name
                    && row.name == *hash
                {
                    row.name = name;
                    changed = true;
                }
                let has_metadata = handle.metadata.load().is_some();
                let checked = matches!(
                    stats.state,
                    librqbit::TorrentStatsState::Live | librqbit::TorrentStatsState::Paused
                );
                if has_metadata && checked && !stats.finished {
                    incomplete.insert(hash.clone());
                }
                if checked && stats.finished && row.completed_at.is_none() {
                    row.completed_at = Some(now);
                    row.seeding_since = Some(now);
                    changed = true;
                    if incomplete.remove(hash) {
                        events.push(TorrentEvent::Completed {
                            id: hash.clone(),
                            name: row.name.clone(),
                        });
                    }
                }
                if changed {
                    dirty.push(row.clone());
                }
                if error.is_none() {
                    error = stats.error;
                }
            }
            match error {
                Some(message) => {
                    if reported.insert(hash.clone()) {
                        events.push(TorrentEvent::Error {
                            id: hash.clone(),
                            message,
                        });
                    }
                }
                None => {
                    reported.remove(hash);
                }
            }
        }
        (dirty, events)
    }

    /// Writes every torrent's cumulative totals.
    pub(crate) async fn flush(&self, run: &Running) -> Result<()> {
        let totals: Vec<(String, u64, u64)> = {
            let mut st = run.state.lock().expect("lock");
            st.last_flush = Some(Instant::now());
            let State { rows, counters, .. } = &mut *st;
            counters
                .iter()
                .filter_map(|(hash, c)| {
                    let row = rows.get_mut(hash)?;
                    if row.downloaded_total == c.down && row.uploaded_total == c.up {
                        return None;
                    }
                    row.downloaded_total = c.down;
                    row.uploaded_total = c.up;
                    Some((hash.clone(), c.down, c.up))
                })
                .collect()
        };
        store::set_totals(self.inner.engine.store(), &totals).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_survives_resets() {
        let mut c = Counter {
            down: 100,
            up: 10,
            ..Counter::default()
        };
        c.observe(Some((50, 5)));
        c.observe(Some((80, 5)));
        assert_eq!((c.down, c.up), (180, 15));
        // A pause/unpause resets librqbit's counters.
        c.observe(Some((20, 1)));
        assert_eq!((c.down, c.up), (200, 16));
        c.observe(None);
        c.observe(Some((30, 2)));
        assert_eq!((c.down, c.up), (230, 18));
    }
}
