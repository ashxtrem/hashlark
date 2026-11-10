// SPDX-License-Identifier: GPL-3.0-or-later

//! Applying the policy: pause / unpause, metadata fetches of magnets, and
//! re-adding a torrent when its add-time options change.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use hashlark_core::Result;
use hashlark_core::settings::SpeedLimits;
use librqbit::{AddTorrent, AddTorrentOptions, ManagedTorrent, ManagedTorrentState};
use tokio_util::sync::CancellationToken;

use super::{
    Desired, Running, TorrentFacts, TorrentService, desired_states, invalid, limits_config,
    live_counters, magnet_peers, peer_share, torrent_key,
};
use crate::store::Row;

/// True when librqbit is transferring or about to (checking, not paused).
pub(crate) fn is_active(handle: &ManagedTorrent) -> bool {
    handle.with_state(|s| match s {
        ManagedTorrentState::Live(_) => true,
        ManagedTorrentState::Initializing(_) => !handle.is_paused(),
        _ => false,
    })
}

impl TorrentService {
    /// Runs the policy and pauses / unpauses torrents to match. The caller
    /// holds `run.op_lock`.
    pub(crate) async fn reconcile_locked(&self, run: &Arc<Running>) {
        let handles = self.handles(run);
        let facts = self.facts(run, &handles);
        let settings = run.settings.lock().expect("lock").clone();
        let plan = desired_states(&facts, &settings, &self.policy_ctx());
        run.state.lock().expect("lock").desired = plan.iter().cloned().collect();
        for (hash, want) in plan {
            let result = match handles.get(&hash) {
                Some(handle) => self.apply(run, &hash, handle, want).await,
                None => {
                    self.apply_pending(run, &hash, want);
                    Ok(())
                }
            };
            if let Err(e) = result {
                tracing::debug!(%hash, error = %e, "could not apply torrent state");
            }
        }
    }

    fn facts(
        &self,
        run: &Running,
        handles: &HashMap<String, Arc<ManagedTorrent>>,
    ) -> Vec<TorrentFacts> {
        let st = run.state.lock().expect("lock");
        st.rows
            .values()
            .map(|row| {
                let handle = handles.get(&row.info_hash);
                let stats = handle.map(|h| h.stats());
                let counter = st.counters.get(&row.info_hash).copied().unwrap_or_default();
                TorrentFacts {
                    id: row.info_hash.clone(),
                    run_mode: row.run_mode,
                    queue_pos: row.queue_pos,
                    has_metadata: handle.is_some(),
                    finished: stats.as_ref().is_some_and(|s| s.finished),
                    done: row.done,
                    uploaded_total: counter.up,
                    downloaded_total: counter.down,
                    size: stats.as_ref().map_or(0, |s| s.total_bytes),
                    seeding_since: row.seeding_since,
                    error: st.errors.contains_key(&row.info_hash)
                        || stats.as_ref().is_some_and(|s| s.error.is_some()),
                }
            })
            .collect()
    }

    /// Pauses or unpauses one librqbit torrent.
    async fn apply(
        &self,
        run: &Running,
        hash: &str,
        handle: &Arc<ManagedTorrent>,
        want: Desired,
    ) -> Result<()> {
        let paused = handle.with_state(|s| match s {
            ManagedTorrentState::Paused(_) => true,
            ManagedTorrentState::Initializing(_) => handle.is_paused(),
            _ => false,
        });
        match want {
            Desired::Run if paused => run
                .session
                .unpause(handle)
                .await
                .map_err(|e| invalid("could not resume", e)),
            Desired::Paused | Desired::Stopped if is_active(handle) => {
                self.pause_handle(run, hash, handle).await
            }
            _ => Ok(()),
        }
    }

    /// Pauses a torrent, keeping what it moved since the last tick.
    pub(crate) async fn pause_handle(
        &self,
        run: &Running,
        hash: &str,
        handle: &Arc<ManagedTorrent>,
    ) -> Result<()> {
        self.capture(run, hash, handle);
        let result = run.session.pause(handle).await;
        if let Some(c) = run.state.lock().expect("lock").counters.get_mut(hash) {
            c.reset_live();
        }
        result.map_err(|e| invalid("could not pause", e))
    }

    /// Adds what a live torrent moved since the last look to its totals.
    pub(crate) fn capture(&self, run: &Running, hash: &str, handle: &ManagedTorrent) {
        let live = live_counters(handle);
        if let Some(c) = run.state.lock().expect("lock").counters.get_mut(hash) {
            c.observe(live);
        }
    }

    /// Starts or cancels the metadata fetch of a magnet without a handle.
    fn apply_pending(&self, run: &Arc<Running>, hash: &str, want: Desired) {
        let mut st = run.state.lock().expect("lock");
        if want != Desired::Run {
            if let Some(token) = st.pending.remove(hash) {
                token.cancel();
            }
            return;
        }
        if st.pending.contains_key(hash) || st.errors.contains_key(hash) {
            return;
        }
        let Some(row) = st.rows.get(hash).cloned() else {
            return;
        };
        let Ok(magnet) = std::fs::read_to_string(self.inner.opts.magnet_file(hash)) else {
            return;
        };
        let token = run.cancel.child_token();
        st.pending.insert(hash.to_owned(), token.clone());
        let running = self.handles(run).values().filter(|h| is_active(h)).count();
        drop(st);
        let opts = self.add_options(run, &row, magnet_peers(&magnet), running + 1, false);
        let svc = self.clone();
        let run = run.clone();
        let hash = hash.to_owned();
        tokio::spawn(async move {
            let result = tokio::select! {
                () = token.cancelled() => return,
                r = run.session.add_torrent(AddTorrent::from_url(magnet), Some(opts)) => r,
            };
            svc.fetch_done(
                &run,
                &hash,
                &token,
                result.map(|_| ()).map_err(|e| format!("{e:#}")),
            )
            .await;
        });
    }

    async fn fetch_done(
        &self,
        run: &Arc<Running>,
        hash: &str,
        token: &CancellationToken,
        result: std::result::Result<(), String>,
    ) {
        let _op = run.op_lock.lock().await;
        if token.is_cancelled() {
            // Paused, removed or stopped meanwhile; a handle that made it
            // into the session is handled like any other.
            return;
        }
        let known = {
            let mut st = run.state.lock().expect("lock");
            st.pending.remove(hash);
            if let Err(message) = &result {
                st.errors.insert(hash.to_owned(), message.clone());
            } else {
                st.incomplete.insert(hash.to_owned());
            }
            st.rows.contains_key(hash)
        };
        if run.cancel.is_cancelled() {
            return;
        }
        if !known {
            // Removed while the metadata was on its way.
            if let Ok(key) = torrent_key(hash) {
                let _ = run.session.delete(key, false).await;
            }
            return;
        }
        if let Err(message) = result {
            tracing::warn!(%hash, %message, "could not fetch torrent metadata");
            return;
        }
        self.reconcile_locked(run).await;
    }

    /// librqbit add options for a row.
    pub(crate) fn add_options(
        &self,
        run: &Running,
        row: &Row,
        peers: Vec<SocketAddr>,
        running: usize,
        paused: bool,
    ) -> AddTorrentOptions {
        let t = run.settings.lock().expect("lock").clone();
        let own = row.peer_limit.unwrap_or(t.peer_limit_torrent);
        AddTorrentOptions {
            paused,
            overwrite: true,
            output_folder: Some(row.save_dir.to_string_lossy().into_owned()),
            ratelimits: limits_config(SpeedLimits {
                down_kib: row.down_kib,
                up_kib: row.up_kib,
            }),
            initial_peers: (!peers.is_empty()).then_some(peers),
            peer_limit: Some(peer_share(t.peer_limit_global, own, running) as usize),
            ..Default::default()
        }
    }

    /// Re-adds a torrent so new per-torrent limits take effect: librqbit
    /// reads them only at add. Keeps the fastresume bitfield, so nothing is
    /// re-checked. The caller holds `run.op_lock`; the next reconcile
    /// resumes it if it should run.
    pub(crate) async fn readd(&self, run: &Arc<Running>, hash: &str) -> Result<()> {
        let key = torrent_key(hash)?;
        let Some(handle) = run.session.get(key) else {
            // Not added yet: the fetch uses the new options.
            return Ok(());
        };
        let Some(bytes) = handle
            .metadata
            .load()
            .as_ref()
            .map(|m| m.torrent_bytes.clone())
        else {
            return Ok(());
        };
        let Some(row) = run.state.lock().expect("lock").rows.get(hash).cloned() else {
            return Ok(());
        };
        let only_files = handle.only_files();
        let output_folder = handle.output_folder().to_path_buf();
        self.capture(run, hash, &handle);
        let running = self.handles(run).values().filter(|h| is_active(h)).count();
        drop(handle);

        let bitv = self
            .inner
            .opts
            .persistence_dir()
            .join(format!("{hash}.bitv"));
        let kept = std::fs::read(&bitv).ok();
        run.session
            .delete(key, false)
            .await
            .map_err(|e| invalid("could not apply the limits", e))?;
        if let Some(kept) = kept
            && let Err(e) = std::fs::write(&bitv, kept)
        {
            tracing::debug!(error = %e, "fastresume state not kept; the torrent is re-checked");
        }
        if let Some(c) = run.state.lock().expect("lock").counters.get_mut(hash) {
            c.reset_live();
        }
        let peers = std::fs::read_to_string(self.inner.opts.magnet_file(hash))
            .map(|m| magnet_peers(&m))
            .unwrap_or_default();
        let mut opts = self.add_options(run, &row, peers, running.max(1), true);
        opts.output_folder = Some(output_folder.to_string_lossy().into_owned());
        opts.only_files = only_files;
        run.session
            .add_torrent(AddTorrent::from_bytes(bytes), Some(opts))
            .await
            .map_err(|e| invalid("could not apply the limits", e))?;
        Ok(())
    }
}
