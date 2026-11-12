# Torrent trackers editing, reannounce and force recheck

Approved by the user (2026-11-12: "approved").

The user asked (2026-11-12) for a way to edit the trackers of torrents in the built-in client, plus Reannounce and Force
recheck actions. Extends `docs/torrent-client-plan.md`.

## 1. Why
- **Trackers are read-only.** The Trackers tab lists URLs only: desktop `TorrentDetails.svelte:361-371`, Android
  `TorrentDetailSheet.kt:277-287`. A dead tracker can't be dropped and a new one can't be added.
- **No reannounce, no recheck.** `TorrentAction` has only Pause/Resume/Stop/Start/ForceStart
  (`crates/hashlark-torrent/src/model.rs:245-251`) and `BulkAction` adds only Remove/Turtle (`model.rs:305-319`). After
  moving or repairing files there is no way to re-verify them, and no way to ask trackers for peers now.
- **librqbit 9.1 has none of the three as an API** (`Cargo.lock:4546`):
  - trackers are a plain `HashSet<Url>` on `ManagedTorrentShared`, fixed at add (`librqbit-9.1.0/src/torrent_state/mod.rs:182-186`);
    `AddTorrentOptions.trackers` only *extends* the .torrent's own list (`session.rs:1150-1164`);
  - announcing starts in `make_peer_rx`, called on every `unpause` (`session.rs:1542-1594`, `:1618-1629`); no reannounce fn;
  - the full piece check (`file_ops.rs:73 initial_check`) runs only during initializing when fastresume is missing or
    invalid (`torrent_state/initializing.rs:86-101,216-223`); no recheck fn.
- **Hashlark already re-adds torrents** to change add-time options, keeping the fastresume `.bitv` so nothing is
  re-checked: `Service::readd` (`crates/hashlark-torrent/src/service/apply.rs:280`), called from `ops.rs:130` and
  `service.rs:537`. All three features build on it.

## 2. Decisions
User (2026-11-12):
- **Full tracker editing per torrent:** add (paste many, one per line), edit a URL, remove. Desktop/web and Android.
- **Reannounce and Force recheck work on one torrent and on many selected.**
- **Force recheck asks for confirmation first.**
- **No bulk tracker editing** (adding a tracker list to many torrents) in this plan.

My calls (change any at approval):
- **Reannounce and recheck are `BulkAction` variants only, no new routes.** Picked: `BulkAction::Reannounce`,
  `BulkAction::Recheck`; a single torrent sends `{ ids: [id], action }`. Rejected: `TorrentAction` variants
  (`TorrentAction::run_mode` maps every variant to a run mode, `model.rs:253-261`; these are one-shot, not modes) and new
  `POST /torrents/{id}/recheck` routes (the desktop row menu, the Android selection bar and the FFI `torrent_bulk`
  already speak bulk, so no FFI or route change).
- **Tracker edits go through `TorrentPatch.trackers: Option<Vec<String>>`, replacing the whole list.** Rejected: add/remove
  deltas (the UI holds the full list anyway; one shape is simpler to validate).
- **Edits are staged in the UI and saved with a Save button**, Discard to revert. Each save re-adds the torrent (peers
  drop for a moment), so per-keystroke saving is wrong.
- **Validation:** schemes `http`, `https`, `udp`, `ws`, `wss`; at most 100 URLs; each at most 2048 bytes; trimmed;
  duplicates dropped (order kept); an empty list is allowed (DHT/PEX only). A bad URL fails the whole patch with
  `Error::Invalid` naming it, before anything changes (same rule as `ops.rs:54` "check everything before changing").
- **Tracker tiers are flattened to one list**, in the user's order. librqbit keeps an unordered `HashSet<Url>`
  (`torrent_state/mod.rs:186`) and `details()` sorts it (`ops/info.rs:221-227`); the order lives only in the torrent
  bytes' announce list (`librqbit-core-9.1.0/src/torrent_metainfo.rs:55`). After this change `details()` orders by that
  list (see §3.2).
- **Bencode via a direct dependency on `librqbit-bencode = "9.1.0"`**, already in `Cargo.lock:4610` as a transitive dep,
  so no new crate or licence. Rejected: a hand-written bencoder (more code to test for no gain). librqbit doesn't
  re-export it (`librqbit-9.1.0/src/lib.rs:85-109`).
- **Recheck confirm reuses the modal, not the delete dialog:** a new small `RecheckTorrentDialog.svelte` wrapping
  `Modal.svelte` like `DeleteTorrentDialog.svelte:29-57` does. Rejected: generalising the delete dialog (its props and
  checkbox are delete-specific, `:6-11,43-46`).
- **Reannounce = librqbit pause + unpause** of the live handle, with `capture()` first so upload totals survive
  (`apply.rs:148`; uploaded bytes reset on pause, `docs/torrent-client-plan.md:741`). Only for torrents that are live and
  not paused (Downloading, Seeding, Checking excluded); others are reported as failed in `BulkResult` with "not running".
- **Recheck = re-add without the `.bitv`.** It works in any state that has metadata (not `Metadata`). The torrent keeps
  its run mode: an auto/forced one resumes after the check, a paused/stopped one stays paused with fresh progress.
- **Not allowed while the torrent is in `Metadata` state** (no info bytes yet) for trackers, reannounce and recheck: the
  UI disables the controls, the engine returns `Error::Invalid`.
- **Magnet file kept in step:** after a tracker edit, `<data>/torrents/magnets/<hash>.magnet` (`add.rs:204-207`) is
  rewritten with the new `tr=` list and its other params kept, so "Copy magnet" shows the new trackers.

## 3. Engine (`crates/hashlark-torrent`)
### 3.1 Model (`src/model.rs`)
- `TorrentPatch` (`:288-299`) gains `pub trackers: Option<Vec<String>>` with a doc line: "Replaces the tracker list; an
  empty list leaves DHT and PEX only."
- `BulkAction` (`:305-319`) gains `Reannounce` and `Recheck` (serde `reannounce` / `recheck`, snake_case like the rest).
- `TorrentDetails.trackers` doc (`:179-180`) stays "no per-tracker status".

### 3.2 Re-add with options (`src/service/apply.rs`)
Generalise `readd` (`:280`) into `readd_with(run, hash, previous, ReaddOpts { torrent_bytes: Option<Bytes>,
keep_fastresume: bool })`; `readd` stays as a thin wrapper (`torrent_bytes: None, keep_fastresume: true`) so
`ops.rs:130` and `service.rs:537` don't change.
- `torrent_bytes: Some(b)` is added instead of the handle's current `torrent_bytes` (`:286-292`), and the fallback
  re-add with `previous` (`:333-`) uses the *old* bytes, so a failed tracker edit restores the old list.
- `keep_fastresume: false` deletes `<persistence_dir>/<hash>.bitv` instead of restoring it (`:303-317`), so librqbit runs
  `initial_check`.
- Options passed on re-add must not set `AddTorrentOptions.trackers` (it would extend the list again).
- `details()` (`ops/info.rs:221-227`): keep reading `handle.shared().trackers`, but order them by the torrent's own
  announce list (from `metadata.torrent_bytes`) and drop the `.sort()`, so the UI shows the user's order. If reading the
  order fails, fall back to the sorted set.
- `apply.rs` is 354 lines; if `readd_with` pushes it past ~400, move the re-add code to `src/service/readd.rs`.

### 3.3 New ops (`src/ops/maintain.rs`, new; `src/ops.rs` declares it)
- `fn tracker_list(raw: &[String]) -> Result<Vec<Url>>` — validation from §2.
- `fn torrent_with_trackers(info_bytes: &[u8], trackers: &[Url]) -> Result<Bytes>` — bencode `{ announce: first or "",
  "announce-list": [[...all]], info: <raw info bytes> }`, the same shape as librqbit's private
  `torrent_file_from_info_bytes` (`librqbit-9.1.0/src/session.rs:521-538`), serialised with `librqbit-bencode`
  (§2; add it to `crates/hashlark-torrent/Cargo.toml`). Info bytes come from the handle's metadata
  (`TorrentMetadata.info_bytes`, pub, `torrent_state/mod.rs:138-141`). The info hash must not change (test).
- `fn magnet_with_trackers(magnet: &str, trackers: &[Url]) -> String` — replaces all `tr=` params, keeps the rest.
- `pub(crate) async fn set_trackers(&self, run, id, raw)` — called from `update()` (`ops.rs:39`) when
  `patch.trackers.is_some()`, under the same `op_lock`, after all other patch checks: validate → read info bytes from
  the handle's metadata (`Error::Invalid("the torrent's metadata isn't known yet")` if none) → `readd_with(bytes, keep
  fastresume)` → rewrite the magnet file if it exists. When the patch also changes limits, do one re-add, not two.
- `pub(crate) async fn reannounce(&self, id) -> Result<()>` — under `op_lock`: handle must exist, have metadata, and
  not be paused → `capture()` → `session.pause(&handle)` → `session.unpause(&handle)`.
- `pub(crate) async fn recheck(&self, id) -> Result<()>` — under `op_lock`: handle with metadata → `readd_with(None,
  keep_fastresume: false)`; then the next reconcile applies the run mode (the `readd` doc already says so, `apply.rs:275-276`).
- `bulk()` (`ops.rs:228-262`) routes the two new variants to these fns, collecting per-id failures like Remove does.
  **Route them above the catch-all `_ => ForceStart` (`ops.rs:259`)**, and better replace that catch-all with explicit
  arms so a future variant can't silently force-start. Extend the bulk JSON test (`ops.rs:303-307`).
- Recheck progress: during the check librqbit reports `progress_bytes = checked_bytes` (`torrent_state/mod.rs:525-527`),
  so the existing `Checking` row shows a moving percentage; a torrent added paused still runs the check and ends
  `Paused` (`torrent_state/mod.rs:357-396,415-418`).

### 3.4 API schema
- The desktop types are generated: `pnpm api:types` (`apps/desktop/package.json:15`) runs `hashlark-server --openapi`
  into `apps/desktop/src/lib/api/openapi.json` and `openapi-typescript` into `src/lib/api/schema.d.ts` (`BulkAction`
  `:691`, `TorrentPatch` `:1576`). T1 regenerates and commits both, since `crates/hashlark-server/src/tests.rs` reads
  `openapi.json`.

## 4. Desktop and web UI (`apps/desktop`)
Strings are inline in components (no i18n layer).
- Types come from the regenerated `schema.d.ts` (§3.4, done in T1); `client.ts` (`:32,38` re-exports,
  `:341-365` methods) needs no type edits.
- `src/lib/torrents.ts`: the `bulk` helper (`:266-268`) and `patch()` (`:270-275`) accept the new action and the
  `trackers` field; extend `torrents.test.ts` (`:177-178,189-190`).
- `src/lib/state/torrents.svelte.ts`: `RowAction` (`:41`) gains `reannounce` and `recheck`; `act()` (`:304-306`) sends
  them through `bulkTorrents`; a `setTrackers(id, list)` that PATCHes and refreshes details. Toast the `BulkResult`
  failures the same way existing bulk actions do.
- `routes/downloads/+page.svelte` opens the recheck confirm like it opens the delete dialog (`:110-114,364`).
- `src/lib/components/TorrentList.svelte` row menu (`:148-192`, acts on the selection, `:149`): add "Reannounce" and
  "Force recheck…" after "Force start". Recheck opens `RecheckTorrentDialog.svelte` (new, §2):
  title "Force recheck", body "Re-verify all downloaded data of N torrent(s)? Downloading waits until the check
  finishes.", buttons Recheck / Cancel. Both items disabled when every selected torrent is in `metadata` state;
  Reannounce also disabled when none is downloading or seeding.
- `src/lib/components/TorrentDetails.svelte` Trackers tab (`:361-371`): an editable list.
  - Each row: the URL, an Edit button (turns the row into a text input; Enter keeps, Escape cancels) and a Remove button
    (aria-label "Remove tracker <url>").
  - Below: a textarea "Add trackers (one per line)" and an Add button; blank lines ignored; client-side check of the
    scheme so obvious typos show inline before Save.
  - Save and Discard appear when the staged list differs from the torrent's; Save calls `setTrackers`, shows the engine
    error inline on failure and keeps the staged list.
  - Keep the "no per-tracker status" note (`:370`). Add a Reannounce button in the tab header.
  - Whole tab read-only with a note "Trackers can be edited once the metadata is known." in `metadata` state.
- Keyboard: every control is a real button/input in tab order; the list keeps focus on the next row after a remove.

## 5. Android (`apps/android`)
- `ui/downloads/TorrentDetailSheet.kt`: Trackers composable (`:277-287`) becomes editable with the same behaviour as §4
  (row edit/remove, multi-line add field, Save/Discard). Action bar (`:151-159`) gains an overflow with Reannounce and
  Force recheck (confirm `AlertDialog` with the §4 text).
- `ui/downloads/DownloadsScreen.kt` selection action bar (`:351-364`): add an overflow with Reannounce and Force recheck
  (same confirm).
- `apps/android/core/src/main/kotlin/io/github/ashxtrem/hashlark/core/TorrentModels.kt`: `TorrentPatch` mirror
  (`:352-363`) gains `trackers: List<String>? = null`; the custom `BulkAction` serializer (`:366-415`, serialize
  `:384-392`, deserialize `:403-413`) learns `Reannounce` and `Recheck`. Extend `ModelsTest.kt` (`:278-298`).
- `DownloadsViewModel.kt`: bulk through `run()` (`:63-78`) and the trackers patch through the generic `update(id, patch)`
  (`:128-129`), which reach FFI `torrent_bulk` / `update_torrent` (`crates/hashlark-ffi/src/torrents.rs:92-100,123-131`);
  no FFI change. `FakeApi.kt` (`:137,144`) and `HashlarkApi.kt` (`:190-201`) need no change unless their types do.
- Strings inline in Kotlin like the existing torrent labels.

## 6. Tests to change or add
- `src/ops/maintain.rs` unit tests: `tracker_list` (valid schemes, bad scheme, too many, too long, dedupe keeps order,
  empty ok); `torrent_with_trackers` keeps the info hash (parse back with librqbit and compare `info_hash`) and the
  announce list equals the input; `magnet_with_trackers` replaces `tr=` and keeps `dn=`/`xt=`.
- `tests/loopback.rs` (13 tests today, ~11 s): add
  1. patch trackers on a finished torrent → `details().trackers` equals the new list in order, state never `Checking`,
     progress unchanged; the magnet file has the new `tr=` list;
  2. patch with a bad URL → `Invalid`, trackers unchanged;
  3. recheck a finished torrent → passes through `Checking`, ends with full progress;
  4. corrupt one byte of a downloaded file, recheck → progress below 100 %, and an auto torrent re-downloads to 100 %;
  5. recheck a paused torrent → stays paused after the check;
  6. reannounce on a live torrent → Ok and it keeps transferring; on a paused one → reported failed in `BulkResult`.
  Note the loopback session sets `disable_trackers: true` (`loopback.rs:37-42`): trackers are still stored and listed;
  the builder confirms that in test 1 and says so in the log if not.
- `crates/hashlark-server/src/tests.rs`: one test that `PATCH /torrents/{id}` with `trackers` and `POST /torrents/bulk`
  with `recheck` deserialize (routes pinned at `:334-341` stay unchanged).
- `crates/hashlark-server/src/tests/torrent_api.rs` (`:147-157`) bulk test: add a `recheck` case.
- Desktop: vitest for the staged-list helper (add/edit/remove/dedupe/diff) in a `.ts` file; `torrents.test.ts` for the
  new action and field; `pnpm check`.
- Android: `ModelsTest.kt` round-trips `Reannounce`, `Recheck` and `trackers`; `DownloadsUiTest.kt` (`:130-187`, driven
  through `SearchHarness.kt:92`) gets one case that Force recheck asks for confirmation then sends `recheck`.

## 7. Tasks
| Task | Files | Spec | Depends on | Checks |
|---|---|---|---|---|
| T1 Engine: trackers patch, reannounce, recheck | `crates/hashlark-torrent/{Cargo.toml,src/model.rs,src/ops.rs,src/ops/maintain.rs,src/ops/info.rs,src/service/apply.rs}` (maybe `src/service/readd.rs`), `Cargo.lock`, `crates/hashlark-torrent/tests/loopback.rs`, `crates/hashlark-server/src/{tests.rs,tests/torrent_api.rs}`, `apps/desktop/src/lib/api/{openapi.json,schema.d.ts}` | partial | – | §6 unit and loopback tests pass; `cargo test --workspace --exclude hashlark-desktop` passes; `cargo clippy --workspace --all-targets` with `-D warnings` and `cargo fmt --all --check` clean; info hash unchanged after a tracker edit; `bulk()` has no catch-all arm; `openapi.json`/`schema.d.ts` regenerated with `pnpm api:types` |
| T2 Desktop/web UI | `apps/desktop/src/lib/{torrents.ts,torrents.test.ts,state/torrents.svelte.ts}`, `src/lib/components/{TorrentList.svelte,TorrentDetails.svelte,RecheckTorrentDialog.svelte}`, `src/routes/downloads/+page.svelte` (+ a small `.ts` tracker-list helper and its test) | partial | T1 | `pnpm check` and `pnpm test` pass; row menu has Reannounce and Force recheck… with a confirm; Trackers tab edits, saves and discards; controls disabled in `metadata` state |
| T3 Android UI | `apps/android/core/src/main/kotlin/io/github/ashxtrem/hashlark/core/TorrentModels.kt`, `apps/android/core/src/test/.../ModelsTest.kt`, `apps/android/app/src/main/kotlin/io/github/ashxtrem/hashlark/ui/downloads/{TorrentDetailSheet.kt,DownloadsScreen.kt,DownloadsViewModel.kt}`, `apps/android/app/src/test/.../DownloadsUiTest.kt` | partial | T1 | `./gradlew lint testDebugUnitTest assembleDebug -Phashlark.abis=x86_64` passes; `BulkAction` serializer round-trips the new actions; detail sheet and selection bar offer Reannounce and Force recheck with a confirm; Trackers tab edits and saves |
| T4 Docs | `docs/user-guide.md`, `CHANGELOG.md` | full (haiku) | – | user guide (Pause/Stop section `:73-77`, tracker text `:157`) explains editing trackers, Reannounce (peers drop for a moment) and Force recheck (re-verifies data, can take minutes); CHANGELOG "Unreleased" lists the three |

T2, T3 and T4 can run alongside each other once T1 is merged (T4 can start at once).

## 8. Risks and known gaps
- **Re-add drops peers** for each tracker save and reannounce; the transfer resumes within seconds. Documented.
- **Reannounce restarts DHT/tracker peer discovery and peer connections**, not just a tracker announce; that's the
  only path librqbit 9.1 offers. If a future librqbit adds a real reannounce, swap the implementation.
- **No per-tracker status** still (librqbit gives none).
- **Recheck of a big torrent is long**; progress shows in the existing `Checking` state (§3.3). Per-file progress is
  empty while checking (`torrent_state/mod.rs:513`), so the Files tab shows 0 until it ends.
- **`docs/torrent-client-plan.md:736-737` promises re-add never re-checks**; recheck is the deliberate exception
  (`keep_fastresume: false`), and the plain `readd` keeps that promise.
- **librqbit JSON persistence** writes the new torrent bytes on re-add (`session_persistence/json.rs:138-160`); a crash
  between delete and add is covered by `readd`'s existing fallback and error marking.

## 9. Progress log
- 2026-11-12: plan written and approved.
