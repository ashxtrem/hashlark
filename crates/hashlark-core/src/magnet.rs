// SPDX-License-Identifier: GPL-3.0-or-later

//! Building and completing magnet links.

use url::Url;
use url::form_urlencoded;

use crate::model::InfoHash;

/// Public trackers added to magnets that have none (or too few). Users will be
/// able to replace or refresh this list in settings (M5).
pub const DEFAULT_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
];

/// Builds a magnet link from an infohash, display name and trackers.
pub fn build_magnet<S: AsRef<str>>(hash: &InfoHash, name: &str, trackers: &[S]) -> String {
    let mut params = form_urlencoded::Serializer::new(String::new());
    if !name.is_empty() {
        params.append_pair("dn", name);
    }
    for tracker in trackers {
        params.append_pair("tr", tracker.as_ref());
    }
    let params = params.finish();
    // `xt` is written by hand: its `urn:btih:` colons must stay unescaped.
    if params.is_empty() {
        format!("magnet:?xt=urn:btih:{hash}")
    } else {
        format!("magnet:?xt=urn:btih:{hash}&{params}")
    }
}

/// Extracts the v1 infohash from a magnet link, if it has one.
pub fn infohash_from_magnet(magnet: &str) -> Option<InfoHash> {
    let url = Url::parse(magnet).ok()?;
    if url.scheme() != "magnet" {
        return None;
    }
    url.query_pairs()
        .filter(|(key, _)| key == "xt")
        .find_map(|(_, value)| {
            value
                .strip_prefix("urn:btih:")
                .and_then(|hash| hash.parse().ok())
        })
}

/// Returns `magnet` with every tracker from `trackers` it doesn't already
/// list appended.
pub fn add_trackers<S: AsRef<str>>(magnet: &str, trackers: &[S]) -> String {
    let existing: Vec<String> = Url::parse(magnet)
        .map(|url| {
            url.query_pairs()
                .filter(|(key, _)| key == "tr")
                .map(|(_, value)| value.into_owned())
                .collect()
        })
        .unwrap_or_default();

    let mut extra = form_urlencoded::Serializer::new(String::new());
    let mut added = false;
    for tracker in trackers {
        let tracker = tracker.as_ref();
        if !existing.iter().any(|t| t == tracker) {
            extra.append_pair("tr", tracker);
            added = true;
        }
    }
    if !added {
        return magnet.to_owned();
    }
    let separator = if magnet.contains('?') { '&' } else { '?' };
    format!("{magnet}{separator}{}", extra.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash() -> InfoHash {
        "1dc689206d6f6ef6021d558d95350ed09004b40c".parse().unwrap()
    }

    #[test]
    fn builds_magnet_with_encoded_name_and_trackers() {
        let magnet = build_magnet(
            &hash(),
            "Ubuntu 20.04 & friends",
            &["udp://t.example:80/announce"],
        );
        assert_eq!(
            magnet,
            "magnet:?xt=urn:btih:1dc689206d6f6ef6021d558d95350ed09004b40c\
             &dn=Ubuntu+20.04+%26+friends&tr=udp%3A%2F%2Ft.example%3A80%2Fannounce"
        );
    }

    #[test]
    fn builds_bare_magnet_without_name_or_trackers() {
        let magnet = build_magnet::<&str>(&hash(), "", &[]);
        assert_eq!(
            magnet,
            "magnet:?xt=urn:btih:1dc689206d6f6ef6021d558d95350ed09004b40c"
        );
    }

    #[test]
    fn extracts_infohash_from_built_and_base32_magnets() {
        let magnet = build_magnet(&hash(), "x", DEFAULT_TRACKERS);
        assert_eq!(infohash_from_magnet(&magnet), Some(hash()));

        let b32 = data_encoding::BASE32.encode(hash().as_bytes());
        assert_eq!(
            infohash_from_magnet(&format!("magnet:?xt=urn:btih:{b32}")),
            Some(hash())
        );
        assert_eq!(infohash_from_magnet("https://example.org"), None);
        assert_eq!(infohash_from_magnet("magnet:?dn=no-hash"), None);
    }

    #[test]
    fn add_trackers_skips_existing_ones() {
        let magnet = build_magnet(&hash(), "x", &["udp://a:1/announce"]);
        let completed = add_trackers(&magnet, &["udp://a:1/announce", "udp://b:2/announce"]);
        assert_eq!(completed.matches("tr=").count(), 2);
        assert!(completed.ends_with("tr=udp%3A%2F%2Fb%3A2%2Fannounce"));
        assert_eq!(add_trackers(&completed, &["udp://b:2/announce"]), completed);
    }
}
