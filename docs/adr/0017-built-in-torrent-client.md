# 0017. Built-in torrent client

- **Status:** Accepted
- **Date:** 2026-11-10
- **Amends:** [0008](0008-os-download-handoff.md) (the OS hand-off stays, as the fallback when the built-in client is off) and [0010](0010-no-telemetry.md) (torrent traffic is added to the list of outgoing requests)

## Context

ADR 0008 chose to hand downloads to the OS only. Users without a torrent client hit a dead end: "No torrent client found". The plan left a built-in downloader as a later option, built on `librqbit`, and the user now asked for a full torrent client in Hashlark: on desktop, on the headless server and on Android.

Constraints:

- **Privacy stays as it is.** Nothing may be sent to us, and no telemetry is added (ADR 0010). Peers do see the user's IP address, so the user has to be told that before the first download.
- **Tor and the search proxy can't carry torrent traffic properly.** They carry TCP only. DHT and uTP use UDP, so they would still go direct. A proxy that covers only part of the traffic would give a false sense of privacy.
- **The engine must fit the stack.** `librqbit` 9.1 uses the same major versions of tokio, reqwest, axum and sqlx as the rest of the workspace, and its `rust-tls` feature avoids OpenSSL.
- **The server must stay safe.** Torrent routes are behind the same API keys, and a remote user must not be able to read or write outside chosen folders.

## Decision

**A new crate, `crates/hashlark-torrent`,** depends on `hashlark-core`. The desktop app and the server use it through the HTTP API (ADR 0004). Android uses it through `hashlark-ffi` with JSON (ADR 0015). It uses `librqbit` 9.1 with `default-features = false` and the `rust-tls` feature.

- Rejected: a module inside `hashlark-core`. The CLI and the definition tooling would then compile `librqbit` for nothing.
- Rejected: librqbit's `http-api` and `webui` features. Hashlark has its own API and UI.
- Rejected: `default-tls`, which pulls in native-tls and OpenSSL.

**The built-in client is on by default.** The setting `torrents.enabled` turns it off. Then results go to the external client as ADR 0008 describes. The "No torrent client" dialog also offers to turn the built-in client on.

**Torrent traffic never uses Tor or the search proxy.** It connects directly. An optional SOCKS5 proxy (`torrents.proxy`) covers TCP connections to peers and trackers only. DHT and uTP go direct. The UI and the user guide say so.

**Peer id and user agent are librqbit's defaults.** We don't send a Hashlark peer id. Private trackers whitelist known clients, so a Hashlark id would gain nothing.

**Public id: the info hash,** as 40 lowercase hex characters. librqbit's numeric ids change between runs.

**Our own SQLite table** (`torrents`, migration `0004`) holds what librqbit doesn't track: queue position, the user's pause, seeding limits, cumulative up and down totals, added and completed times, and the source result. librqbit keeps its own state in `<data>/torrents/`.

**Queue, seeding, schedule, connection and Wi-Fi rules are pure functions** over a snapshot. A one-second tick applies the result with `pause` and `unpause`. The pure functions are unit-tested without a network.

**Defaults:**

- 3 active downloads and 5 active seeds; speeds unlimited; alternative limits 1024 KiB/s down and 256 KiB/s up.
- Seed until ratio 1.0 on desktop and server; ratio 0 on Android.
- Connections: 1000 in total and 200 per torrent on desktop and server; 400 and 100 on Android.
- DHT, peer exchange, local discovery, uTP and UPnP are on.
- Listen port: picked at random on first start on desktop and Android; 6881 on the server, so Docker port mapping is predictable.

**Storage.** On the server, downloads, the watched folder and the paths for creating torrents must lie inside `[torrents] roots`. The default is `[download_dir]`. The desktop app and Android have no such limit.

**Notice.** Before the first download, a one-time notice says that downloading shares pieces with other people and shows them the user's IP address. The answer is stored in settings.

**Power and tray (desktop only).** Keep awake uses `keepawake`. "When all done" uses `system_shutdown`. Battery detection uses `starship-battery`. These live in the desktop shell only. Closing the window keeps the client running in the tray (`close_to_tray`).

**The watched folder** is polled every five seconds. Added files are renamed to `*.torrent.added`, not deleted.

**Creating torrents** is available on desktop and server. Android can't create them, because a file picked through SAF has no stable path for librqbit to read.

## Consequences

- Users can download without another app. Hashlark is no longer a search-only tool, so the README and the user guide say so, and the Privacy section now lists torrent traffic.
- **What peers can see:** the user's IP address and the torrents the user downloads or seeds. Torrent traffic is outside Tor, and trackers and DHT use the system DNS, not DoH. These limits are in the notice, the Settings screen and the user guide.
- **The global connection limit is approximate.** librqbit only has a per-torrent peer limit, so each running torrent gets a share of the global one. Incoming connections can briefly go over the total.
- **Server setup needs two steps.** The listen port 6881 (TCP and UDP) must be published for incoming peers, and the download folder must be mounted and listed in `roots`.
- **Android downloads run in a foreground service** with a notification. Wi-Fi and unmetered networks are the default. Some phone makers' battery savers still stop such services, and the user has to exclude Hashlark from them.
- **Binary size grows** by an estimated 3 to 5 MB per binary. The Android build is checked against the APK size budget (`docs/android-plan.md`).
- **New dependencies:** `librqbit` and its tree, `keepawake` (MIT), `system_shutdown` (MIT or Apache-2.0) and `starship-battery` (ISC). `cargo deny` checks their licences.
- **Known gaps, stated in the user guide:** no streaming, no RSS, no categories or tags, no moving of storage after adding, no per-file priorities (only select or skip), no protocol encryption (MSE/PE), no tracker status (only the tracker list), no performance presets and no CLI client.
- **Testing:** the policies are unit-tested. `tests/loopback.rs` runs two sessions on `127.0.0.1` and checks a fetch by magnet, so it needs no internet access.
- A later move to another torrent engine, or a Hashlark peer id, would need a new ADR.
