# Plan: send a result to another device on the LAN

## Context

Today a result can only be opened on the device that found it (ADR 0008: hand
the magnet to the OS handler). The common case is searching on the phone while
the torrent client lives on the PC. Goal: from the phone, find Hashlark
desktops on the same network, pick one, and have the magnet open in that
computer's torrent client. Desktop → phone is a secondary goal.

Decisions already made:

- **Trust:** pair once with a 6-digit PIN shown on the receiver. After that,
  sends from a paired device open immediately. Payloads are encrypted.
- **Android receiving:** only while the app is open (no background service).
- **Receiver still uses the OS handler** (ADR 0008 stays true on each device).

What exists and is reused:

- Desktop opens magnets with `commands::open_magnet` +
  `magnet_handler::registered()` (`apps/desktop/src-tauri/src/commands.rs`,
  `magnet_handler.rs`); the UI shows `NoHandlerDialog` when there is none.
- Android opens magnets with `MagnetLauncher.open(context, magnet, preferredPackage)`
  (`apps/android/app/.../system/MagnetLauncher.kt`), driven from `ResultActions.openMagnet`.
- Both resolve a result to a magnet through the same core call
  (`resolve(id, Magnet)`): desktop via `app.client.resolve`, Android via
  `app.withApi { it.resolve(...) }`.
- Secrets: `SecretStore` trait (`crates/hashlark-core/src/secrets.rs`) — OS
  keychain on desktop, Android Keystore through the FFI.
- Rate limiting: `governor` (already used in `hashlark-server/src/auth.rs`).
- FFI boundary is JSON documents + callback interfaces (ADR 0015,
  `crates/hashlark-ffi/src/lib.rs`).

The existing in-process API (`serve_local`) stays loopback-only. The LAN
listener is a **separate, minimal** server that exposes nothing but the
routes below.

## Design

### New crate `crates/hashlark-lan`

One Rust implementation used by both the desktop (directly) and Android
(through `hashlark-ffi`), so the protocol cannot drift.

| Piece | Choice |
|---|---|
| Discovery | mDNS service `_hashlark._tcp.local.` via `mdns-sd`. TXT: `id`, `v=1`, `platform` (`desktop`/`android`). Plus manual "add by address" (`192.168.1.20:47821`) for networks that block multicast. |
| Listener | `axum` on `0.0.0.0:47821` (falls back to a random port; mDNS advertises the real one). |
| Pairing | SPAKE2 (`spake2` crate) keyed by the PIN, then HKDF-SHA256 → 32-byte device key. A sniffed pairing can't be brute-forced offline. |
| Sends | ChaCha20-Poly1305 with the device key; AAD = sender id. Plaintext `{kind:"magnet", url, title, sent_at, nonce}`. Reject if clock skew > 5 min or nonce seen before. |
| Storage | Own identity under secret `lan/self` (`{id, name}`); each peer under `lan/peer/<id>` (`{name, platform, key, last_addr}`), index under `lan/peers`. No DB migration. |
| Limits | Pairing window 2 min, opened explicitly from the UI; 3 wrong PINs → new PIN. `governor` per-IP limit on all routes. Body size cap 8 KB. |

Routes (all under `/lan/v1`):

- `GET  /hello` → `{id, name, platform, v}` (no auth; used to label manual addresses)
- `POST /pair/start` `{id, name, platform, spake_msg}` → `{spake_msg, session}` — 403 unless the pairing window is open
- `POST /pair/confirm` `{session, mac}` → `{mac}` — both sides store the peer; pairing is mutual, so desktop → phone works afterwards with no second pairing
- `POST /send` `{from, nonce, ciphertext}` → 204; 401 for unknown/forged

Public API (sketch):

```rust
LanService::start(identity_name, platform, secrets, on_event: Arc<dyn LanEvents>) -> LanService
service.devices() -> Vec<Device>            // discovered ∪ paired, with `paired`, `online`
service.add_manual(addr) -> Device
service.open_pairing() -> Pin               // shows on the receiver
service.pair(device_id, pin) -> Device      // runs on the sender
service.forget(device_id)
service.send_magnet(device_id, url, title)
service.stop()
// LanEvents: on_received(Received{from, title, url}), on_devices_changed(), on_paired(Device)
```

Files: `crates/hashlark-lan/{Cargo.toml, src/lib.rs, discovery.rs, pairing.rs,
crypto.rs, server.rs, client.rs, store.rs, tests.rs}`; add to workspace `Cargo.toml`.

### Desktop (receiver first — the main target)

- `apps/desktop/src-tauri/src/lan.rs`: owns the `LanService`; started in
  `start_backend` (`main.rs`) when the setting is on. `on_received` reuses the
  `open_magnet` logic (check `magnet_handler::registered()`, open via opener)
  and emits a Tauri event `lan-received` (`{from, title, outcome}`).
- Tauri commands (registered in `main.rs`): `lan_status`, `lan_set_enabled`,
  `lan_devices`, `lan_add_manual`, `lan_open_pairing`, `lan_pair`,
  `lan_forget`, `lan_send`.
- UI (Tauri-only, hidden in the headless web UI via `isTauri()`):
  - `src/lib/platform.ts`: wrappers for the commands.
  - `src/routes/settings/+page.svelte`: "Devices on this network" — toggle
    (off by default; note that Windows asks to allow it through the firewall),
    this computer's name, **Pair a phone** (shows PIN + countdown), paired list
    with Forget.
  - `+layout.svelte`: listen for `lan-received` → toast "Opened ‘title’ from
    Pixel 9", or open `NoHandlerDialog` when no client is registered.
  - `src/lib/actions.ts`: `sendToDevice(result, deviceId)` — resolve with
    `'magnet'` like `copyMagnet`, then `lan_send`.
  - New `src/lib/components/DevicePicker.svelte`; "Send to…" entry in
    `ResultDetails.svelte` and the result context menu.

### Android (sender first — the main target)

- FFI `crates/hashlark-ffi/src/lan.rs`: `#[uniffi::Object] HashlarkLan` wrapping
  `LanService`, JSON in/out, callback interface `LanListener`
  (`onReceived(json)`, `onDevicesChanged()`, `onPaired(json)`). Reuses the
  Kotlin `SecretStore` already passed to the engine.
- `apps/android/core/.../LanApi.kt`: suspend wrapper + `Flow<List<Device>>`.
- `apps/android/app/.../system/LanController.kt`: tied to
  `ProcessLifecycleOwner` — on start (and setting on) acquire a
  `WifiManager.MulticastLock`, start the service; on stop release and stop.
  `onReceived` → `MagnetLauncher.open(context, url, prefs.magnetPackage)`
  (allowed: the app is in the foreground) + snackbar via `AppViewModel.say`.
- Manifest: `ACCESS_WIFI_STATE`, `CHANGE_WIFI_MULTICAST_STATE`, `ACCESS_NETWORK_STATE`.
  (Rust sockets aren't subject to Android's cleartext-traffic policy.)
- `ui/ResultActions.kt`: `sendToDevice(result)` → resolve magnet → device
  picker. If the chosen device isn't paired, show the PIN dialog first, then send.
- New `ui/lan/DevicePickerSheet.kt` (bottom sheet: online paired devices,
  then "Pair with a computer…" listing unpaired ones, then "Enter address").
  Entry points: `ResultDetails.kt` action row and `ResultRow.kt` long-press menu.
- `ui/settings/SettingsScreen.kt` + `SettingsViewModel.kt`: "Devices" section —
  toggle, device name, paired list, "Allow a computer to pair" (shows PIN).
- `system/AppPrefs.kt`: `lanEnabled` (default on for sending; listening only
  while the app is open).

### Docs

- `docs/adr/0016-lan-device-handoff.md` (protocol, trust model, why a separate
  listener; amends 0008's "OS only" to "OS handler on the receiving device").
- `docs/user-guide.md`: "Send to your computer" section.

## Order of work

1. `hashlark-lan` crate with tests (pairing, crypto, server, client; loopback, discovery off).
2. Desktop receiver + pairing UI in Settings.
3. FFI + Android sender (picker, PIN dialog). **Phone → desktop works end to end here.**
4. Desktop sender + Android receiver (while open). Desktop ↔ phone both ways.
5. ADR + user guide.

Out of scope for v1: `.torrent`-only results (send says "no magnet link",
same message `copyMagnet` shows), background receiving on Android, sending to
the headless server.

## Verification

- `cargo test -p hashlark-lan`: right PIN pairs; wrong PIN fails and 3 misses
  rotate it; pairing refused outside the window; send decrypts and fires
  `on_received`; replayed nonce, skewed clock, unknown sender, tampered
  ciphertext all rejected; per-IP limit returns 429.
- Two-instance integration test: two `LanService`s on 127.0.0.1 with manual
  addresses pair and send in both directions.
- `cargo test -p hashlark-ffi` (JSON shapes), `cargo clippy --workspace`.
- `cd apps/desktop && pnpm check && pnpm test` (`sendToDevice` with mocked `invoke`).
- `cd apps/android && ./gradlew test lint` (LanController state, picker logic).
- Manual end to end: `pnpm tauri dev` on the PC and a debug APK on a phone on
  the same Wi-Fi → Settings → Pair a phone → phone: Send to… → enter PIN →
  magnet opens in qBittorrent on the PC. Then send from the PC with the phone
  app open → opens in the phone's torrent app. Repeat with mDNS blocked using
  "Enter address".
