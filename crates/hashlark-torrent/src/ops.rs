// SPDX-License-Identifier: GPL-3.0-or-later

//! Operations on torrents (plan §3.2), as [`TorrentService`] methods.
//! Errors are core `Invalid` / `NotFound`, so the server maps them as usual.

mod add;
mod info;

use std::collections::HashSet;

use hashlark_core::{Error, Result};
use librqbit::ManagedTorrentState;

use crate::model::{
    BulkAction, BulkFailure, BulkRequest, BulkResult, QueueMove, QueueStep, TorrentDetails,
    TorrentPatch,
};
use crate::service::{Running, TorrentService, invalid, torrent_key};
use crate::store;

/// Where `mv` puts the item at `from` in a list of `len`.
fn queue_target(from: usize, len: usize, mv: QueueMove) -> usize {
    let last = len.saturating_sub(1);
    match mv {
        QueueMove::Step(QueueStep::Top) => 0,
        QueueMove::Step(QueueStep::Up) => from.saturating_sub(1),
        QueueMove::Step(QueueStep::Down) => (from + 1).min(last),
        QueueMove::Step(QueueStep::Bottom) => last,
        QueueMove::Index(i) => usize::try_from(i).unwrap_or(usize::MAX).min(last),
    }
}

impl TorrentService {
    /// Changes a torrent (plan §3.2): run mode, files, limits, connection
    /// limit, seeding limits, queue position.
    ///
    /// New speed or connection limits re-add the torrent at once (librqbit
    /// reads them only at add); its fastresume state is kept.
    pub async fn update(&self, id: &str, patch: TorrentPatch) -> Result<TorrentDetails> {
        let id = id.to_ascii_lowercase();
        let run = self.require()?;
        let key = torrent_key(&id)?;
        let op = run.op_lock.lock().await;
        let mut row = run
            .state
            .lock()
            .expect("lock")
            .rows
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("torrent `{id}`")))?;
        let handle = run.session.get(key);

        // Check everything before changing anything.
        let files = match &patch.files {
            Some(files) => {
                let handle = handle.as_ref().ok_or_else(|| {
                    Error::Invalid("the torrent's file list isn't known yet".into())
                })?;
                let count = handle
                    .with_metadata(|m| m.file_infos.len())
                    .map_err(|e| invalid("the torrent's file list isn't known yet", e))?;
                if files.is_empty() {
                    return Err(Error::Invalid("select at least one file".into()));
                }
                if let Some(bad) = files.iter().find(|&&i| i >= count) {
                    return Err(Error::Invalid(format!("the torrent has no file {bad}")));
                }
                Some(files.iter().copied().collect::<HashSet<usize>>())
            }
            None => None,
        };
        let peer_limit = match patch.peer_limit {
            Some(0) => Some(None),
            Some(n) if !(10..=2000).contains(&n) => {
                return Err(Error::Invalid(
                    "peer_limit must be between 10 and 2000, or 0 for the default".into(),
                ));
            }
            other => other.map(Some),
        };
        if let Some(ratio) = patch.seed.and_then(|s| s.ratio)
            && !(0.0..=100.0).contains(&ratio)
        {
            return Err(Error::Invalid(
                "seed ratio must be between 0 and 100".into(),
            ));
        }

        if let (Some(files), Some(handle)) = (&files, &handle) {
            run.session
                .update_only_files(handle, files)
                .await
                .map_err(|e| invalid("could not change the selected files", e))?;
        }
        let mut readd = false;
        if let Some(limits) = patch.limits {
            readd |= row.down_kib != limits.down_kib || row.up_kib != limits.up_kib;
            row.down_kib = limits.down_kib;
            row.up_kib = limits.up_kib;
        }
        if let Some(limit) = peer_limit {
            readd |= row.peer_limit != limit;
            row.peer_limit = limit;
        }
        if let Some(seed) = patch.seed {
            row.seed = seed;
        }
        let mut retry = false;
        if let Some(action) = patch.action {
            row.run_mode = action.run_mode();
            retry = matches!(
                row.run_mode,
                crate::model::RunMode::Auto | crate::model::RunMode::Forced
            );
        }
        store::update(self.engine().store(), &row).await?;
        {
            let mut st = run.state.lock().expect("lock");
            st.rows.insert(id.clone(), row);
            if retry {
                // Resume / Start / Force start retry a failed fetch.
                st.errors.remove(&id);
            }
        }
        if let Some(mv) = patch.queue_pos {
            self.move_in_queue(&run, &id, mv).await?;
        }
        if readd {
            self.readd(&run, &id).await?;
        }
        if retry
            && let Some(handle) = run.session.get(key)
            && handle.with_state(|s| matches!(s, ManagedTorrentState::Error(_)))
        {
            run.session
                .unpause(&handle)
                .await
                .map_err(|e| invalid("could not restart the torrent", e))?;
        }
        self.reconcile_locked(&run).await;
        drop(op);
        self.details(&id).await
    }

    async fn move_in_queue(&self, run: &Running, id: &str, mv: QueueMove) -> Result<()> {
        let order = {
            let st = run.state.lock().expect("lock");
            let mut rows: Vec<_> = st.rows.values().collect();
            rows.sort_by_key(|r| (r.queue_pos, r.added_at));
            let mut order: Vec<String> = rows.iter().map(|r| r.info_hash.clone()).collect();
            let from = order.iter().position(|h| h == id).unwrap_or(0);
            let to = queue_target(from, order.len(), mv);
            let item = order.remove(from);
            order.insert(to, item);
            order
        };
        store::set_queue(self.engine().store(), &order).await?;
        let mut st = run.state.lock().expect("lock");
        for (pos, hash) in order.iter().enumerate() {
            if let Some(row) = st.rows.get_mut(hash) {
                row.queue_pos = i64::try_from(pos).unwrap_or(i64::MAX);
            }
        }
        Ok(())
    }

    /// Removes a torrent and its stored state; with `delete_files`, also
    /// its downloaded files.
    pub async fn remove(&self, id: &str, delete_files: bool) -> Result<()> {
        let id = id.to_ascii_lowercase();
        let run = self.require()?;
        let key = torrent_key(&id)?;
        let _op = run.op_lock.lock().await;
        {
            let mut st = run.state.lock().expect("lock");
            if !st.rows.contains_key(&id) {
                return Err(Error::NotFound(format!("torrent `{id}`")));
            }
            if let Some(token) = st.pending.remove(&id) {
                token.cancel();
            }
        }
        if run.session.get(key).is_some() {
            run.session
                .delete(key, delete_files)
                .await
                .map_err(|e| invalid("could not remove the torrent", e))?;
        }
        store::delete(self.engine().store(), &id).await?;
        for file in [
            self.inner.opts.magnet_file(&id),
            self.inner.opts.created_file(&id),
        ] {
            let _ = std::fs::remove_file(file);
        }
        let mut st = run.state.lock().expect("lock");
        st.rows.remove(&id);
        st.counters.remove(&id);
        st.errors.remove(&id);
        st.incomplete.remove(&id);
        st.desired.remove(&id);
        Ok(())
    }

    /// Applies one action to many torrents (all when `ids` is absent).
    pub async fn bulk(&self, req: BulkRequest) -> Result<BulkResult> {
        let run = self.require()?;
        if let BulkAction::Turtle(on) = req.action {
            let mut settings = self.engine().settings();
            settings.torrents.turtle = on;
            let saved = self.engine().set_settings(settings).await?;
            self.apply_settings(&saved).await?;
            return Ok(BulkResult::default());
        }
        let ids = match req.ids {
            Some(ids) => ids,
            None => {
                let st = run.state.lock().expect("lock");
                let mut rows: Vec<_> = st.rows.values().collect();
                rows.sort_by_key(|r| (r.queue_pos, r.added_at));
                rows.iter().map(|r| r.info_hash.clone()).collect()
            }
        };
        drop(run);
        let mut result = BulkResult::default();
        for id in ids {
            let done = match &req.action {
                BulkAction::Remove { delete_files } => self.remove(&id, *delete_files).await,
                action => {
                    let action = match action {
                        BulkAction::Pause => crate::model::TorrentAction::Pause,
                        BulkAction::Resume => crate::model::TorrentAction::Resume,
                        BulkAction::Stop => crate::model::TorrentAction::Stop,
                        BulkAction::Start => crate::model::TorrentAction::Start,
                        _ => crate::model::TorrentAction::ForceStart,
                    };
                    let patch = TorrentPatch {
                        action: Some(action),
                        ..TorrentPatch::default()
                    };
                    self.update(&id, patch).await.map(|_| ())
                }
            };
            match done {
                Ok(()) => result.ok.push(id.to_ascii_lowercase()),
                Err(e) => result.failed.push(BulkFailure {
                    id,
                    message: e.to_string(),
                }),
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_moves() {
        let step = |s| QueueMove::Step(s);
        assert_eq!(queue_target(2, 5, step(QueueStep::Top)), 0);
        assert_eq!(queue_target(2, 5, step(QueueStep::Up)), 1);
        assert_eq!(queue_target(0, 5, step(QueueStep::Up)), 0);
        assert_eq!(queue_target(2, 5, step(QueueStep::Down)), 3);
        assert_eq!(queue_target(4, 5, step(QueueStep::Down)), 4);
        assert_eq!(queue_target(2, 5, step(QueueStep::Bottom)), 4);
        assert_eq!(queue_target(2, 5, QueueMove::Index(1)), 1);
        assert_eq!(queue_target(2, 5, QueueMove::Index(99)), 4);
    }

    #[test]
    fn queue_move_json() {
        let top: QueueMove = serde_json::from_str("\"top\"").unwrap();
        assert_eq!(top, QueueMove::Step(QueueStep::Top));
        let idx: QueueMove = serde_json::from_str("3").unwrap();
        assert_eq!(idx, QueueMove::Index(3));
        let bulk: BulkRequest =
            serde_json::from_str(r#"{"action":{"remove":{"delete_files":true}}}"#).unwrap();
        assert_eq!(bulk.action, BulkAction::Remove { delete_files: true });
        let bulk: BulkRequest = serde_json::from_str(r#"{"ids":["a"],"action":"pause"}"#).unwrap();
        assert_eq!(bulk.action, BulkAction::Pause);
    }
}
