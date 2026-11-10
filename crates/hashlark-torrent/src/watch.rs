// SPDX-License-Identifier: GPL-3.0-or-later

//! The watched folder (plan §3.4): every few seconds, each `*.torrent` in
//! `torrents.watch_dir` is added with the default options. A file that was
//! added is renamed to `*.torrent.added`, one that could not be to
//! `*.torrent.failed`. Desktop and server only: the poll does nothing while
//! `watch_dir` is unset, and Android never sets it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use hashlark_core::engine::MAX_TORRENT_BYTES;

use crate::model::{AddRequest, AddSource, Origin};
use crate::service::{Running, TorrentService};

/// Files changed more recently than this may still be being written.
const SETTLE: Duration = Duration::from_secs(1);

/// What to do with the file at `path`: the new name after a successful or a
/// failed add (`foo.torrent` becomes `foo.torrent.added` or `.failed`).
fn renamed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

/// True for `*.torrent`, whatever the case of the extension.
fn is_torrent(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("torrent"))
}

impl TorrentService {
    /// Starts the poller of the watched folder; it ends with the session.
    pub(crate) fn spawn_watch(&self, run: &Arc<Running>) {
        let svc = self.clone();
        let run = run.clone();
        let every = self.inner.opts.watch_interval;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(every);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut warned: Option<PathBuf> = None;
            loop {
                tokio::select! {
                    () = run.cancel.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let dir = run.settings.lock().expect("lock").watch_dir.clone();
                let Some(dir) = dir else {
                    warned = None;
                    continue;
                };
                match svc.path_policy().check(&dir) {
                    Ok(dir) => {
                        warned = None;
                        svc.poll_watch(&run, &dir).await;
                    }
                    Err(e) => {
                        if warned.as_ref() != Some(&dir) {
                            tracing::warn!(dir = %dir.display(), error = %e, "watched folder not allowed");
                            warned = Some(dir);
                        }
                    }
                }
            }
        });
    }

    /// Adds every `*.torrent` in `dir` once.
    async fn poll_watch(&self, run: &Arc<Running>, dir: &Path) {
        let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
            return;
        };
        let mut files = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !is_torrent(&path) {
                continue;
            }
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let settled = meta
                .modified()
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .is_none_or(|age| age >= SETTLE);
            if settled {
                files.push((path, meta.len()));
            }
        }
        files.sort();
        for (path, len) in files {
            if run.cancel.is_cancelled() {
                return;
            }
            let outcome = self.add_watched(&path, len).await;
            let suffix = match &outcome {
                Ok(()) => "added",
                Err(message) => {
                    tracing::warn!(file = %path.display(), %message, "could not add watched torrent");
                    "failed"
                }
            };
            if let Err(e) = tokio::fs::rename(&path, renamed(&path, suffix)).await {
                tracing::warn!(file = %path.display(), error = %e, "could not rename watched torrent");
            }
        }
    }

    async fn add_watched(&self, path: &Path, len: u64) -> Result<(), String> {
        if len > MAX_TORRENT_BYTES {
            return Err("the .torrent file is too large".into());
        }
        let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
        let req = AddRequest::new(AddSource::Torrent(data_encoding::BASE64.encode(&bytes)));
        self.add_from(req, Origin::Watch)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_targets() {
        assert_eq!(
            renamed(Path::new("/w/a.torrent"), "added"),
            PathBuf::from("/w/a.torrent.added")
        );
        assert_eq!(
            renamed(Path::new("/w/a b.TORRENT"), "failed"),
            PathBuf::from("/w/a b.TORRENT.failed")
        );
    }

    #[test]
    fn only_torrent_files() {
        assert!(is_torrent(Path::new("/w/a.torrent")));
        assert!(is_torrent(Path::new("/w/a.TORRENT")));
        assert!(!is_torrent(Path::new("/w/a.torrent.added")));
        assert!(!is_torrent(Path::new("/w/a.txt")));
        assert!(!is_torrent(Path::new("/w/torrent")));
    }
}
