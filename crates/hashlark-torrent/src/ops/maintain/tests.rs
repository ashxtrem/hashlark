// SPDX-License-Identifier: GPL-3.0-or-later

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
    let old = original.clone();
    let original = librqbit::torrent_from_bytes(&original).unwrap();

    let trackers = tracker_list(&strings(&[
        "udp://b.example:1",
        "http://a.example/announce",
    ]))
    .unwrap();
    let bytes = torrent_with_trackers(&old, INFO, &trackers).unwrap();
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

    let empty = torrent_with_trackers(&old, INFO, &[]).unwrap();
    let parsed = librqbit::torrent_from_bytes(&empty).unwrap();
    assert_eq!(parsed.info_hash, original.info_hash);
    assert!(ordered_trackers(&HashSet::new(), &empty).is_empty());
    // Trackerless: the old announce keys are gone, not left empty.
    assert!(!empty.windows(8).any(|w| w == b"announce"));

    // Unreadable old bytes: just announce, announce-list and info.
    let bare = torrent_with_trackers(b"garbage", INFO, &trackers).unwrap();
    let parsed = librqbit::torrent_from_bytes(&bare).unwrap();
    assert_eq!(parsed.info_hash, original.info_hash);
}

#[test]
fn torrent_with_trackers_keeps_other_keys() {
    let mut old = b"d8:announce19:http://old.example/13:announce-listll19:http://old.example/ee\
                    7:comment5:hello10:created by3:me!13:creation datei1700000000e4:info"
        .to_vec();
    old.extend_from_slice(INFO);
    old.extend_from_slice(b"8:url-listl17:http://w.example/ee");
    let original = librqbit::torrent_from_bytes(&old).unwrap();

    let trackers = tracker_list(&strings(&["udp://n.example:1"])).unwrap();
    let bytes = torrent_with_trackers(&old, INFO, &trackers).unwrap();
    let mut want = b"d8:announce17:udp://n.example:113:announce-listll17:udp://n.example:1ee\
                     7:comment5:hello10:created by3:me!13:creation datei1700000000e4:info"
        .to_vec();
    want.extend_from_slice(INFO);
    want.extend_from_slice(b"8:url-listl17:http://w.example/ee");
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        String::from_utf8_lossy(&want)
    );
    let parsed = librqbit::torrent_from_bytes(&bytes).unwrap();
    assert_eq!(parsed.info_hash, original.info_hash);
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
