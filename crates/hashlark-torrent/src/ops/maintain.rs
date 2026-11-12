// SPDX-License-Identifier: GPL-3.0-or-later

//! Tracker edits, reannounce and recheck (`docs/torrent-trackers-plan.md`
//! §3.3). librqbit has none of them as an API: each one re-adds the torrent
//! or pauses and resumes it.

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use hashlark_core::{Error, Result};
use librqbit::{ManagedTorrent, ManagedTorrentState};
use librqbit_bencode::raw_value::RawValue;
use serde::Serialize;
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

/// A .torrent with `info_bytes` as its info dictionary and `trackers` as
/// one tier, like librqbit's own `torrent_file_from_info_bytes`. The info
/// bytes are kept as they are, so the info hash doesn't change.
pub(crate) fn torrent_with_trackers(info_bytes: &[u8], trackers: &[Url]) -> Result<Bytes> {
    #[derive(Serialize)]
    struct Torrent<'a> {
        announce: &'a str,
        #[serde(rename = "announce-list")]
        announce_list: &'a [Vec<&'a str>],
        info: RawValue<&'a [u8]>,
    }
    let tier: Vec<&str> = trackers.iter().map(Url::as_str).collect();
    let torrent = Torrent {
        announce: tier.first().copied().unwrap_or(""),
        announce_list: std::slice::from_ref(&tier),
        info: RawValue(info_bytes),
    };
    let mut out = Vec::new();
    librqbit_bencode::bencode_serialize_to_writer(&torrent, &mut out)
        .map_err(|e| invalid("could not build the torrent", e))?;
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
    /// Checks a tracker edit for `update`, changing nothing.
    pub(crate) fn tracker_edit(
        handle: Option<&Arc<ManagedTorrent>>,
        raw: &[String],
    ) -> Result<TrackerEdit> {
        let trackers = tracker_list(raw)?;
        let info_bytes = handle
            .and_then(|h| h.metadata.load().as_ref().map(|m| m.info_bytes.clone()))
            .ok_or_else(|| Error::Invalid(NO_METADATA.into()))?;
        let torrent_bytes = torrent_with_trackers(&info_bytes, &trackers)?;
        Ok(TrackerEdit {
            trackers,
            torrent_bytes,
        })
    }

    /// Re-adds a torrent with a new tracker list (and the row's current
    /// options, so a limits change in the same patch needs no second
    /// re-add), then rewrites its magnet file. The caller holds
    /// `run.op_lock`.
    pub(crate) async fn set_trackers(
        &self,
        run: &Arc<Running>,
        id: &str,
        previous: &Row,
        edit: TrackerEdit,
    ) -> Result<()> {
        let opts = ReaddOpts {
            torrent_bytes: Some(edit.torrent_bytes),
            keep_fastresume: true,
        };
        self.readd_with(run, id, previous, opts).await?;
        let file = self.inner.opts.magnet_file(id);
        if let Ok(magnet) = std::fs::read_to_string(&file)
            && let Err(e) = std::fs::write(&file, magnet_with_trackers(&magnet, &edit.trackers))
        {
            tracing::warn!(%id, error = %e, "could not update the magnet file");
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
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn invalid_list(list: &[String]) -> bool {
        matches!(tracker_list(list), Err(Error::Invalid(_)))
    }

    #[test]
    fn tracker_list_rules() {
        let ok = tracker_list(&strings(&[
            "http://a.example/announce",
            " https://b.example/announce ",
            "udp://c.example:1337",
            "ws://d.example/",
            "wss://e.example/",
            "",
        ]))
        .unwrap();
        let ok: Vec<&str> = ok.iter().map(Url::as_str).collect();
        assert_eq!(
            ok,
            [
                "http://a.example/announce",
                "https://b.example/announce",
                "udp://c.example:1337",
                "ws://d.example/",
                "wss://e.example/",
            ]
        );
        assert!(tracker_list(&[]).unwrap().is_empty());

        let Err(Error::Invalid(message)) =
            tracker_list(&strings(&["http://a.example/", "ftp://x.example/"]))
        else {
            panic!("ftp is refused");
        };
        assert!(message.contains("ftp://x.example/"), "{message}");
        assert!(invalid_list(&strings(&["not a url"])));
        assert!(invalid_list(&strings(&["magnet:?xt=urn:btih:00"])));

        let many: Vec<String> = (0..=MAX_TRACKERS)
            .map(|i| format!("http://t{i}.example/"))
            .collect();
        assert!(invalid_list(&many));
        assert!(tracker_list(&many[..MAX_TRACKERS]).is_ok());

        let long = format!("http://a.example/{}", "x".repeat(MAX_URL_LEN));
        assert!(invalid_list(&[long]));
    }

    #[test]
    fn tracker_list_dedupes_in_order() {
        let list = tracker_list(&strings(&[
            "udp://b.example:1",
            "http://a.example/announce",
            "udp://b.example:1",
            " http://a.example/announce",
            "http://c.example/announce",
        ]))
        .unwrap();
        let list: Vec<&str> = list.iter().map(Url::as_str).collect();
        assert_eq!(
            list,
            [
                "udp://b.example:1",
                "http://a.example/announce",
                "http://c.example/announce"
            ]
        );
    }

    const INFO: &[u8] =
        b"d6:lengthi5e4:name5:a.bin12:piece lengthi16384e6:pieces20:01234567890123456789e";

    #[test]
    fn torrent_with_trackers_keeps_the_info_hash() {
        let mut original = b"d8:announce19:http://old.example/4:info".to_vec();
        original.extend_from_slice(INFO);
        original.push(b'e');
        let original = librqbit::torrent_from_bytes(&original).unwrap();

        let trackers = tracker_list(&strings(&[
            "udp://b.example:1",
            "http://a.example/announce",
        ]))
        .unwrap();
        let bytes = torrent_with_trackers(INFO, &trackers).unwrap();
        let parsed = librqbit::torrent_from_bytes(&bytes).unwrap();
        assert_eq!(parsed.info_hash, original.info_hash);
        let announce: Vec<&[u8]> = parsed.iter_announce().map(|b| b.as_ref()).collect();
        assert_eq!(
            announce,
            [&b"udp://b.example:1"[..], &b"http://a.example/announce"[..]]
        );

        let set: HashSet<Url> = trackers.iter().cloned().collect();
        assert_eq!(
            ordered_trackers(&set, &bytes),
            ["udp://b.example:1", "http://a.example/announce"]
        );
        assert_eq!(
            ordered_trackers(&set, b"garbage"),
            ["http://a.example/announce", "udp://b.example:1"]
        );

        let empty = torrent_with_trackers(INFO, &[]).unwrap();
        let parsed = librqbit::torrent_from_bytes(&empty).unwrap();
        assert_eq!(parsed.info_hash, original.info_hash);
        assert!(ordered_trackers(&HashSet::new(), &empty).is_empty());
    }

    #[test]
    fn magnet_with_trackers_replaces_tr_only() {
        let magnet = "magnet:?xt=urn:btih:abcd&dn=My%20File&tr=http%3A%2F%2Fold.example%2F\
                      &x.pe=127.0.0.1:6881&tr=udp%3A%2F%2Fold2.example%3A1&x.pe=[::1]:6881&so=0";
        let trackers = tracker_list(&strings(&[
            "udp://new.example:80",
            "http://n2.example/a?b=c",
        ]))
        .unwrap();
        let out = magnet_with_trackers(magnet, &trackers);
        assert_eq!(
            out,
            "magnet:?xt=urn:btih:abcd&dn=My%20File&x.pe=127.0.0.1:6881&x.pe=[::1]:6881&so=0\
             &tr=udp%3A%2F%2Fnew.example%3A80&tr=http%3A%2F%2Fn2.example%2Fa%3Fb%3Dc"
        );
        let parsed = Url::parse(&out).unwrap();
        let tr: Vec<String> = parsed
            .query_pairs()
            .filter(|(k, _)| k == "tr")
            .map(|(_, v)| v.into_owned())
            .collect();
        assert_eq!(tr, ["udp://new.example:80", "http://n2.example/a?b=c"]);
        assert_eq!(crate::service::magnet_peers(&out).len(), 2);

        let none = magnet_with_trackers(magnet, &[]);
        assert!(!none.contains("tr="), "{none}");
        assert!(none.contains("dn=My%20File"));
    }
}
