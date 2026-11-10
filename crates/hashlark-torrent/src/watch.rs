// SPDX-License-Identifier: GPL-3.0-or-later

//! The watched folder (plan §3.4): every few seconds, each `*.torrent` in
//! `torrents.watch_dir` is added with the default options. A file that was
//! added is renamed to `*.torrent.added`, one that could not be to
//! `*.torrent.failed`. Desktop and server only: the poll does nothing while
//! `watch_dir` is unset, and Android never sets it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use hashlark_core::engine::MAX_TORRENT_BYTES;

use crate::model::{AddRequest, AddSource, Origin};
use crate::service::{Running, TorrentService};

/// Files changed more recently than this may still be being written.
const SETTLE: Duration = Duration::from_secs(1);

/// Size and modification time of a watched file at one poll.
type Seen = (u64, Option<SystemTime>);

/// What the poller remembers between polls.
#[derive(Default)]
struct WatchState {
    /// Every `*.torrent` seen at the last poll.
    seen: HashMap<PathBuf, Seen>,
    /// Files handled whose rename failed, as they were then: skipped while
    /// unchanged instead of being added (and warned about) every poll.
    stuck: HashSet<(PathBuf, Seen)>,
}

/// True when a file is safe to read: unchanged since the last poll (a copy
/// that keeps an old mtime still grows) and not modified for `SETTLE`.
fn is_ready(previous: Option<&Seen>, now: Seen, at: SystemTime) -> bool {
    let settled = now
        .1
        .and_then(|m| at.duration_since(m).ok())
        .is_none_or(|age| age >= SETTLE);
    settled && previous == Some(&now)
}

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
            let mut state = WatchState::default();
            loop {
                tokio::select! {
                    () = run.cancel.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let dir = run.settings.lock().expect("lock").watch_dir.clone();
                let Some(dir) = dir else {
                    warned = None;
                    state = WatchState::default();
                    continue;
                };
                match svc.path_policy().check(&dir) {
                    Ok(dir) => {
                        warned = None;
                        svc.poll_watch(&run, &dir, &mut state).await;
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
    async fn poll_watch(&self, run: &Arc<Running>, dir: &Path, state: &mut WatchState) {
        let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
            return;
        };
        let now = SystemTime::now();
        let mut seen = HashMap::new();
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
            let current: Seen = (meta.len(), meta.modified().ok());
            if is_ready(state.seen.get(&path), current, now)
                && !state.stuck.contains(&(path.clone(), current))
            {
                files.push((path.clone(), current));
            }
            seen.insert(path, current);
        }
        // Forget files that are gone (or changed, for the stuck ones).
        state.stuck.retain(|(path, s)| seen.get(path) == Some(s));
        state.seen = seen;
        files.sort();
        for (path, current) in files {
            if run.cancel.is_cancelled() {
                return;
            }
            let outcome = self.add_watched(&path, current.0).await;
            let suffix = match &outcome {
                Ok(()) => "added",
                Err(message) => {
                    tracing::warn!(file = %path.display(), %message, "could not add watched torrent");
                    "failed"
                }
            };
            if let Err(e) = tokio::fs::rename(&path, renamed(&path, suffix)).await {
                tracing::warn!(
                    file = %path.display(),
                    error = %e,
                    "could not rename watched torrent; it is skipped until it changes"
                );
                state.stuck.insert((path, current));
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
    fn ready_once_unchanged_between_polls() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let old = Some(t0 - Duration::from_secs(3600));
        // First sight: not yet, even with an old mtime (a copy keeps it).
        assert!(!is_ready(None, (10, old), t0));
        // Still growing with the same old mtime.
        assert!(!is_ready(Some(&(10, old)), (20, old), t0));
        // Same size and mtime on two polls.
        assert!(is_ready(Some(&(20, old)), (20, old), t0));
        // Unchanged but modified just now.
        let fresh = Some(t0 - Duration::from_millis(100));
        assert!(!is_ready(Some(&(20, fresh)), (20, fresh), t0));
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
