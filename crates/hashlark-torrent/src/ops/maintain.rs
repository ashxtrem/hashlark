// SPDX-License-Identifier: GPL-3.0-or-later

//! Tracker edits, reannounce and recheck (`docs/torrent-trackers-plan.md`
//! §3.3). librqbit has none of them as an API: each one re-adds the torrent
//! or pauses and resumes it.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use bytes::Bytes;
use hashlark_core::{Error, Result};
use librqbit::{ManagedTorrent, ManagedTorrentState};
use librqbit_bencode::WithRawBytes;
use serde::Serialize;
use serde::de::IgnoredAny;
use url::Url;

use crate::service::{ReaddOpts, Running, TorrentService, invalid, torrent_key};
use crate::store::Row;

const SCHEMES: [&str; 5] = ["http", "https", "udp", "ws", "wss"];
const MAX_TRACKERS: usize = 100;
const MAX_URL_LEN: usize = 2048;
const NO_METADATA: &str = "the torrent's metadata isn't known yet";

/// A checked tracker edit, ready to apply.
#[derive(Debug)]
pub(crate) struct TrackerEdit {
    trackers: Vec<Url>,
    torrent_bytes: Bytes,
}

/// The tracker list a user sent: trimmed, blanks and duplicates dropped
/// (order kept). A bad URL fails the whole list.
pub(crate) fn tracker_list(raw: &[String]) -> Result<Vec<Url>> {
    let mut out: Vec<Url> = Vec::new();
    for item in raw {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if item.len() > MAX_URL_LEN {
            let start: String = item.chars().take(60).collect();
            return Err(Error::Invalid(format!(
                "the tracker URL `{start}…` is longer than {MAX_URL_LEN} bytes"
            )));
        }
        let url = Url::parse(item)
            .ok()
            .filter(|u| u.has_host())
            .ok_or_else(|| Error::Invalid(format!("`{item}` is not a valid tracker URL")))?;
        if !SCHEMES.contains(&url.scheme()) {
            return Err(Error::Invalid(format!(
                "the tracker URL `{item}` must start with http, https, udp, ws or wss"
            )));
        }
        if !out.contains(&url) {
            out.push(url);
        }
    }
    if out.len() > MAX_TRACKERS {
        return Err(Error::Invalid(format!(
            "a torrent can have at most {MAX_TRACKERS} trackers"
        )));
    }
    Ok(out)
}

/// The bencoded form of `value`.
fn bencode(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    librqbit_bencode::bencode_serialize_to_writer(value, &mut out)
        .map_err(|e| invalid("could not build the torrent", e))?;
    Ok(out)
}

/// `old` (a .torrent) with `info_bytes` as its info dictionary and
/// `trackers` as one tier. Its other top-level keys (comment, created by,
/// creation date, url-list, ...) are kept as they were; when `old` can't be
/// read, the torrent has just the three. The info bytes are kept as they
/// are, so the info hash doesn't change.
pub(crate) fn torrent_with_trackers(
    old: &[u8],
    info_bytes: &[u8],
    trackers: &[Url],
) -> Result<Bytes> {
    type Raw<'a> = WithRawBytes<IgnoredAny, &'a [u8]>;
    let mut keys: BTreeMap<&[u8], Cow<'_, [u8]>> =
        librqbit_bencode::from_bytes::<HashMap<&[u8], Raw<'_>>>(old)
            .map(|dict| {
                dict.into_iter()
                    .map(|(k, v)| (k, Cow::Borrowed(v.raw_bytes)))
                    .collect()
            })
            .unwrap_or_default();
    let tier: Vec<&str> = trackers.iter().map(Url::as_str).collect();
    let announce = bencode(&tier.first().copied().unwrap_or(""))?;
    keys.insert(b"announce", Cow::Owned(announce));
    let list = bencode(&std::slice::from_ref(&tier))?;
    keys.insert(b"announce-list", Cow::Owned(list));
    keys.insert(b"info", Cow::Borrowed(info_bytes));
    // A dictionary: keys as byte strings, sorted (the map's order).
    let mut out = vec![b'd'];
    for (key, value) in &keys {
        out.extend_from_slice(format!("{}:", key.len()).as_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(value);
    }
    out.push(b'e');
    Ok(out.into())
}

/// `magnet` with its `tr` params replaced by `trackers`; every other param
/// (`xt`, `dn`, `x.pe`, `so`, ...) is kept as it was.
pub(crate) fn magnet_with_trackers(magnet: &str, trackers: &[Url]) -> String {
    let (base, query) = magnet.split_once('?').unwrap_or((magnet, ""));
    let mut params: Vec<String> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .filter(|p| {
            let key = p.split_once('=').map_or(*p, |(k, _)| k);
            key != "tr"
                && !key
                    .strip_prefix("tr.")
                    .is_some_and(|n| n.parse::<u32>().is_ok())
        })
        .map(str::to_owned)
        .collect();
    params.extend(trackers.iter().map(|t| {
        let value: String = url::form_urlencoded::byte_serialize(t.as_str().as_bytes()).collect();
        format!("tr={value}")
    }));
    format!("{base}?{}", params.join("&"))
}

/// The trackers librqbit holds, in the order of the torrent's announce
/// list (the user's order); sorted when that can't be read.
pub(crate) fn ordered_trackers(set: &HashSet<Url>, torrent_bytes: &[u8]) -> Vec<String> {
    let mut sorted: Vec<String> = set.iter().map(ToString::to_string).collect();
    sorted.sort();
    let Ok(torrent) = librqbit::torrent_from_bytes(torrent_bytes) else {
        return sorted;
    };
    let mut out: Vec<String> = Vec::with_capacity(set.len());
    for announce in torrent.iter_announce() {
        if let Ok(url) = std::str::from_utf8(announce.as_ref()).map(Url::parse)
            && let Ok(url) = url
            && set.contains(&url)
            && !out.contains(&url.to_string())
        {
            out.push(url.to_string());
        }
    }
    // Any librqbit holds that the list doesn't name (none today).
    for t in sorted {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// The live librqbit handle of a torrent that has its metadata.
fn with_metadata(run: &Running, id: &str) -> Result<Arc<ManagedTorrent>> {
    run.session
        .get(torrent_key(id)?)
        .filter(|h| h.metadata.load().is_some())
        .ok_or_else(|| Error::Invalid(NO_METADATA.into()))
}

fn require_row(run: &Running, id: &str) -> Result<Row> {
    run.state
        .lock()
        .expect("lock")
        .rows
        .get(id)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("torrent `{id}`")))
}

impl TorrentService {
    /// Checks a tracker edit for `update`, changing nothing. `None` when the
    /// list is the torrent's current one (in the order `details` shows):
    /// there is nothing to re-add.
    pub(crate) fn tracker_edit(
        handle: Option<&Arc<ManagedTorrent>>,
        raw: &[String],
    ) -> Result<Option<TrackerEdit>> {
        let trackers = tracker_list(raw)?;
        let handle = handle.ok_or_else(|| Error::Invalid(NO_METADATA.into()))?;
        let (info_bytes, old_bytes) = handle
            .metadata
            .load()
            .as_ref()
            .map(|m| (m.info_bytes.clone(), m.torrent_bytes.clone()))
            .ok_or_else(|| Error::Invalid(NO_METADATA.into()))?;
        let current = ordered_trackers(&handle.shared().trackers, &old_bytes);
        if current
            .iter()
            .map(String::as_str)
            .eq(trackers.iter().map(Url::as_str))
        {
            return Ok(None);
        }
        let torrent_bytes = torrent_with_trackers(&old_bytes, &info_bytes, &trackers)?;
        Ok(Some(TrackerEdit {
            trackers,
            torrent_bytes,
        }))
    }

    /// Re-adds a torrent with a new tracker list (and the row's current
    /// options, so a limits change in the same patch needs no second
    /// re-add), then rewrites its magnet file and the .torrent it was
    /// created as, if it has them. The caller holds `run.op_lock`.
    pub(crate) async fn set_trackers(
        &self,
        run: &Arc<Running>,
        id: &str,
        previous: &Row,
        edit: TrackerEdit,
    ) -> Result<()> {
        let opts = ReaddOpts {
            torrent_bytes: Some(edit.torrent_bytes.clone()),
            keep_fastresume: true,
        };
        self.readd_with(run, id, previous, opts).await?;
        let file = self.inner.opts.magnet_file(id);
        if let Ok(magnet) = std::fs::read_to_string(&file)
            && let Err(e) = std::fs::write(&file, magnet_with_trackers(&magnet, &edit.trackers))
        {
            tracing::warn!(%id, error = %e, "could not update the magnet file");
        }
        // `torrent_file` exports this one first.
        let created = self.inner.opts.created_file(id);
        if created.is_file()
            && let Err(e) = std::fs::write(&created, &edit.torrent_bytes)
        {
            tracing::warn!(%id, error = %e, "could not update the created torrent");
        }
        Ok(())
    }

    /// Asks trackers and peers again: librqbit announces on every resume,
    /// so the torrent is paused and resumed. Only for a live torrent.
    pub(crate) async fn reannounce(&self, id: &str) -> Result<()> {
        let id = id.to_ascii_lowercase();
        let run = self.require()?;
        let _op = run.op_lock.lock().await;
        require_row(&run, &id)?;
        let handle = with_metadata(&run, &id)?;
        if !handle.with_state(|s| matches!(s, ManagedTorrentState::Live(_))) {
            return Err(Error::Invalid("the torrent isn't running".into()));
        }
        self.pause_handle(&run, &id, &handle).await?;
        run.session
            .unpause(&handle)
            .await
            .map_err(|e| invalid("could not resume the torrent", e))
    }

    /// Checks every piece again: re-adds the torrent without its fastresume
    /// state. It keeps its run mode, so a paused one stays paused.
    pub(crate) async fn recheck(&self, id: &str) -> Result<()> {
        let id = id.to_ascii_lowercase();
        let run = self.require()?;
        let _op = run.op_lock.lock().await;
        let row = require_row(&run, &id)?;
        with_metadata(&run, &id)?;
        let opts = ReaddOpts {
            torrent_bytes: None,
            keep_fastresume: false,
        };
        self.readd_with(&run, &id, &row, opts).await?;
        self.reconcile_locked(&run).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
