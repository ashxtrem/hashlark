// SPDX-License-Identifier: GPL-3.0-or-later

//! Re-adding a torrent to librqbit: the only way to change what it reads at
//! add (limits, trackers) or to make it check the data again.

use std::sync::Arc;

use bytes::Bytes;
use hashlark_core::Result;
use librqbit::AddTorrent;

use super::{Running, TorrentService, invalid, is_active, magnet_peers, torrent_key};
use crate::store::Row;

/// What a re-add changes besides the row's options.
#[derive(Debug, Clone)]
pub(crate) struct ReaddOpts {
    /// Torrent bytes to add instead of the current ones (a new tracker
    /// list); the fallback re-add uses the current ones.
    pub torrent_bytes: Option<Bytes>,
    /// Keep the fastresume bitfield; without it librqbit checks every piece.
    pub keep_fastresume: bool,
}

impl TorrentService {
    /// Re-adds a torrent so new per-torrent limits take effect: librqbit
    /// reads them only at add. Keeps the fastresume bitfield, so nothing is
    /// re-checked. The caller holds `run.op_lock`; the next reconcile
    /// resumes it if it should run.
    ///
    /// When the add fails, the torrent is added back with `previous`'s
    /// options; when that fails too, it is marked with an error so it
    /// doesn't silently vanish.
    pub(crate) async fn readd(&self, run: &Arc<Running>, hash: &str, previous: &Row) -> Result<()> {
        let opts = ReaddOpts {
            torrent_bytes: None,
            keep_fastresume: true,
        };
        self.readd_with(run, hash, previous, opts).await
    }

    /// [`readd`](Self::readd) with new torrent bytes and / or without the
    /// fastresume bitfield (a recheck).
    pub(crate) async fn readd_with(
        &self,
        run: &Arc<Running>,
        hash: &str,
        previous: &Row,
        with: ReaddOpts,
    ) -> Result<()> {
        let key = torrent_key(hash)?;
        let Some(handle) = run.session.get(key) else {
            // Not added yet: the fetch uses the new options.
            return Ok(());
        };
        let Some(old_bytes) = handle
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
        let kept = if with.keep_fastresume {
            std::fs::read(&bitv).ok()
        } else {
            None
        };
        run.session
            .delete(key, false)
            .await
            .map_err(|e| invalid("could not re-add the torrent", e))?;
        match kept {
            Some(kept) => {
                if let Err(e) = std::fs::write(&bitv, kept) {
                    tracing::debug!(error = %e, "fastresume state not kept; the torrent is re-checked");
                }
            }
            None if !with.keep_fastresume => {
                if let Err(e) = std::fs::remove_file(&bitv)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(%hash, error = %e, "could not drop the fastresume state");
                }
            }
            None => {}
        }
        if let Some(c) = run.state.lock().expect("lock").counters.get_mut(hash) {
            c.reset_live();
        }
        let peers = std::fs::read_to_string(self.inner.opts.magnet_file(hash))
            .map(|m| magnet_peers(&m))
            .unwrap_or_default();
        // `trackers` stays unset: it would extend the torrent's own list.
        let options = |row: &Row| {
            let mut opts = self.add_options(run, row, peers.clone(), running.max(1), true);
            opts.output_folder = Some(output_folder.to_string_lossy().into_owned());
            opts.only_files.clone_from(&only_files);
            opts
        };
        let bytes = with.torrent_bytes.unwrap_or_else(|| old_bytes.clone());
        let err = match run
            .session
            .add_torrent(AddTorrent::from_bytes(bytes), Some(options(&row)))
            .await
        {
            Ok(_) => return Ok(()),
            Err(e) => invalid("could not re-add the torrent", e),
        };
        tracing::warn!(%hash, error = %err, "re-adding with the previous options");
        if let Err(e) = run
            .session
            .add_torrent(AddTorrent::from_bytes(old_bytes), Some(options(previous)))
            .await
        {
            let message = format!("could not add the torrent back: {e:#}");
            tracing::warn!(%hash, %message, "torrent lost from the session");
            run.state
                .lock()
                .expect("lock")
                .errors
                .insert(hash.to_owned(), message);
        }
        Err(err)
    }
}
