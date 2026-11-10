# Plan: built-in torrent client

Approved by the user (2026-11-10: "Approved").

The user asked on 2026-11-10 for "an active torrent client in Hashlark, with all the major features a torrent client
needs". Until now Hashlark has been a search tool that hands results to another client. This plan adds a full
downloader on desktop, on the headless server and on Android, built on `librqbit`.

## 1. Why

- **Hashlark can't download anything itself.** Results go to the OS `magnet:` handler or are saved as `.torrent`
  (ADR 0008, `docs/adr/0008-os-download-handoff.md:10-12`; `apps/desktop/src-tauri/src/commands.rs:40-58`;
  `apps/android/.../system/MagnetLauncher.kt:37`). With no client installed, the user hits a dead end:
  "No torrent client found" (`apps/desktop/src/lib/components/NoHandlerDialog.svelte:19`, `ui/HashlarkRoot.kt:382-386`).
- **The roadmap already pointed here.** `docs/PLAN.md:75` lists a built-in downloader as a non-goal that is "possible
  in Phase 3 with `librqbit`", and `docs/PLAN.md:667` has "Built-in downloader (`librqbit`)" on the later roadmap.
- **librqbit fits the stack without forcing version splits.** librqbit 9.1.0 (Apache-2.0, released 2026-10-29) uses
  edition 2024, tokio 1, reqwest 0.13, axum 0.8 and sqlx 0.9. Those are the same majors as
  `crates/hashlark-core/Cargo.toml:24,33,37` and `crates/hashlark-server/Cargo.toml:12`. Its `rust-tls` feature avoids
  OpenSSL.

## 2. Decisions

User (2026-11-10):
- **Platforms: desktop, headless server and Android in v1.** The engine sits in the shared Rust layer, the desktop app
  and the server share one API and UI, and Android reaches it through the FFI.
- **The built-in client is the default.** A setting turns it off. When it is off, results go to the external client as
  today. When no external client exists either, the existing "No torrent client" dialog also offers "Turn on the
  built-in client".
- **Extras in v1:** opening magnets and `.torrent` files from the browser or OS (Hashlark as handler) plus a watched
  folder; alternative speed limits with a weekly schedule; creating torrents. **Streaming is out.**
- **Torrent traffic never uses Tor or the search proxy.** It connects directly, with an optional SOCKS5 proxy just for
  torrents. The UI says that DHT and uTP (UDP) still go direct.
- **Android downloads in the background** through a foreground service with a progress notification. It uses Wi-Fi or
  unmetered networks only by default (a setting allows mobile data). It stops seeding once a download finishes
  (default ratio 0).
- **Default seeding limit is ratio 1.0** on desktop and server. It can be changed globally and per torrent.
- **Quick add.** "Download" starts at once in the default folder with every file. A setting turns on an "Add torrent"
  dialog for every add; Shift+click (desktop) or a long-press (Android) opens it for one add.

User, second review (2026-11-10):
- **Connection limits: 1000 global, 200 per torrent.** Both can be edited, and each torrent can override its own.
- **Pause ≠ Stop, plus Force start.**
  - **Pause** is temporary: the torrent keeps its active slot and queue position.
  - **Stop** takes the torrent out of the queue: it never auto-starts, and a finished one stops seeding. **Start** puts
    it back.
  - **Force start** runs it now, ignoring queue limits.
  - **Delete** asks "Also delete downloaded files" (unticked by default) and removes the torrent and its stored state.
- **Filters:** All, Downloading, Seeding, Queued, Paused, Stopped, Finished, Error, plus a name filter. In the Queued
  filter, the list is in queue order and can be reordered.
- **Scheduling: a weekly 7×24 hour grid** (like µTorrent). Each hour is **Normal**, **Alternative speed**,
  **Seed only** (no downloading) or **Off** (everything paused). A manual "turtle" toggle forces alternative speed.
- **Power:**
  - **Keep awake while active.** The computer or phone doesn't sleep while something downloads; optionally while
    seeding too.
  - **When all done.** After the last download, do nothing, quit Hashlark, sleep, hibernate or shut down, with a 60 s
    cancel countdown. Desktop only.
  - **On battery.** Pause all, use alternative speed, or do nothing (default: alternative speed). This applies to a
    laptop on battery, and to Android with battery saver on or below 15% while not charging.

My calls (change any at approval):
- **The engine lives in a new crate `crates/hashlark-torrent`**, which depends on `hashlark-core`. Rejected: a module
  inside `hashlark-core`, because `hashlark-cli` and the definition tooling would then compile librqbit for nothing.
  This mirrors how `hashlark-lan` is split out.
- **librqbit 9.1 with `default-features = false, features = ["rust-tls"]`.** Rejected: `http-api` and `webui`
  (Hashlark has its own API and UI) and `default-tls` (native-tls/OpenSSL).
- **Public torrent id = the info hash** as 40 lowercase hex characters. librqbit's numeric ids change between runs.
- **Our own SQLite table holds what librqbit doesn't track.** That is the queue position, the user-paused flag, seed
  limits, cumulative up/down totals, added/completed times and the source result. librqbit's JSON persistence and
  fastresume hold the torrent state in `<data>/torrents/session/`.
- **The queue, seeding limits, scheduler and Wi-Fi gate are pure functions over a snapshot.** A 1-second tick applies
  the result with `pause`/`unpause`. Pure functions can be unit-tested without the network.
- **Defaults:**
  - Max active downloads **3**, max active seeds **5**, speeds **unlimited**.
  - Alternative limits **1024 KiB/s down / 256 KiB/s up**.
  - DHT, PEX, LSD, uTP and UPnP **on**.
  - Listen port: **picked once at random from 49152–65534** and saved on desktop and Android; **6881** on the server,
    so Docker port mapping is predictable.
- **Default download folder:**
  - desktop: `<OS Downloads>/Hashlark`;
  - server: `<data>/downloads`;
  - Android: `Download/Hashlark`, falling back to the app-specific Downloads folder when writing there fails.
- **The server only reads and writes inside allowed roots** (`[torrents] roots` in `hashlark.toml`, default
  `[<download dir>]`). This covers download folders, the watched folder and paths for creating torrents, so a remote API
  user can't write or read anywhere else on the box. The desktop app and Android have no such limit.
- **Peer/tracker identity is librqbit's default** ("rqbit" peer id and user agent). Rejected: a Hashlark peer id. It
  would be honest, but private trackers whitelist known clients and recognise neither, so it gains nothing.
- **A one-time notice before the first download.** It says: "Downloading shares pieces with other people and shows
  your IP address to them. Only download what you have the right to." The answer is stored in settings. This matches
  the first-run legal notice (`docs/PLAN.md:506`).
- **Desktop keeps running in the tray.** Closing the window hides it to a tray icon while the built-in client is on
  (setting `close_to_tray`, default on). A one-time toast explains this. The tray menu has Show, Pause all / Resume all,
  Alternative speed (turtle), "When all done ▸" and Quit.
- **Completion notifications come from the Rust shell** (`tauri-plugin-notification`), so they arrive while the
  window is hidden. The server sends none, and Android uses its service notification.
- **The watched folder is polled every 5 s and only exists on desktop and server.** Added files are renamed to
  `*.torrent.added`, not deleted.
- **Creating torrents is desktop and server only.** On Android, a SAF-picked file has no stable filesystem path for
  librqbit to read. The torrent is seeded at once, and its `.torrent` and magnet can be fetched.
- **The UI gets live updates over one SSE stream** (`GET /torrents/events`), sending a snapshot every 1 s. This fits
  the existing fetch+SSE client (`apps/desktop/src/lib/api/client.ts:141-161`). Android polls the FFI at 1 s while
  the Downloads screen is visible and every 2 s for the notification. Rejected: a new FFI callback interface, which
  ADR 0015 keeps to a minimum.
- **Android navigation.** "Downloads" becomes a top-level destination. With a bottom bar, which fits five labels
  (`ui/HashlarkRoot.kt:167`), History moves into a tab of the Favorites screen; rail and drawer show all six.
- **Results show a "In downloads" badge** when their info hash is already in the client.
- **How the global connection cap is enforced.** librqbit only has a per-torrent `peer_limit` (`AddTorrentOptions`;
  `None` = unlimited) and no session-wide cap. Each running torrent therefore gets
  `min(own limit, max(20, global / running torrents))`. The value is recalculated whenever the set of running torrents
  changes, and applied at runtime when T1 finds a setter, or else on the torrent's next start.
- **Android connection defaults: 400 global / 100 per torrent**, to save battery and phone/router load. Desktop and
  server: 1000 / 200.
- **"When all done" is a per-session choice, not a saved setting.** It sits in the Downloads toolbar and the tray menu,
  and it resets once it fires or when the app restarts, so a shutdown can't surprise you days later. "All done" means
  nothing is downloading or queued to download; seeding doesn't count.
- **Schedule "Off" also pauses force-started torrents.** Force start skips the queue limits, not the schedule.
- **Power crates (desktop shell only):**
  - `keepawake` 0.6 (MIT) for the OS sleep inhibitor;
  - `system_shutdown` 4.1 (MIT/Apache-2.0) for sleep, hibernate and shutdown (hibernate is hidden on macOS);
  - `starship-battery` 0.12 (ISC) to detect running on battery, polled every 30 s.
  - The server has no power features.
- **Out of v1 (known gaps):**
  - streaming;
  - RSS auto-download;
  - categories/tags (v1 only has status filters and a name filter);
  - moving storage after adding;
  - per-file priorities (v1 only selects or skips files);
  - protocol encryption (MSE/PE is not in librqbit);
  - performance presets;
  - a CLI client.

## 3. Engine: `crates/hashlark-torrent`

New crate, added to the workspace by the existing `members = ["crates/*", ...]` glob in `Cargo.toml`.

```
crates/hashlark-torrent/
  Cargo.toml        librqbit 9.1 (rust-tls), hashlark-core (path), tokio, serde, sqlx, thiserror, tracing, utoipa (optional, feature "openapi")
  src/lib.rs        pub use service::TorrentService; pub mod model;
  src/service.rs    TorrentService: start/stop/apply_settings, the 1 s tick, the event broadcast
  src/ops.rs        add / list / details / update / remove / peers / stats / create (each a TorrentService method)
  src/store.rs      queries for the `torrents` table (pattern: impl blocks per feature like engine/library.rs:38)
  src/model.rs      API JSON types (serde + cfg_attr openapi ToSchema, as in settings.rs:21)
  src/policy/queue.rs, seeding.rs, schedule.rs, network.rs   pure functions + tests
  src/watch.rs      watched-folder poller
  src/paths.rs      PathPolicy (Unrestricted | Roots(Vec<PathBuf>)), default folders
  tests/loopback.rs two sessions on 127.0.0.1: seed a created torrent, fetch it by magnet with initial_peers
```

### 3.1 Service lifecycle
- `TorrentService::new(engine: Engine, opts: ServiceOptions { data_dir, default_download_dir, path_policy })` is cheap.
  It holds `RwLock<Option<Running>>`.
- `start()` reads `engine.settings().torrents` (core `Settings`, `crates/hashlark-core/src/settings.rs:19-30`) and,
  when `enabled`, builds the librqbit `Session`:
  - `Session::new_with_opts(download_dir, SessionOptions { persistence: Json { folder: <data>/torrents/session },
    fastresume: true, dht: Some(_) | None, listen: Some(ListenerOptions { listen_addr: 0.0.0.0:<port>,
    enable_upnp_port_forwarding: upnp, .. }), connect: Some(ConnectionOptions { proxy_url: torrents.proxy, .. }),
    disable_local_service_discovery: !lsd, ratelimits, blocklist_url, .. })`.
  - Then it spawns the tick task with a `CancellationToken`.
  - Nothing from `crates/hashlark-core/src/net/mod.rs:78-106` (DoH, Tor, search proxy) is passed in.
- `stop()` cancels the tick, flushes the up/down totals to SQLite and calls `session.stop()`.
- `apply_settings(&Settings)` reacts to settings changes:
  - enabled→disabled: `stop()`; torrents stay persisted;
  - disabled→enabled: `start()`;
  - port, DHT, LSD, proxy or blocklist changed: restart the session;
  - limits or schedule changed: `session.ratelimits.set_download_bps/set_upload_bps`.
- It is called after a settings save:
  - from the server `PUT /settings` handler (`crates/hashlark-server/src/api.rs:264`);
  - from FFI `set_settings` (`crates/hashlark-ffi/src/lib.rs:384`).
  - The core `Engine` doesn't know about torrents (dependency direction).
- `subscribe() -> broadcast::Receiver<TorrentEvent>` sends `Snapshot` (every tick), `Completed { id, name }` and
  `Error { id, message }`.
- `TorrentService` implements `Debug` by hand (printing the state and the torrent count). Both `AppState`
  (`crates/hashlark-server/src/lib.rs:34`) and desktop `Backend` (`main.rs:22`) derive `Debug`, and librqbit's `Session`
  may not implement it.

### 3.2 Operations (all return `model` types; errors map to core `Error::Invalid`/`NotFound` so the server's
`ApiError` mapping at `crates/hashlark-server/src/error.rs:63-98` applies unchanged)

| Method | Does |
|---|---|
| `add(AddRequest)` | `source`: `magnet` \| `torrent` (base64 bytes) \| `result_id`. For `result_id`, it calls `Engine::resolve(id, Some(Magnet))` (`engine/mod.rs:347`); when that gives only a `.torrent` URL, it fetches the file with `Engine::fetch_torrent` (`engine/mod.rs:408`, which keeps the size cap and bencode check). Options: `save_dir`, `files: Option<Vec<usize>>`, `paused`. It checks `save_dir` against the `PathPolicy`. A duplicate info hash returns the existing torrent with `already: true`. |
| `list()` | `Vec<TorrentSummary>` |
| `details(id)` | `TorrentDetails` (summary + files + trackers + limits + magnet) |
| `update(id, TorrentPatch)` | `action`: `pause` \| `resume` \| `stop` \| `start` \| `force_start` (sets `run_mode`, §3.3); `files` (selected indices → `session.update_only_files`); `limits {down_kib, up_kib}`; `peer_limit`; `seed {ratio, minutes}`; `queue_pos` (`top`/`up`/`down`/`bottom`, or an absolute index for drag reorder) |
| `remove(id, delete_files)` | `session.delete(id, delete_files)`; deletes the row and the stored `.torrent` copy (`<data>/torrents/created/<hash>.torrent`) |
| `peers(id)` | `Vec<PeerInfo>` from the live per-peer snapshot (T1 confirms the API) |
| `stats()` | `SessionStats { down_bps, up_bps, listen_port, dht_nodes?, schedule_mode, turtle, on_battery, network_allowed, peers_connected, counts by state }` |
| `bulk(BulkAction)` | `ids` (or all) + `action` as in `update`; also `remove { delete_files }`, `turtle(bool)` |
| `set_power(PowerState { on_battery })` | desktop shell and Android report battery state (§3.3) |
| `create(CreateRequest)` | `{ path, trackers, name?, piece_length? }`; checks `PathPolicy`; `session.create_and_serve_torrent`; stores the `.torrent` bytes at `<data>/torrents/created/<hash>.torrent` |
| `torrent_file(id)` | the `.torrent` bytes (created torrents and any torrent with metadata) |
| `set_network_allowed(bool)` | Android Wi-Fi gate (see 3.3) |

### 3.3 Policies (pure, in `src/policy/`)
- **Run modes** (stored per torrent as `run_mode`):

  | Mode | Set by | Active slot | Transfers |
  |---|---|---|---|
  | `auto` | add, Resume, Start | queue decides | when the queue runs it |
  | `paused` | Pause | keeps it: counts toward the limit, so nothing queued takes its place | no |
  | `stopped` | Stop | releases it: leaves the queue; a finished one stops seeding | no |
  | `forced` | Force start | ignores limits and doesn't count toward them | yes, unless the schedule says Off |

- **Queue.** `plan(facts: &[TorrentFacts], cfg, ctx: PlanCtx { network_allowed, schedule_mode, on_battery_pause })
  -> Vec<(Id, Desired)>`.
  - `Desired` is one of: `Run`, `QueuedDownload`, `QueuedSeed`, `Paused`, `Stopped`, `Scheduled` (the schedule says
    Off or Seed only), `OnBattery`, `NetworkWait`, `Done`.
  - `auto` incomplete torrents run in `queue_pos` order, up to `max_active_downloads` minus the `paused` incomplete
    ones.
  - Magnets still fetching metadata count as downloads.
  - Complete torrents under their seed limit seed, up to `max_active_seeds` minus the `paused` complete ones, lowest
    ratio first.
  - `forced` torrents run on top of these limits.
  - Context comes before modes, in this order:
    1. `network_allowed == false` makes everything `NetworkWait`;
    2. `on_battery_pause` makes everything `OnBattery`;
    3. schedule Off makes everything `Scheduled`;
    4. schedule Seed only makes only downloads `Scheduled`.
  - The tick turns the diff into `pause`/`unpause` calls.
- **Peers.** `peer_share(global, own_limits, running) -> per-torrent limit`, using
  `min(own, max(20, global / running))` (§2).
- **Seeding.**
  - `ratio = uploaded_total / max(downloaded_total, selected_size)`.
  - Seeding stops at `seed_ratio` or after `seed_minutes` of seeding, whichever comes first. A per-torrent override wins
    over the global value; `None` means no limit.
  - Ratio `0.0` means it stops as soon as the download completes.
  - The totals are our cumulative counters (base from SQLite + this run's librqbit counters), written every 30 s and on
    stop.
- **Schedule.**
  - `mode_at(grid: Option<&WeekGrid>, now_local) -> ScheduleMode`, where `ScheduleMode` is `Normal`, `Alt`, `SeedOnly`
    or `Off`.
  - `WeekGrid` is 168 cells, Monday 00:00 first, in local time. In JSON it is a 168-character string of `n`/`a`/`s`/`o`.
  - The rate limits in effect: the alternative limits when `turtle` is on, or the schedule says `Alt`, or the battery
    policy says `AltSpeed`; otherwise the normal limits.
  - A DST change uses the local wall-clock hour, so an hour is skipped or repeated on DST change days.
- **Power.** `battery_effect(on_battery, setting) -> None | Alt | Pause`. The setting is `on_battery`
  (`nothing` / `alt_speed` / `pause`).
- **Network gate.** `network_allowed` is `true` everywhere except Android. There, the service sets it from
  `ConnectivityManager` when `unmetered_only` is on.

### 3.4 Watched folder (desktop, server)
`watch.rs` polls `torrents.watch_dir` every 5 s and adds each `*.torrent` with the default options. A successful add
renames the file to `*.torrent.added`; a failed add renames it to `*.torrent.failed`, with a log line. The folder must
pass the `PathPolicy`.

## 4. Settings and data

### 4.1 `TorrentSettings` in `crates/hashlark-core/src/settings.rs`
New field `torrents: TorrentSettings` on `Settings` (`settings.rs:19-30`), with `#[serde(default)]` like its siblings
(`:22`) and a manual `Default`:

| Field | Type | Default |
|---|---|---|
| `enabled` | bool | `true` |
| `download_dir` | `Option<PathBuf>` | `None` (resolved per platform, §2) |
| `ask_on_add` | bool | `false` |
| `notice_accepted` | bool | `false` |
| `listen_port` | u16 | `0` = pick on first start and save |
| `upnp`, `dht`, `lsd`, `utp` | bool | `true` |
| `limits`, `alt_limits` | `SpeedLimits { down_kib: Option<u32>, up_kib: Option<u32> }` | unlimited; `1024 / 256` |
| `turtle` | bool | `false` |
| `schedule` | `Option<WeekGrid>` (168-char string) | `None` (always Normal) |
| `max_active_downloads`, `max_active_seeds` | u16 | `3`, `5` |
| `peer_limit_global`, `peer_limit_torrent` | u32 | `1000`, `200`; `400`, `100` when `cfg!(target_os = "android")` |
| `keep_awake` | `off` \| `downloading` \| `active` (also seeding) | `downloading` (desktop and Android; ignored on the server) |
| `on_battery` | `nothing` \| `alt_speed` \| `pause` | `alt_speed` (desktop and Android) |
| `seed_ratio` | `Option<f32>` | `Some(1.0)`; `Some(0.0)` when `cfg!(target_os = "android")` |
| `seed_minutes` | `Option<u32>` | `None` |
| `proxy` | `Option<Url>` | `None` |
| `blocklist_url` | `Option<Url>` | `None` |
| `watch_dir` | `Option<PathBuf>` | `None` |
| `unmetered_only` | bool | `true` (only read on Android) |
| `close_to_tray` | bool | `true` (only read on desktop) |

`PathBuf` fields use `#[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]`, like
`DownloadSettings.torrent_dir` (`settings.rs:172`).

`torrents.download_dir` is a different setting from the existing `downloads.torrent_dir` (`settings.rs:170-173`, read
by `Engine::download_dir`, `engine/mod.rs:393-402`). The existing one is where saved `.torrent` files go, and it stays.

`Settings::validate` (`settings.rs:213-263`) gains these checks:
- `proxy` scheme is `socks5`/`socks5h`;
- `listen_port` is 0 or 1024–65535;
- `seed_ratio` is 0–100;
- `schedule` is exactly 168 characters of `n`/`a`/`s`/`o`;
- `max_active_*` is 1–100;
- `peer_limit_torrent` is 10–2000, and `peer_limit_global` is 20–10000 and ≥ `peer_limit_torrent`.

### 4.2 Migration `crates/hashlark-core/migrations/0004_torrents.sql`
Next after `0003_builtin_repo.sql`; forward-only, as ADR 0006 requires.

```sql
CREATE TABLE torrents (
  info_hash      TEXT PRIMARY KEY,           -- 40 lowercase hex
  name           TEXT NOT NULL,
  save_dir       TEXT NOT NULL,
  source         TEXT NOT NULL,              -- magnet | torrent | result | created | watch
  result_id      TEXT,
  added_at       INTEGER NOT NULL,           -- epoch ms (ADR 0006)
  completed_at   INTEGER,
  seeding_since  INTEGER,
  run_mode       TEXT NOT NULL DEFAULT 'auto', -- auto | paused | stopped | forced
  queue_pos      INTEGER NOT NULL,
  peer_limit     INTEGER,                    -- NULL = global per-torrent default
  seed_ratio     REAL,                       -- NULL = global
  seed_minutes   INTEGER,
  down_kib       INTEGER,
  up_kib         INTEGER,
  downloaded_total INTEGER NOT NULL DEFAULT 0,
  uploaded_total   INTEGER NOT NULL DEFAULT 0,
  done           INTEGER NOT NULL DEFAULT 0  -- seed limit reached
);
```
The migration lives in core because the core `MIGRATOR` (`crates/hashlark-core/src/store.rs:16`) owns the one schema.
`hashlark-torrent/src/store.rs` queries it through `engine.store().pool()` (`engine/mod.rs:157`, `store.rs:57`).

### 4.3 Server config (`crates/hashlark-server/src/config.rs:13-30`, `deny_unknown_fields`)
New optional table:
```toml
[torrents]
download_dir = "/downloads"      # overrides the settings default on the server
roots = ["/downloads"]           # PathPolicy; default [download_dir]
```
`docs/deploy/hashlark.example.toml` gets this table (commented), since `config.rs:58` parses that file in a test.
`Dockerfile:48-49` (`VOLUME /data`, `EXPOSE 8787`) gains `VOLUME /downloads` and `EXPOSE 6881/tcp 6881/udp`.

## 5. HTTP API (`crates/hashlark-server`)

- New file `src/torrents.rs` holds the handlers, with `#[utoipa::path]` like `api.rs:87`. `api.rs` is already 715 lines.
- `AppState` (`lib.rs:35-40`) gains `torrents: TorrentService`.
- `protected_routes()` (`lib.rs:116-147`) gains the routes below.
- `ApiDoc` (`lib.rs:79-100`) lists the new schemas.

| Route | Handler |
|---|---|
| `GET /torrents` | list |
| `POST /torrents` | add (JSON `AddRequest`); `400 invalid` "the built-in client is off" when disabled, since `error.rs:63-98` has no 409 and the UI checks the setting first anyway. The route gets `DefaultBodyLimit::max(14 MiB)`: the base64 of a 10 MiB `.torrent` (`MAX_TORRENT_BYTES`, `engine/mod.rs:574`) is about 13.4 MiB, and axum's 2 MB default would answer 413 |
| `GET /torrents/events` | SSE: `snapshot` every 1 s `{ session, torrents }`, plus `completed` and `error`. Built like `api.rs:96-108` from `subscribe()` |
| `GET /torrents/stats` | session stats |
| `POST /torrents/bulk` | `{ ids?, action }`: pause / resume / stop / start / force_start / remove `{delete_files}` for the ids or for all; `turtle` |
| `POST /torrents/create` | create |
| `GET /torrents/{id}` | details |
| `PATCH /torrents/{id}` | update (`action`, files, limits, peer_limit, seed, queue_pos) |
| `DELETE /torrents/{id}?delete_files=bool` | remove |
| `GET /torrents/{id}/peers` | peers |
| `GET /torrents/{id}/file` | `.torrent` download (`application/x-bittorrent`) |

Signature changes and their callers:
- `serve_local(engine)` (`lib.rs:261`) becomes `serve_local(engine, torrents)`. Its only caller is desktop
  `start_backend` (`apps/desktop/src-tauri/src/main.rs:40`).
- `router(engine, config)` (`lib.rs:157`) takes the service the same way. Callers: `serve_local` (`lib.rs:265`), the
  server `main.rs:261`, and the `app()` helper in `tests.rs:69` that every server test uses. `AppState` is built in
  one place only (`lib.rs:158`).
- The server's `main.rs` builds the service with `PathPolicy::Roots` from the config, after
  `start_background_tasks` (`main.rs:213`). `torrents.stop()` runs after `serve` returns (`main.rs:285-290`, before
  `Ok(())` at `:291`); Ctrl-C already triggers a graceful shutdown (`main.rs:264-268`).
- The `PUT /settings` handler (`api.rs:264`) calls `torrents.apply_settings(&saved)`. On the server, it first rejects a
  `torrents.download_dir`/`watch_dir` outside the roots with `400 invalid`.

## 6. Desktop shell (`apps/desktop/src-tauri`)

- **Start the service.** `start_backend` (`main.rs:30-48`) builds
  `TorrentService::new(engine.clone(), ServiceOptions { path_policy: Unrestricted, .. })`, calls `start()`, and passes it
  to `serve_local`. `Backend` (`main.rs:23-28`) gains `torrents`. The existing `RunEvent::Exit` handler
  (`main.rs:121-125`, which already does `block_on(lan.stop())`) also blocks on `torrents.stop()`, so fastresume and
  the totals are flushed.
- **Magnets from paired devices.** Today `lan.rs:79` hands a magnet received over LAN to the OS through
  `hand_off_magnet`. With the built-in client on, it is added to the built-in client instead, and the window shows a
  toast. The same rule applies on Android (T11 finds where `LanController` opens a received magnet).
- **Open magnets and `.torrent` files.**
  - Add `tauri-plugin-deep-link` (scheme `magnet`) and `bundle.fileAssociations`
    `[{ ext: ["torrent"], mimeType: "application/x-bittorrent", role: "Viewer" }]` in `tauri.conf.json`.
  - The single-instance callback (`main.rs:60-65`, which ignores argv today) and first-launch argv both go through
    `open_args(args)`. A `magnet:` URL or a `.torrent` path emits the Tauri event `torrent-open { magnet | path }` and
    focuses the window.
  - Pair the npm package `@tauri-apps/plugin-deep-link` with the crate version (memory: tauri plugin version pairing).
- **Register as the magnet handler.** A command `magnet_handler_register(bool)` calls deep-link `register`/`unregister`
  on Windows/Linux. On macOS, `Info.plist` lists the scheme and the user picks the default.
  `magnet_handler_status` (`commands.rs:193`, unused today) returns `{ registered, is_self }`.
- **Never hand a magnet back to ourselves.** `magnet_handler::registered()` (`magnet_handler.rs:7-40`) also reports
  whether the handler is Hashlark:
  - Windows: compare the `magnet` open command's exe with `std::env::current_exe()`;
  - Linux: compare the `xdg-mime` default with Hashlark's `.desktop` id.
  - `hand_off_magnet` (`commands.rs:47-58`) treats "handler is Hashlark" as `NoHandler`, so the built-in-off + Hashlark
    handler case can't loop.
  - A magnet that reaches Hashlark through argv while the built-in client is off shows the NoHandler dialog with
    "Turn on the built-in client".
- **Tray.** Turn on the `tray-icon` feature of `tauri`. `close_to_tray` hides the window on `CloseRequested`; the tray
  menu is listed in §2.
- **Notifications.** Add `tauri-plugin-notification` (pair with `@tauri-apps/plugin-notification`). A task subscribed
  to `torrents.subscribe()` shows "Download finished: <name>" on `Completed`.
- **Capabilities** (`capabilities/default.json:8-13`) gain `deep-link:default` and `notification:default`.
- **Power** (new `src/power.rs`, desktop only):
  - **Keep awake.** It holds a `keepawake` guard (idle sleep only; the display may sleep) while
    `keep_awake == downloading` and something downloads, or `== active` and something downloads or seeds. It drops the
    guard otherwise. It is driven by the `Snapshot` events.
  - **Battery.** `starship-battery` is polled every 30 s; a change calls `torrents.set_power(PowerState { on_battery })`.
    With no battery (desktops), it is never on battery.
  - **When all done.**
    - The commands `set_when_done(action)` / `when_done()` take `nothing | quit | sleep | hibernate | shutdown` and live
      in memory only.
    - When the snapshot goes from "something downloading or queued" to "none", the shell shows the window and emits
      `when-done-countdown { action, seconds: 60 }`.
    - Unless the UI calls `cancel_when_done`, the action runs on expiry: `system_shutdown::{sleep, hibernate,
      shutdown}`, or for quit, `app.exit(0)` after `torrents.stop()`.
    - Before sleep, hibernate or shutdown it awaits `torrents.stop()`, so state is flushed.
    - Hibernate is hidden where `system_shutdown` doesn't support it (macOS).

## 7. Desktop / web UI (`apps/desktop/src`, also served by the headless server)

### 7.1 API client and state
- After the server task, regenerate `src/lib/api/openapi.json` and `schema.d.ts` with `pnpm api:types`
  (`package.json:14`).
- `client.ts` (`class Api :118`) gains `torrents*` methods and `torrentEvents()`. The latter reuses `SseParser`
  (`src/lib/api/sse.ts:14`) like `search` (`client.ts:141-161`).
- New `src/lib/state/torrents.svelte.ts`:
  - It holds `list`, `session`, `connected` and a `byHash` set (for "In downloads" badges).
  - It subscribes once from `routes/+layout.svelte` `onMount` (`:22-27`) and reconnects with backoff.
  - `add(req, { dialog })` runs the add flow: the notice (§2), then the Add dialog when `ask_on_add` or Shift is held,
    then `api.addTorrent`.

### 7.2 Downloads page `src/routes/downloads/+page.svelte`
- **Sidebar.** `Sidebar.svelte:8-12` gains "Downloads" after Search, with a count of active torrents. The sidebar
  footer shows global ↓/↑ speed and the turtle (alternative speed) toggle.
- **Toolbar:**
  - Add (magnet text or `.torrent` file picker; the dialog plugin is already allowed, `capabilities/default.json`);
  - Create torrent;
  - Pause all / Resume all;
  - "When all done" menu (desktop only: Nothing, Quit, Sleep, Hibernate, Shut down; it shows the armed action);
  - filter tabs with counts: All, Downloading, Seeding, Queued, Paused, Stopped, Finished, Error; plus a name filter.
  - "Finished" means 100% of the selected files, whether seeding, done or stopped.
- **List.** Columns: queue # (Queued filter), name, size, a progress bar, status, ↓, ↑, ETA, ratio and peers
  (connected/limit).
  - Status shows why a torrent isn't moving: Queued, Paused, Stopped, "Scheduled: off", "Scheduled: seed only",
    "On battery", "Waiting for Wi-Fi", Done (seed limit), or Error.
  - Row actions, also for a multi-selection: Pause/Resume, Stop/Start, Force start, Move to top / up / down / bottom,
    and Delete.
  - **Delete** opens a confirmation dialog listing the names, with "Also delete downloaded files" unticked. The button
    reads "Delete and remove files" when it is ticked.
  - In the Queued filter, rows are in queue order and can be dragged to reorder; this sends `queue_pos` as an index.
  - Below 720 px wide, rows become cards (the web UI is used on phones). There, reordering uses the move actions.
  - Keyboard path:
    - arrow keys move the selection;
    - Space pauses or resumes;
    - Shift+S stops or starts;
    - Ctrl/Cmd+↑/↓ moves in the queue;
    - Delete opens the delete dialog;
    - Enter opens details.
- **When-done countdown.** `WhenDoneDialog.svelte` listens for `when-done-countdown` and shows the action and the
  seconds left, with Cancel focused by default.
- **Details panel (tabs):**
  - Overview: save folder, hash, added/completed, ratio, per-torrent speed limits, peer limit, seed limit, and the
    magnet with a copy button.
  - Files: checkbox tree with per-file progress.
  - Peers.
  - Trackers.
  - Actions: Open folder (`reveal_path`, `commands.rs:97`; desktop only) and Remove (keep or delete files).
- **Dialogs.** `AddTorrentDialog.svelte` (folder, file tree once metadata is known, start paused) and
  `CreateTorrentDialog.svelte` (desktop: file/folder picker; web: a path text field checked against roots; trackers
  textarea).
- **Adding in other ways.** Drop `.torrent` files anywhere on the window, or paste a magnet (Ctrl/Cmd+V) on the
  Downloads page. The `torrent-open` Tauri event from §6 goes through `torrents.add`.

### 7.3 Results and settings
- **Download action.** `src/lib/actions.ts` `download()` (`:13`) checks `settings.torrents.enabled`:
  - on: `torrents.add({ source: result_id })`, then a toast "Added to Downloads" with a "Show" link;
  - off: today's hand-off (`platform.ts:18-26`).
- **Result buttons.**
  - `ResultsList.svelte:102-109` and `ResultDetails.svelte:54` show "Download" as the main action when the client is on
    and "Open in client" when it is off.
  - The other action stays in the existing secondary actions.
  - Shift+click opens the Add dialog.
  - An "In downloads" badge appears when `torrents.byHash` has the result's hash.
- **NoHandler dialog.** `NoHandlerDialog.svelte:17-34` gains a "Turn on the built-in client" button when it is off.
  It saves the setting and retries the add.
- **Settings.** `routes/settings/+page.svelte` (309 lines) gets a new "Torrent client" section, built as
  `src/lib/components/TorrentSettingsPanel.svelte` to keep the page small:
  - on/off, folder, ask on add;
  - port + UPnP, DHT/LSD/uTP;
  - limits and alternative limits;
  - **schedule:** `ScheduleGrid.svelte`, a 7×24 grid. Drag to paint the mode picked from a legend (Normal, Alternative,
    Seed only, Off). The keyboard moves a cell cursor with the arrow keys, and 1–4 set the mode. The current hour is
    outlined. There's an "Off" switch for the whole schedule.
  - connections: global and per-torrent peer limits;
  - queue sizes, seeding limits;
  - power (desktop): "Keep the computer awake" (Off / While downloading / While downloading or seeding) and "On
    battery" (Do nothing / Alternative speed / Pause all);
  - SOCKS5 proxy with the UDP note, blocklist URL, watched folder;
  - desktop only: "Open magnet links with Hashlark" and "Keep running in the tray".
  - The existing "Downloads" section (`:234-251`, `downloads.torrent_dir`, where saved `.torrent` files go) is renamed
    "Saved .torrent files". It stays separate from `torrents.download_dir`, the folder for downloaded content, so the
    two aren't confused.
  - "Network & privacy" (`:139`) gains one line: "Downloads don't use Tor or this proxy; see Torrent client".

## 8. FFI (`crates/hashlark-ffi`)

Following ADR 0015 (JSON in, JSON out), `HashlarkEngine` (`lib.rs:254-257`) gains methods. They live in a new
`src/torrents.rs` module, like `lan.rs`:
- `start_torrents(default_dir: String)`: Kotlin passes the resolved Android folder;
- `torrents() -> String`;
- `torrent(id) -> String`;
- `add_torrent(json) -> String`;
- `update_torrent(id, json)`;
- `remove_torrent(id, delete_files)`;
- `torrent_peers(id) -> String`;
- `torrent_stats() -> String`;
- `torrent_bulk(json)`;
- `set_network_allowed(bool)`.

`set_settings` (`lib.rs:384`) calls `apply_settings`. It must be a no-op when `start_torrents` hasn't been called, so
the existing `set_settings` tests (`crates/hashlark-ffi/src/tests.rs:147,154,185`) and Android callers
(`EngineInstrumentedTest.kt:121,149`, `ContinuityTest.kt:46`, `FakeApi.kt:96`) keep passing.
`crates/hashlark-ffi/Cargo.toml` adds `hashlark-torrent`.

## 9. Android (`apps/android`)

### 9.1 Core module
- The `HashlarkApi` interface and `EngineApi` (`core/HashlarkApi.kt:78`) gain the torrent calls. The test fake
  `app/src/test/.../FakeApi.kt:58-63` implements the interface, so it gets stubs too.
- The new `core/TorrentModels.kt` has `@Serializable` models decoded with `HashlarkJson` (`core/Models.kt:18`).
- **The Kotlin `Settings` model gains `torrents: TorrentSettings = TorrentSettings()` with every field from §4.1**
  (`core/Models.kt:371-378`). Without it, Android would wipe the torrent settings on every save:
  - every Android save encodes the whole Kotlin object (`HashlarkApi.kt:128-129`, `AppViewModel.kt:152-169`,
    `SettingsViewModel.kt:48-53`, `HashlarkRoot.kt:288`);
  - `ignoreUnknownKeys` (`Models.kt:18-19`) drops the unknown `torrents` key on read;
  - serde then resets the missing section to defaults.
  - `ModelsTest` gets a round-trip that keeps a non-default `torrents` value.

### 9.2 Service, storage, network (app module)
- **Service.** New `system/TorrentService.kt`:
  - a foreground service with `foregroundServiceType="specialUse"` and the property
    `android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE` = "BitTorrent downloads the user started".
  - It starts when a torrent is added or the app opens with unfinished torrents. It stops itself when nothing is
    downloading, seeding, or waiting for Wi-Fi, the schedule or the battery. Paused and stopped torrents don't keep it
    alive.
  - The notification shows the overall progress plus ↓/↑ speed and has the actions "Pause all" and "Stop".
  - **Keep awake.** It holds a partial wake lock plus a `WifiManager` high-performance Wi-Fi lock while `keep_awake`
    says so (downloading, or downloading or seeding), and releases both otherwise.
- **Battery.** `system/PowerGate.kt` listens for `ACTION_BATTERY_CHANGED` and
  `PowerManager.ACTION_POWER_SAVE_MODE_CHANGED`. It reports `on_battery = (power saver on || level < 15%) && !charging`
  through `set_power`, and the notification says "On battery: alternative speed" or "Paused: battery".
- **Wi-Fi gate.** A `ConnectivityManager.NetworkCallback` calls `set_network_allowed(unmetered || !unmetered_only)`.
  While it waits, the notification says "Waiting for Wi-Fi".
- **Manifest** (`AndroidManifest.xml`):
  - permissions `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`, `POST_NOTIFICATIONS` (asked on the first add,
    API 33+), `WAKE_LOCK`, `ACCESS_NETWORK_STATE`, and `WRITE_EXTERNAL_STORAGE` with `maxSdkVersion="28"`;
  - `requestLegacyExternalStorage="true"` (API 29);
  - the `<service>`;
  - a `FileProvider`, so finished files open in other apps;
  - MainActivity intent filters for `VIEW` scheme `magnet`, and for `VIEW` content/file with mime type
    `application/x-bittorrent` or path pattern `.*\.torrent` (next to the YAML filters at `:88-108`).
- **Download folder.** It resolves to `Download/Hashlark`:
  - a write probe at start; on failure, `getExternalFilesDir(DIRECTORY_DOWNLOADS)`;
  - on API 26–28, ask for `WRITE_EXTERNAL_STORAGE` first;
  - on completion, `MediaScannerConnection.scanFile` runs, so files show in Files and the gallery.
- **Intents.** `IntentRouter` gains `magnet:` and `.torrent` routes, and `MainActivity.handle` (`:109-121`) reads
  `.torrent` bytes from a `content://` URI. The add flow then runs with `ask_on_add`.
- **Hand-off.** `MagnetLauncher.handlers()` (`system/MagnetLauncher.kt:25-31`) gains a filter on
  `context.packageName`; today it returns every matching activity. That way hand-off never picks Hashlark. Its readers
  are `open` (`:39`) and the settings client picker (`ui/settings/SettingsScreen.kt:310`).
  - Today `open()` falls back to an unpinned `startActivity` (`:37-43`). Once MainActivity has a `magnet:` filter, that
    could resolve to Hashlark itself.
  - So with no preferred package, `open()` targets the first entry of the filtered `handlers()` explicitly, and returns
    `NoHandler` when the list is empty.
- **API levels (minSdk 26, `app/build.gradle.kts:50`).**
  - `FOREGROUND_SERVICE_SPECIAL_USE`, the `specialUse` type and its property only exist on API 34+.
  - The service starts through `ServiceCompat.startForeground`, passing `ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE`
    on API 34+ and no type below.
  - `POST_NOTIFICATIONS` is asked only on API 33+.
- **`.torrent` intents** are read with the core's `MAX_TORRENT_BYTES` cap (`crates/hashlark-core/src/engine/mod.rs:573`),
  not with `MAX_DEFINITION_BYTES` (`IntentRouter.kt:28`, read at `MainActivity.kt:126`).
- **Saved `.torrent` files don't change.** `system/TorrentFiles.kt:19-60` keeps saving them through MediaStore. Content
  downloads can't use MediaStore, because librqbit needs a real file path; hence the `Download/Hashlark` path above.

### 9.3 UI (app module)
- **Navigation.** `Destination` (`ui/Destination.kt:14-20`) gains `Downloads` after `Search`.
  - With a bottom bar (`HashlarkRoot.kt:156-169`, where the comment notes that five labels fit), the bar leaves out
    `History`. Today it iterates every `Destination.entries` (`:231`); the drawer (`:202`) keeps all six.
  - On a bar layout, `Destination.History` stays a valid destination. Shortcuts and intents still reach it
    (`IntentRouter.kt:34-36`, `system/Shortcuts.kt:16,33`, `res/xml/shortcuts.xml:26-30`). Picking it shows the
    Favorites screen (`ui/library/LibraryScreens.kt:57`) with a Favorites | History tab row, and the bar highlights
    Favorites. `HistoryScreen` (`LibraryScreens.kt:119`) is reused as the tab body.
  - The exhaustive `when` over destinations (`HashlarkRoot.kt:332-338`) gains a `Downloads` branch.
  - Readers to keep passing: `SearchScreen.kt:163`, `SearchHarness.kt:145-146` (`navigationShown()` keys on the
    "Favourites" label) and `IntentRouterTest.kt:74-76`.
- **Downloads screen.** New `ui/downloads/`:
  - `DownloadsScreen.kt`: the same filter tabs as desktop, as a scrollable chip row with counts; a list with 1 s
    updates and status reasons; multi-select by long-press on a row; a bottom action bar with Pause/Resume,
    Stop/Start, Force start, Move up/down and Delete; reordering by drag handle in the Queued filter.
  - `TorrentDetailSheet.kt`: Files with checkboxes, Peers, Trackers, and Overview (with speed and peer limits), plus
    Open file and Delete.
  - `DeleteTorrentDialog.kt`: "Also delete downloaded files", unticked.
  - `AddTorrentSheet.kt`.
  - Layouts follow the existing window-shape rules (`WindowShapeTest`, `LayoutMatrixTest`).
  - On a result, a long-press opens the Add sheet. On a Downloads row, a long-press starts multi-select.
- **Result actions.** `ui/ResultActions.kt` `open()` (`:55`) adds with the built-in client when it's on, and a
  long-press opens the Add sheet. When it's off, it uses `openMagnet` (`:65`) as today.
- **No handler dialog.** The dialog at `HashlarkRoot.kt:382-386` gains "Turn on the built-in client".
- **Settings.** `ui/settings/SettingsScreen.kt` (545 lines) gets a new section "Torrent client" in a new file,
  `ui/settings/TorrentSettings.kt`, added to the section enum `:91-98`. The section is saved by name (`:106`,
  `:121`, `:135`), and the exhaustive `when` at `:167-173` gains its branch. It covers:
  - on/off, folder (read-only path + reset) and "Download on mobile data";
  - limits and alternative limits;
  - the schedule: `ScheduleGridEditor.kt`, a day picker plus a 24-hour strip of mode cells on compact width, and the
    full 7×24 grid on medium and expanded width;
  - peer limits, queue sizes and seeding limits;
  - "Keep the phone awake" and "On battery";
  - the proxy.
- **Notice.** The same one-time notice as desktop before the first add.

## 10. Tests to change or add

- **hashlark-torrent** unit tests:
  - `policy/queue.rs`:
    - queue limits;
    - metadata counts as a download;
    - order by `queue_pos`;
    - a paused torrent holds its slot (nothing queued starts);
    - a stopped one frees it and never auto-starts;
    - a forced one runs over the limit and doesn't count;
    - context order: network, then battery pause, then schedule Off, then Seed only;
    - schedule Off also pauses forced torrents;
  - `peer_share`: 1000 global over 3 running gives 200 each (own cap); over 100 running gives 20 (floor);
  - `seeding.rs`: ratio, minutes, overrides, ratio 0, a stopped finished torrent doesn't seed;
  - `schedule.rs`: `mode_at` for each mode, Monday 00:00 and Sunday 23:00 cells, turtle and battery giving alternative
    limits, a malformed grid rejected by `validate`;
  - `power`: `battery_effect` for each setting;
  - `paths.rs`: roots accept/reject, `..` traversal, symlink escape.
- **hashlark-torrent** `tests/loopback.rs`. Session A creates a 3 MiB temp file torrent and seeds it. Session B
  (DHT/LSD/UPnP off) adds its magnet with `initial_peers = [A]`. It must complete in < 60 s with byte-identical files,
  then pass these checks: pause/resume, stop/start, force start over a limit of 1, and delete with and without files
  (the files are gone or kept). This is offline, so it runs in CI.
- **core**:
  - `settings.rs` tests (`:279-300`): `TorrentSettings` defaults, a partial JSON without `torrents`, and validation
    errors;
  - `store.rs:130`: the migration applies.
- **server** `src/tests.rs`:
  - torrent routes need auth;
  - add from magnet → list → patch pause → delete;
  - `POST /torrents` when disabled → 409;
  - `save_dir` outside roots → 400;
  - SSE `snapshot` arrives;
  - `openapi_lists_routes` (`:303`) includes the new paths;
  - `providers_and_settings_round_trip` (`:268`) still passes with the new section;
  - `config.rs:53` covers `[torrents]` parsing.
- **UI vitest**:
  - the primary-action choice (on/off/Shift);
  - speed/ETA/ratio formatters;
  - the schedule grid ↔ 168-character string conversion, and painting a drag range;
  - the SSE snapshot reducer;
  - the filter tab counts (Finished includes stopped and done finished torrents);
  - the delete dialog sends `delete_files` only when ticked.
- **Android unit**:
  - `IntentRouterTest` gets magnet and `.torrent` routes;
  - network-gate and power-gate logic tests (power saver, < 15%, charging);
  - the schedule editor's day-strip ↔ grid mapping;
  - `LayoutMatrixTest`/`WindowShapeTest` get the 6th destination and the bar rule;
  - a `ModelsTest` decode of the torrent JSON.

## 11. Tasks

| Task | Files | Spec | Depends on | Checks |
|---|---|---|---|---|
| T1 Spike + crate skeleton | `crates/hashlark-torrent/{Cargo.toml,src/lib.rs,tests/loopback.rs}`, `Cargo.lock` | partial | – | `cargo deny check` passes with librqbit (no new licence outside `deny.toml:48-63`); `cargo ndk -t arm64-v8a check -p hashlark-torrent` builds; `tests/loopback.rs` completes in < 60 s; written answers in the progress log on 7 API points: runtime per-torrent rate limit setter, runtime per-torrent `peer_limit` setter, per-peer stats, tracker status, whether `uploaded_bytes` survives restart, pause while fetching metadata, `create_and_serve_torrent` return type |
| T2 Settings + migration | `crates/hashlark-core/src/settings.rs`, `crates/hashlark-core/migrations/0004_torrents.sql` | full | – | `TorrentSettings` defaults as in §4.1 (Android ratio 0 and peer limits 400/100 by cfg); validate rejects a bad proxy scheme, port, ratio, a schedule that isn't 168 × `n/a/s/o`, or peer limits out of range or global < per-torrent; a `Settings` JSON without `torrents` still loads; the migration test passes |
| T3 Service + operations | `crates/hashlark-torrent/src/{service,ops,store,model,paths}.rs` | partial | T1, T2 | add (magnet/torrent/result_id) → list → details → update files/limits/peer_limit and each `action` (pause, resume, stop, start, force_start) → remove with and without files works in the loopback test; bulk actions work on ids and on all; a duplicate add returns `already: true`; the `PathPolicy` roots tests pass (traversal, symlink); start/stop/apply_settings toggling keeps torrents across a restart (persistence folder) |
| T4 Policies + watch | `crates/hashlark-torrent/src/policy/*.rs`, `src/watch.rs`, the tick hook in `src/service.rs` | full | T3 | queue (run modes, context order), peer_share, seeding, schedule and power unit tests (§10) pass; ratio 1.0 stops seeding in the loopback test (mark done, pause); a grid cell set to Off pauses everything and Alt applies the alternative limits (with an injected clock); `set_power(on_battery)` applies the `on_battery` setting; a watched `.torrent` is added and renamed `.added` |
| T5 Server API + client | `crates/hashlark-server/src/{torrents.rs,lib.rs,api.rs,config.rs,main.rs,tests.rs}`, `docs/deploy/hashlark.example.toml`, `apps/desktop/src/lib/api/{openapi.json,schema.d.ts,client.ts}`, `Dockerfile` | full | T3 | the server tests in §10 pass, including a > 2 MB `torrent` body that reaches validation (400), not 413, and "client off" → 400; `openapi_lists_routes` (`tests.rs:304-322`, an explicit list) names `/torrents*`; `config.rs:58` still parses the example toml; `pnpm api:types` output is committed; `client.ts` has the torrent methods + `torrentEvents()`; `Dockerfile` exposes 6881/tcp+udp and a `/downloads` volume |
| T6 Desktop shell | `apps/desktop/src-tauri/{Cargo.toml,tauri.conf.json,capabilities/default.json,src/main.rs,src/commands.rs,src/magnet_handler.rs,src/lan.rs,src/power.rs}`, `apps/desktop/package.json`, `pnpm-lock.yaml` | partial | T5 | `cargo clippy -p hashlark-desktop` is clean; `cargo deny check` accepts `keepawake` (MIT), `system_shutdown` ("MIT/Apache-2.0") and `starship-battery` (ISC), or a `deny.toml` clarification is added; `pnpm tauri info` shows matching plugin crate/npm versions; the keep-awake guard is held only while downloading (log line); "When all done → Quit" with a 5 s test override fires and Cancel stops it; a second launch with a `magnet:` arg emits `torrent-open`; a handler that is Hashlark reports `is_self` and `hand_off_magnet` returns NoHandler; closing the window with `close_to_tray` hides it |
| T7 Downloads page | `apps/desktop/src/routes/downloads/+page.svelte`, `src/lib/state/torrents.svelte.ts`, `src/lib/components/{TorrentList,TorrentDetails,AddTorrentDialog,CreateTorrentDialog,DeleteTorrentDialog,WhenDoneDialog,TorrentNotice}.svelte`, `Sidebar.svelte`, `routes/+layout.svelte` | partial | T5, T6 | `pnpm check` and `pnpm test` pass; the snapshot reducer, formatter, filter-count and delete-dialog tests pass; the page renders a live list from SSE in `pnpm tauri dev` and in the server web UI (screenshots); the 8 filter tabs show counts; dragging in the Queued filter reorders; Pause, Stop, Force start and Delete (with and without files) work from row and multi-select; keyboard path: arrows, Space, Shift+S, Ctrl+↑/↓, Delete, Enter; the when-done countdown shows and Cancel works |
| T8 Results + settings UI | `apps/desktop/src/lib/actions.ts`, `ResultsList.svelte`, `ResultDetails.svelte`, `NoHandlerDialog.svelte`, `TorrentSettingsPanel.svelte`, `ScheduleGrid.svelte`, `routes/settings/+page.svelte` | full | T7 | the primary-action test (on/off/Shift) passes; turning the client off shows "Open in client" and hands off; NoHandler shows "Turn on the built-in client" when off; the schedule grid conversion and drag-paint tests pass; the grid can be used with the keyboard (arrows, 1–4); the peer-limit and power settings save and reload |
| T9 FFI + Kotlin API | `crates/hashlark-ffi/{Cargo.toml,src/lib.rs,src/torrents.rs}`, `apps/android/core/.../{HashlarkApi.kt,TorrentModels.kt,Models.kt}`, `app/src/test/.../FakeApi.kt` | full | T4 | `cargo test -p hashlark-ffi` passes (the `set_settings` tests at `tests.rs:147-191` are untouched); `ModelsTest` decodes the torrent JSON and round-trips `Settings` with a non-default `torrents` section; `FakeApi` compiles with stubs; the arm64 `libhashlark_ffi.so` size before/after goes in the progress log, with the per-ABI APK ≤ 25 MB (`docs/android-plan.md:195`) |
| T10 Android service + intents | `apps/android/app/src/main/AndroidManifest.xml`, `system/{TorrentService,TorrentStorage,NetworkGate,PowerGate}.kt`, `system/MagnetLauncher.kt`, `IntentRouter.kt`, `MainActivity.kt`, `res/xml/file_paths.xml` | partial | T9 | `./gradlew lint testDebugUnitTest assembleDebug -Phashlark.abis=x86_64` passes; `IntentRouterTest` covers magnet + `.torrent`; on the emulator, a magnet from a browser intent adds a torrent, the service notification shows progress, and switching the emulator to metered pauses with "Waiting for Wi-Fi"; `adb shell dumpsys battery unplug` + `set level 10` switches to alternative speed (or pauses, per setting); the wake and Wi-Fi locks are released when idle (`dumpsys power`); `handlers()` excludes our package |
| T11 Android UI | `ui/Destination.kt`, `ui/HashlarkRoot.kt`, `ui/downloads/*.kt`, `ui/ResultActions.kt`, `ui/settings/{SettingsScreen,TorrentSettings}.kt`, `ui/library/LibraryScreens.kt` (History tab), the LAN receive path (magnets go to the built-in client when on), tests incl. `SearchHarness.kt` | partial | T10 | `LayoutMatrixTest`/`WindowShapeTest`/`SearchHarness` and `IntentRouterTest` pass; the bottom bar has 5 items, and rail/drawer have 6; the History shortcut opens the History tab on a bar layout; a long-press on a result opens the Add sheet; filter chips with counts, multi-select actions (Pause, Stop, Force start, Delete with or without files) and queue reorder work; the schedule editor saves the grid; Downloads shows live progress on the emulator (screenshots: phone + unfolded Fold7 width) |
| T12 Docs | `docs/adr/0017-built-in-torrent-client.md`, `docs/adr/README.md` (a new 0017 row; the 0008 and 0010 rows note the amendment), `docs/adr/0010-no-telemetry.md:12`, `docs/PLAN.md` (`:75`, `:93`, `:509-513`, `:667`, `:698`), `docs/user-guide.md` (`:3`, `:12`, `:27`, a new "Downloading" section, Privacy `:129`), `README.md`, `docs/deploy/README.md`, `CHANGELOG.md` | full | T6, T8, T11 | ADR 0017 amends 0008 and adds torrent traffic to ADR 0010's list of outgoing requests; the user guide covers adding, Pause vs Stop vs Force start, deleting with files, the filters and queue, seeding, connection limits, the schedule grid, the power options (keep awake, when all done, on battery), the handler, the watched folder, creating torrents, the Android Wi-Fi rule and the known gaps; the CHANGELOG `[Unreleased] ### Added` entry follows the existing format |

Order:
- T1 and T2 can run in parallel.
- T3 comes next.
- After T3, T4 and T5 can run in parallel.
- T6 → T7 → T8 run in sequence after T5.
- T9 (after T4) → T10 → T11 can run in parallel with T6–T8.
- T12 is last.

Run before the PR:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --exclude hashlark-desktop`
- `cargo deny check`
- in `apps/desktop`: `pnpm check && pnpm test && pnpm build`
- in `apps/android`: `./gradlew lint testDebugUnitTest assembleDebug -Phashlark.abis=x86_64`

## 12. Risks and known gaps

- **librqbit API gaps.** The plan assumes a runtime per-torrent rate limit, per-peer stats and tracker status. T1
  confirms each one. Fallbacks:
  - per-torrent limit change → re-add with the same options (fastresume, no rehash);
  - peers/trackers → hide the tab rather than invent data.
- **No protocol encryption (MSE/PE).** Some ISPs throttle, and some private trackers require it. The user guide states
  this.
- **Android storage over FUSE.** librqbit writes with positional writes into `Download/` through FUSE. T10 measures
  throughput and falls back to the app-specific folder if writes fail.
- **The Android `specialUse` service is fine for APKs we ship (GitHub/F-Droid).** Google Play would need a
  declaration. Other OEM battery savers (Samsung) may still kill the service; the user guide links to the battery
  setting.
- **Windows default apps.** A `UserChoice` set to another client overrides our `HKCU` registration. The Settings
  button then opens Windows "Default apps" with a hint. macOS requires the user to choose Hashlark in another client
  or Finder.
- **Privacy.** Peers see the user's IP, and trackers and DHT bootstrap use system DNS, not DoH. The SOCKS5 proxy
  covers TCP peers and trackers only. The notice, the settings and the user guide all say this.
- **Server exposure.** Torrent routes sit behind the same API keys (`crates/hashlark-server/src/auth.rs:125`). Path
  roots stop file reads and writes outside the allowed folders. The listen port must be published for incoming peers.
- **The global connection cap is approximate.** librqbit has no session-wide cap, so it is shared per torrent (§2). If
  T1 finds no runtime `peer_limit` setter, a running torrent keeps its old share until its next start. Incoming
  connections may briefly exceed the cap.
- **Power actions need OS permission.**
  - Linux shutdown or sleep goes through logind and may ask polkit.
  - macOS uses System Events, which prompts the first time.
  - A failure shows a toast and does nothing; it never retries.
  - Hibernate may be off in Windows (`powercfg /h`), in which case `system_shutdown` returns an error, shown as a toast.
- **Binary size.** librqbit adds roughly 3–5 MB (estimate) to each binary; T9 measures the arm64 build against the
  25 MB APK budget.
- **ADR change.** This reverses the D8 non-goal (`docs/PLAN.md:75,93`). ADR 0017 records it, and "hands off to your
  client" text in the README and user guide changes. Code comments that cite ADR 0008 get updated by the task that
  touches each file:
  - `apps/desktop/src-tauri/src/commands.rs:39` (T6);
  - `apps/desktop/src/lib/actions.ts:12` (T8);
  - `system/MagnetLauncher.kt:14` (T10);
  - `ui/ResultActions.kt:54` (T11).

## 13. Progress log
- 2026-11-10: plan written and approved.
- 2026-11-10: built with /runner (T1–T12 on `plan/torrent-client`).
  - T1 librqbit 9.1 answers:
    1. A per-torrent rate limit can only be set at add time. Changing it re-adds the torrent, copying the fastresume
       file across so there is no re-check. Session-wide limits can be changed at runtime.
    2. `peer_limit` can only be set at add time. A new value applies on re-add or on the next start.
    3. Per-peer stats are available (`per_peer_stats_snapshot`).
    4. There is no tracker status, only the URL list. The UI lists URLs without a status.
    5. `uploaded_bytes` resets on every pause and restart, so Hashlark keeps the cumulative totals in its own table.
    6. A magnet can't be paused while it fetches metadata. Hashlark holds those adds as its own cancellable "pending
       metadata" entries.
    7. `create_and_serve_torrent` returns `(CreateTorrentResult { meta, output_folder }, handle)`.
  - `cargo deny` ignores RUSTSEC-2026-0293: ringbuf, reached through librqbit-utp, only stores `u8`.
  - The loopback transfer of 3 MiB takes 27–53 ms. The 13 loopback tests take about 11 s.
  - T9 Android library size (arm64, stripped): 24.1 MB before, 28.3 MB with librqbit. With `opt-level = "s"` in the
    android profile (the user's choice) it is 23.6 MB, under the 25 MB budget.
  - T10 emulator (API 36):
    - A browser magnet was added and the FGS notification showed progress.
    - Switching to a metered network showed "Waiting for Wi-Fi".
    - Battery unplugged at 10% switched to alternative speed.
    - The wake and Wi-Fi locks were released when idle.
    - FUSE writes to `Download/` ran at 76 MB/s, so storage is not the bottleneck.
  - T11: no emulator run, because the disk was too full for the AVD. The screenshots are Robolectric renders.
  - Calls made during the run:
    - Paused unfinished torrents block "When all done"; stopped ones don't.
    - On Android, a long-press on a result row keeps the existing menu, which also has "Send to computer". The menu gains
      "Download with options…", and a long-press on Download in the details opens the Add sheet.
    - With no preferred app, the Android hand-off picks the first other client rather than showing a chooser.
    - Deleting a created torrent never deletes its source files.
  - Final checks all pass: fmt, clippy with `-D warnings`, workspace tests, desktop tests, defs lint, `cargo deny`, pnpm
    check, test and build, and Android lint, unit tests and assemble.
