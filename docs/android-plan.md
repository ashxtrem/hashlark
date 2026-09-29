# Hashlark for Android — Plan

> Detailed plan for Phase 2 of [PLAN.md](PLAN.md#11-android-app-phase-2). It keeps the decision in [ADR 0003](adr/0003-standalone-android-uniffi.md) (standalone app, Rust core through UniFFI, Kotlin + Jetpack Compose) and adds what that section left open: adaptive layouts for every screen shape (portrait, landscape, foldables — mainly the **Galaxy Z Fold7**), Android-only features, distribution and a revised milestone list.

| | |
|---|---|
| Status | Implemented (A0 to A5), 2026-09-29. Decisions in [§11](#11-decisions); what changed while building it in [§12](#12-implementation-notes) |
| Replaces | Milestones A1–A3 in PLAN.md §17 (see [§9](#9-milestones)) |
| Stack | Kotlin 2.x, Jetpack Compose + Material 3 + Material 3 Adaptive, Jetpack WindowManager, WorkManager, UniFFI, `cargo-ndk` |
| SDK levels | `minSdk 26` (Android 8.0), `targetSdk 36` (Android 16) or newer. See [§5.4](#54-minimum-android-version) |
| Distribution | **GitHub Releases only**: signed release APKs ([§8](#8-distribution-and-signing)) |
| Tor | **Built in** (Arti) from v1 |
| Test devices | Galaxy Z Fold7 (primary), a regular phone, Android Studio's resizable and foldable emulators, an Android 8/9 emulator |

---

## 1. Why Compose and not Tauri's Android target

Tauri 2 can build Android apps, which would reuse the Svelte UI. We still stay with ADR 0003:

- **Foldable support needs native APIs.** Hinge position and posture come from Jetpack WindowManager (`FoldingFeature`). Android WebView doesn't expose them reliably to CSS/JS, so a WebView UI can't do tabletop or book layouts properly.
- **Material 3 Adaptive does the hard part for us.** `NavigationSuiteScaffold` and `ListDetailPaneScaffold` switch between bottom bar / rail and one / two panes, and keep panes off the hinge.
- **Android integrations are first class in Kotlin:** share targets, `PROCESS_TEXT`, WorkManager, Keystore, MediaStore, drag and drop between apps, the challenge `WebView` and `CookieManager`.

The cost is writing the screens a second time. The screens are small (about 2,500 lines of Svelte today) and all logic already lives in the Rust core, so the Kotlin side is mostly layout.

---

## 2. Screen shapes we have to handle

### 2.1 The Galaxy Z Fold7

| Display | Resolution | Shape | Typical window size class* |
|---|---|---|---|
| Cover, portrait | 1080 × 2520 | 21:9, tall and narrow | Compact width, Expanded height |
| Cover, landscape | 2520 × 1080 | 21:9, short and wide | Expanded / Medium width, **Compact height** |
| Inner, unfolded | 1968 × 2184 | about 1.1:1, almost square | Medium or Expanded width in either orientation |
| Inner, half-open, hinge horizontal | — | **Tabletop** posture | Two halves stacked |
| Inner, half-open, hinge vertical | — | **Book** posture | Two halves side by side |
| Split screen, pop-up window, DeX / desktop windowing | any | any | anything down to very small |

\* The exact dp values change with the user's display-size setting, so **layouts are chosen by window size class and posture, never by device model or resolution.** Code that works for the Fold7 then also works for the Pixel Fold, tablets, Chromebooks and split screen.

### 2.2 Rules that apply everywhere

1. **Never lock orientation or aspect ratio.** No `screenOrientation`, no `resizeableActivity="false"`, no `maxAspectRatio`. From Android 16 (targetSdk 36) the system ignores these on large screens anyway, and the Fold7's inner display counts as large.
2. **No state lost on rotate, fold or unfold.** The engine lives in `Application` scope; the running search, results and scroll position live in `ViewModel`s and `rememberSaveable`. Folding the phone mid-search keeps streaming results onto the cover screen.
3. **Edge-to-edge** (enforced from targetSdk 35): handle system bar, display cutout and IME insets explicitly.
4. **Compact height is a first-class case.** Cover screen in landscape with the keyboard open leaves very little room. The search field must stay visible and results must still scroll.
5. **Support a single declared activity** with Compose navigation, so multi-window and continuity work without extra activities (the challenge WebView is the one exception).

---

## 3. Adaptive layouts

### 3.1 Building blocks

| Piece | Library | Use |
|---|---|---|
| `currentWindowAdaptiveInfo()` | `material3-adaptive` | Window size class + posture in one value |
| `NavigationSuiteScaffold` | `material3-adaptive-navigation-suite` | Bottom bar (compact) → navigation rail (medium/expanded) → drawer (large) |
| `ListDetailPaneScaffold` | `material3-adaptive-layout` | Results list + result details; keeps panes off a separating hinge |
| `SupportingPaneScaffold` | `material3-adaptive-layout` | Filters / provider status as a side pane on wide windows |
| `WindowInfoTracker` / `FoldingFeature` | `androidx.window` | Tabletop vs book posture, hinge bounds, `isSeparating` |

### 3.2 Layout per shape

| Shape | Navigation | Search screen | Details |
|---|---|---|---|
| **Cover portrait** (compact width) | Bottom bar | Search bar at top, filters as chips + bottom sheet, 2-line result cards | Full-screen, back returns to list with scroll position kept |
| **Cover landscape** (compact height) | Navigation rail | Search bar collapses on scroll, single-line rows, filter sheet slides from the side | Full-screen |
| **Inner portrait / landscape** (Fold7: about 750 × 832 dp or 832 × 750 dp) | Rail | Results use the full width (a table when wide enough: size, seeds, peers, age, sources). No forced split | Full-screen; navigation hidden, Back returns to the same list position |
| **Tablets, DeX, wide windows** (from 840 dp, if both panes fit) | Rail, sidebar from 1200 dp | Results table, then results beside details after a result is opened; filters in a dialog | Right pane with a Close button; no empty pane before selection |\| results table \| details | Right pane |
| **Tabletop** (half-open, hinge horizontal) | Hidden while in posture | Top half: results (or details). Bottom half: search field, filter chips and the keyboard | Top half |
| **Book** (half-open, hinge vertical) | Rail | Results on the left of the hinge, details on the right | Right of hinge |
| **Split screen / small pop-up** | Adapts like the rows above | Must not crash or clip at very small sizes; falls back to the cover-portrait layout | Full-screen |

### 3.3 Results list at different widths

The same `ResultRow` composable has three densities picked from available width (not the window class, since it may sit inside a pane):

- **Narrow:** filename (2 lines), then labelled facts: size, `Seeds n` (or `Seeds unknown`, never a bare dash for missing counts), source count; age and provider on a third line when width and text scale allow. One save (star) action; opening the result is a tap, and the primary "Open in client" action, copy, share, `.torrent` and source links are in the details (and in the long-press menu and TalkBack actions).
- **Medium:** the same with age and provider always shown.
- **Wide:** a table with sortable column headers, like the desktop `ResultsList`.

### 3.4 Other screens

| Screen | Compact | Wider |
|---|---|---|
| Providers | List; tap opens settings full-screen | List-detail (list + provider settings, health, test results) |
| Settings | Single column of sections | Section list + section content |
| History / Favourites | List | List-detail (entry + its results) |
| Repositories | List with add button | List-detail |

---

## 4. Features

### 4.1 Parity with desktop (v1)

- Streaming search across all enabled providers, with the provider status bar (started / finished / failed / health).
- Filters: categories, providers, sort; pagination.
- Result details: sources, files and trackers where known, infohash.
- Magnet handoff: `ACTION_VIEW` on the magnet URI; fall back to copy / share when no app handles it.
- Save `.torrent`: straight into **Downloads** through MediaStore on Android 10+, or a "Save as" picker (Storage Access Framework) on Android 8–9 and whenever the user wants another folder. Then offer "Open with".
- Providers: enable/disable, health, test, per-provider network policy (DoH, proxy, Tor), Torznab add, import `.yml`.
- Definition repositories: add, sync, remove; trust-on-first-use key prompt.
- History, favourites, first-run legal notice, light/dark theme.
- Network settings: DoH choice, proxy, **built-in Tor (Arti)**, and external Tor (Orbot on `127.0.0.1:9050`) as an alternative.
- Browser-check flow: an in-app `WebView` activity using the same user-agent as the core; read cookies with `CookieManager` and send them to the core's session API.
- Update notice: checks the latest **published** GitHub release (same endpoint family as desktop, can be turned off in Settings, as PLAN.md §14 already allows). It shows "Version X is available" and opens the release page; the app doesn't install APKs itself, so it needs no `REQUEST_INSTALL_PACKAGES` permission.

**Not on Android in v1:** API keys (server-only) and the click-to-pick definition editor (desktop-first; maybe later on large screens).

### 4.2 Android-only features

**v1 (cheap and high value)**

| Feature | How |
|---|---|
| Search from anywhere | Share target (`ACTION_SEND`, `text/plain`) and text-selection menu (`ACTION_PROCESS_TEXT`, "Search in Hashlark") |
| IMDb links → exact search | Shared IMDb URLs are parsed into `SearchQuery.imdb_id` (already in the core) |
| Add a definition or repo from a link | Intent filters for `.yml` files and a `hashlark://repo?url=…` deep link, both with a confirmation screen |
| App shortcuts | Long-press icon: *New search*, *Favourites*, *History*; dynamic shortcuts for the last three searches |
| Background repo sync | WorkManager, every 24 h, only on unmetered network and not low battery (constraints replace the core's own timer on Android) |
| Material You | Dynamic colour on Android 12+, falling back to the Hashlark palette; follows system dark mode on Android 10+, in-app theme switch on all versions |
| Predictive back | Enabled (default for targetSdk 36; animations on Android 14+) between list and details |
| Pick the torrent client | Show installed magnet handlers and remember the choice (needs a `<queries>` entry for the `magnet:` scheme because of Android 11+ package visibility) |

**v1 foldable and large-screen extras**

| Feature | How |
|---|---|
| Drag a result into another app | Long-press a result on the inner screen in split screen and drag it into the torrent client (magnet as `ClipData` text) |
| Keyboard, mouse, trackpad | `Ctrl+F` / `/` focus search, arrow keys move through results, `Enter` opens, `Ctrl+C` copies the magnet; hover states and right-click context menus |
| Multi-window | Works in split screen and pop-up view at any size; `android:resizeableActivity="true"` declared explicitly |

**Later (v1.x)**

| Feature | Notes |
|---|---|
| Home-screen widget | Glance widget with a search box and recent searches |
| App lock | Optional biometric lock and "hide content in recents" (`FLAG_SECURE`) for privacy |
| Remote mode | Use a headless server instead of the embedded core (already listed in PLAN.md Phase 3); the OpenAPI spec can generate the Kotlin client |
| Definition editor on large screens | Reuse the desktop picker idea in a WebView on the inner display |
| Tor bridges | obfs4 / Snowflake for networks that block Tor, once desktop has them (PLAN.md follow-ups) |

---

## 5. Architecture

```
┌────────────────────────── apps/android ──────────────────────────┐
│ :app        Compose UI, navigation, ViewModels, adaptive layouts  │
│ :core       Kotlin wrapper: Engine singleton, Flow<SearchEvent>,  │
│             SecretStore (Keystore), WorkManager workers           │
│ :ffi        generated UniFFI bindings + jniLibs/*.so (AAR)        │
└──────────────────────────────┬────────────────────────────────────┘
                               │ JNI (UniFFI, via JNA)
┌──────────────────────────────▼────────────────────────────────────┐
│ crates/hashlark-ffi   Engine object, async fns, SearchListener,   │
│                       SecretStore foreign trait, FFI records      │
└──────────────────────────────┬────────────────────────────────────┘
                               │
                     crates/hashlark-core (unchanged API, a few additions)
```

### 5.1 `hashlark-ffi` crate

- UniFFI proc-macro mode; async exports run on one shared Tokio multi-thread runtime.
- One `HashlarkEngine` object wrapping `Engine`. Methods mirror the HTTP API: `search`, `resolve`, `providers`, `update_provider`, `add_torznab`, `add_definition`, repos, history, favourites, settings, `set_session`, `tor_status`.
- **Streaming:** `search(query, listener) -> SearchHandle`. The listener is a callback interface (`on_results`, `on_provider_status`, `on_done`); `SearchHandle.cancel()` triggers the `CancellationToken`. The Kotlin wrapper turns this into a cold `Flow` that cancels when collection stops.
- **Types:** FFI records live in `hashlark-ffi` (or use UniFFI's remote-type support) with `From` conversions. This keeps UniFFI's generated `unsafe` code out of `hashlark-core`, where the workspace forbids it; only `hashlark-ffi` allows `unsafe_code`.
- **Errors:** one flat `HashlarkError` enum (kind + message) mapped from `hashlark_core::Error`.

### 5.2 Core changes needed

| Area | Today | Android change |
|---|---|---|
| Data dir | `directories` crate, which has no Android support | Pass `context.filesDir` in via `AppPaths::at` (already exists) |
| Secrets | `keyring` behind the `keychain` feature | Build with `default-features = false`; Kotlin implements `SecretStore` with the Android Keystore (AES-GCM key) + DataStore. **Don't use `EncryptedSharedPreferences`** (mentioned in PLAN.md §11); it is deprecated |
| TLS roots | reqwest with `rustls-platform-verifier` | On Android the verifier needs a JNI init with the app `Context` and its companion Maven artifact. Alternative: bundled `webpki-roots` on Android (simpler, but ignores user-installed CAs). Decide in the spike |
| Crypto provider | `aws-lc-rs` in the tree | Needs CMake + NDK to cross-compile; check that it builds with `cargo-ndk` and what it adds to `.so` size. Fall back to `ring` if it's a problem |
| Background tasks | `start_background_tasks()` timers | Add an option to skip them; WorkManager calls `sync_all_repos()` / `refresh_trackers()` instead |
| `.torrent` saving | `save_torrent` writes to a file path | Add `fetch_torrent(url) -> bytes` so Kotlin writes through MediaStore / SAF |
| Tor (Arti) | Uses default state/cache dirs | Point its state and cache dirs at the app's files/cache dirs. Bootstrap only when a provider or the global setting needs Tor, and keep the circuit state across app restarts so later bootstraps are quick |
| Definition hot reload | `notify` file watcher | Not used on Android |

### 5.3 Build

- `cargo-ndk` for `arm64-v8a` (Fold7 and nearly all phones), `armeabi-v7a` (older 32-bit phones, which are mostly Android 8–10) and `x86_64` (emulators, Chromebooks).
- **16 KB page alignment:** link the Rust `.so` with `-Wl,-z,max-page-size=16384` (the default with NDK r28+). Newer Android 15/16 devices can use 16 KB pages and refuse libraries aligned to 4 KB.
- A Gradle task runs `cargo ndk` and `uniffi-bindgen` before `preBuild`, so `./gradlew assembleRelease` is the only command needed.
- UniFFI's Kotlin bindings load the library through **JNA** (`net.java.dev.jna:jna` AAR). R8 shrinking stays on, with keep rules for JNA and the generated bindings.
- **Size:** Arti, SQLite and the TLS stack all go into one `.so`. Target under about 25 MB per ABI APK; A0 measures the real number. Release profile already uses thin LTO, one codegen unit and `strip`; add `opt-level = "s"` for the Android library if needed.

### 5.4 Minimum Android version

Every v1 feature was checked against the Android version it needs:

| Feature | Needs | Below that |
|---|---|---|
| Compose, Material 3, WindowManager, WorkManager (current AndroidX) | Android 6 (API 23) | — |
| Keystore AES-GCM for secrets | Android 6 (API 23) | — |
| Share target, `PROCESS_TEXT` | Android 6 (API 23) | — |
| Drag and drop between apps, multi-window | Android 7 (API 24) | — |
| App shortcuts | Android 7.1 (API 25) | — |
| Rust `.so` via NDK, Arti, SQLite | Android 5 (API 21) | — |
| Save `.torrent` straight to Downloads (MediaStore) | **Android 10 (API 29)** | "Save as" picker instead; same result, one extra tap, no storage permission |
| Follow system dark mode | **Android 10 (API 29)** | In-app light/dark switch, which exists on all versions anyway |
| Dynamic colour | Android 12 (API 31) | Hashlark palette (same for everyone below 12) |
| Predictive back animations | Android 14 (API 34) | Normal back (same for everyone below 14) |
| Foldable layouts | Any — every foldable ships with Android 10+ | — |

**Result: `minSdk 26` (Android 8.0).** Going below Android 10 drops no feature; two things just take a slightly different path. Android 8 is a clean floor (adaptive icons, notification channels, `java.time` without desugaring). Going lower than 8 gains almost no devices. CI runs the UI tests on an Android 8 and an Android 9 emulator as well.

---

## 6. Testing

| Layer | Approach |
|---|---|
| FFI | Rust tests for the conversion layer; an instrumented Kotlin test that runs a search against a `wiremock`-style fixture server on the emulator |
| Kotlin wrapper | JUnit tests for the `Flow` wrapper (cancellation, error mapping), with a fake engine |
| Layouts | **Screenshot tests at a fixed size matrix** (Compose Preview Screenshot Testing or Roborazzi): cover portrait, cover landscape with IME, inner portrait, inner landscape, tabletop, book, half of split screen, 1/3 split screen |
| Postures | `androidx.window:window-testing` to feed fake `FoldingFeature`s (half-open horizontal / vertical, flat) into UI tests |
| Continuity | UI test that recreates the activity mid-search and checks results and scroll position survive |
| Old versions | Instrumented tests on Android 8 and 9 emulators, including the "Save as" path |
| Tor | Instrumented test that bootstraps Arti and runs a search in "Tor only" mode (on a network that allows Tor) |
| Manual | Real Z Fold7 checklist: fold/unfold mid-search, rotate on both screens, Flex mode, split screen with a torrent client + drag and drop, keyboard + mouse, DeX/desktop windowing if available, Tor on mobile data |

---

## 7. CI

- **`ci.yml`** gets an Android job: install the NDK and `cargo-ndk`, build the `.so` files, run `./gradlew lint testDebugUnitTest`, run screenshot tests, and one emulator job for the FFI smoke test (PLAN.md §16).
- **`release.yml`** (on a `v*` tag) gets an Android job that builds signed release APKs and attaches them to the same **draft** GitHub release as the desktop installers (see [§8](#8-distribution-and-signing)).
- `cargo deny` already covers the Rust side; add Gradle dependency verification for Kotlin.

---

## 8. Distribution and signing

**Decided: GitHub Releases only**, as signed release APKs. No Play Store, no F-Droid for now.

### 8.1 What each release contains

| File | For |
|---|---|
| `hashlark-<version>-arm64-v8a.apk` | Almost every phone, including the Fold7 |
| `hashlark-<version>-armeabi-v7a.apk` | Older 32-bit phones |
| `hashlark-<version>-x86_64.apk` | Emulators, Chromebooks |
| `hashlark-<version>-universal.apk` | Anyone who isn't sure; all ABIs, bigger |
| `SHA256SUMS` | Checksums for all APKs (and the desktop files) |

- `versionCode` = `major·1000000 + minor·10000 + patch·100 + abi` (abi: 1 = armeabi-v7a, 2 = arm64-v8a, 3 = x86_64, 9 = universal), so updates always move forward whichever APK a user installed.
- Signed with **APK Signature Scheme v2 and v3** (v1/JAR signing isn't needed for `minSdk 26`).
- The release notes and `docs/releasing.md` list the **SHA-256 fingerprint of the signing certificate**, so users can check the APK (e.g. with AppVerifier) before installing. Obtainium users can point it at the GitHub repo to get updates.

### 8.2 Signing key

- Generate one release keystore once (`keytool`, RSA 4096, 30+ year validity). Keep the file and its passwords in a password manager **and** an offline backup. **If it's lost, existing installs can't be updated**; users would have to uninstall (and lose their data) to move to a new key.
- GitHub repository secrets:
  - `ANDROID_KEYSTORE_BASE64`: the keystore file, base64-encoded;
  - `ANDROID_KEYSTORE_PASSWORD`;
  - `ANDROID_KEY_ALIAS`;
  - `ANDROID_KEY_PASSWORD`.
- Gradle's `signingConfigs.release` reads these from environment variables; local release builds use a `keystore.properties` file that is git-ignored. Debug builds keep the default debug key.
- The v3 scheme allows rotating to a new key later (with a signing lineage) if it's ever needed.

### 8.3 Release steps (added to `docs/releasing.md` in A5)

1. Bump `versionName` / `versionCode` together with the desktop version.
2. Tag and push; `release.yml` builds the desktop, server and Android files into one draft release.
3. Install the arm64 APK on the Fold7 over the previous version (checks the update path and signature), run the manual checklist from §6.
4. Publish the release. The in-app update notice only sees published releases.

---

## 9. Milestones

Replaces A1–A3 in PLAN.md §17. Same assumption: one developer, about 15–20 h/week.

| M | Name | Scope | Exit criteria | Est. |
|---|---|---|---|---|
| **A0** | Spike | Cross-compile `hashlark-core` with `cargo-ndk` (SQLite, `aws-lc-rs`, **Arti**); TLS roots on device; UniFFI async + callback streaming prototype; measure `.so` size with Tor; check 16 KB alignment | A debug APK runs one normal and one Tor search on the Fold7; size and TLS decisions written down as an ADR | 1 wk |
| **A1** | FFI | `hashlark-ffi` with the full engine surface, `SecretStore` bridge, core changes from §5.2, Gradle integration, AAR | Kotlin instrumented test runs a streaming search and a cancel on an emulator | 2 wks |
| **A2** | Adaptive shell + search | App skeleton, theme, `NavigationSuiteScaffold`, search screen with streaming results, `ListDetailPaneScaffold` details, filters, magnet handoff, share / process-text entry points, state kept across rotate and fold | Search → magnet opens in a torrent client, on cover and inner screens, in both orientations, and survives folding mid-search | 3–4 wks |
| **A3** | Parity | Providers, repositories, history, favourites, settings, `.torrent` saving (MediaStore + "Save as"), challenge WebView, **built-in Tor** and Orbot, WorkManager sync, app shortcuts, first-run notice, update notice | Every desktop feature in §4.1 works on Android, including "Tor only" mode | 3 wks |
| **A4** | Foldable polish | Tabletop and book layouts, three-pane layout, table rows on wide widths, drag and drop, keyboard/mouse, split-screen and pop-up sizes, screenshot test matrix, Android 8/9 emulator runs | The §6 manual checklist passes on a real Z Fold7 | 1–2 wks |
| **A5** | Release | Release keystore + GitHub secrets, signed ABI-split and universal APKs in `release.yml`, `SHA256SUMS`, certificate fingerprint published, `docs/releasing.md` and user-guide sections | Signed v1.0 APKs on a published GitHub release; updating over a previous build works | 1 wk |

**Total: about 11–13 weeks part time.** The v1.x items in §4.2 come after A5.

---

## 10. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| `aws-lc-rs` / Arti don't cross-compile cleanly | A0/A1 delays | Found in the A0 spike; switch the TLS crypto provider to `ring` if needed |
| Tor makes the APK large | Bigger downloads | ABI splits (users download one ABI, not four); size-optimised release profile; measured in A0 |
| Tor can't bootstrap on some networks (as seen on desktop) | "Tor only" providers fail | Clear status in the UI; Orbot (which has bridges) as the documented alternative; add bridges when desktop does |
| Arti bootstrap uses battery and data on mobile | Poor experience | Bootstrap on demand only; keep state between runs; never keep Tor running in the background |
| TLS verification differences on Android | Providers fail with certificate errors | Decide the verifier in A0; test against all first-party providers on a real device |
| UniFFI callback streaming under load (many providers, many results) | Janky UI, dropped events | Batch results in Rust (the aggregator already sends batches); collect the Flow off the main thread; test with 16 providers |
| Samsung-specific behaviour (cover-screen continuity setting, Flex mode panel, DeX) | Layout surprises on the Fold7 | Real-device checklist in A4; rely on standard APIs, never on Samsung SDKs |
| Lost or leaked signing key | No more updates for existing installs / fake updates | Offline backup + password manager; secrets only in GitHub Actions; publish the certificate fingerprint |
| Users sideloading from untrusted mirrors | Tampered APKs | Only GitHub Releases is official; checksums and certificate fingerprint in every release |

---

## 11. Decisions

| # | Question | Decision (2026-09-29) |
|---|---|---|
| 1 | Distribution | **GitHub Releases only**, signed release APKs ([§8](#8-distribution-and-signing)) |
| 2 | Tor in v1 | **Yes**, built-in Arti, with Orbot as an alternative |
| 3 | Minimum Android version | **Android 8.0 (`minSdk 26`)**: nothing is lost below Android 10 ([§5.4](#54-minimum-android-version)) |
| 4 | Saved-search alerts | **Not planned** |

---

## 12. Implementation notes

What was built, and where it differs from the plan above. Measured on an Android 36 foldable emulator (Pixel Fold profile) and unit-tested; the Galaxy Z Fold7 checklist in §6 is still to be run on the real device.

### Spike results (A0)

- `hashlark-core` with **Arti, SQLite and `aws-lc-rs` cross-compiles** for all three ABIs with `cargo-ndk` and NDK 29 (no fallback to `ring` needed).
- Native library: about **23.5 MB stripped for arm64-v8a** with Tor. A release APK is **13 MB per ABI** (libraries compressed in the APK) and 24 MB for the universal APK, inside the 25 MB target.
- TLS: bundled Mozilla roots on Android, see [ADR 0014](adr/0014-android-tls-roots.md).
- 16 KB page alignment comes from NDK 29's default linker settings; the APK build is `targetSdk 36`, `minSdk 26`.

### Differences from the plan

| Plan | Built | Why |
|---|---|---|
| FFI records for every type (§5.1) | One `HashlarkEngine` object that exchanges the HTTP API's JSON | [ADR 0015](adr/0015-ffi-json-boundary.md): one contract for desktop, server and Android, and a 400-line FFI crate |
| `ListDetailPaneScaffold`, `SupportingPaneScaffold` (§3.1) | `NavigationSuiteScaffold` plus a small two-pane layout (`ui/common/AdaptiveListDetail.kt`) driven by `WindowShape` | The library scaffold kept its pane state after an unfold (the details stayed alone on the wide screen) and, on the emulator's cover screen, still split the window at the inner screen's hinge. Deriving the layout only from the current window size and posture removed both problems and made it testable |
| Material 3 Adaptive 1.3 | 1.2 | 1.3 needs `compileSdk 37` |
| A chip per provider under the search box (§4.1) | One summary chip beside the result count; tapping it opens the per-provider list (counts, speed, errors, "open site" for browser checks) | The chips took several rows on a narrow list pane; the popup keeps the same information and gives the results the room |
| Separate `SupportingPaneScaffold` for filters; three panes from 1200 dp | Filters are a bottom sheet below 600 dp and a dialog above, with Apply / Reset / Cancel; two panes at most, and only when `WindowShape.planPanes()` finds room after navigation, insets, margins and text scale (never from 600 dp alone: the Fold7 inner display, about 750 × 832 dp, uses list → full-screen details) | Replaced the earlier 1200 dp filter panel (and the 600 dp two-pane rule) in the UI refactor of 29 September 2026. Earlier reason:  The inner display of the Fold7 is about 840 dp wide: two roomy panes beat three cramped ones |
| `WindowInfoTracker` for postures | `currentWindowAdaptiveInfo().windowPosture`, which is built on it | Same data; fewer moving parts. A hinge is ignored when either half would be under 300 dp (the emulator reports the inner hinge on the cover screen) |
| Roborazzi screenshot tests (§6) | Robolectric layout tests that assert the layout branch for nine window shapes and save a PNG of each | Same coverage of layouts without the plugin (and its AGP 9 risk); pictures are uploaded by CI for review, not diffed |
| `EncryptedSharedPreferences` avoided | AES-256-GCM key in the Android Keystore, values in a DataStore file | As planned; the instrumented test found that Keystore keys refuse a caller-supplied IV, which is why encryption lets the provider pick it |
| Trust-on-first-use prompt (§4.1) | `Engine::preview_repo` fetches the signed index, and the app shows the key fingerprint before anything is stored | The core had no way to show the key before pinning it; `preview_repo` is new |
| `fetch_torrent(url) -> bytes` (§5.2) | Added; `save_torrent` now uses it | As planned |

### Bugs the tests and the emulator caught

- The `.yml` intent filter matched every file (a `pathPattern` is ignored without a host), so Hashlark appeared in its own "Open with" list for `.torrent` files; fixed, and Hashlark is excluded from that chooser.
- Two `KeystoreSecretStore` instances created two DataStores on one file, which DataStore forbids; the store is now shared per file.
- Keystore encryption failed with "Caller-provided IV not permitted".
- A cancelled search ends without a `done` event, so the Kotlin `Flow` never completed; the listener has an explicit `on_end`.

### Tests

| Layer | Where |
|---|---|
| Rust | `cargo test -p hashlark-ffi` (engine, errors, streaming, cancel) and the core's tests, including `preview_repo` |
| Kotlin logic | `apps/android/core/src/test` (models against real JSON, sorting, search state, IMDb parsing, update check, encryption) |
| App logic | `apps/android/app/src/test` (view model with a fake engine, intent routing, window shapes) |
| Layouts | `LayoutMatrixTest`: cover portrait and landscape, inner portrait and landscape, a large window, tabletop, book, half and a third of a split screen |
| On a device | `app/src/androidTest`: a real search against a local Torznab fixture, cancellation, the Keystore, and recreating the activity mid-search with the scroll position kept. The Tor test is opt-in (`-Pandroid.testInstrumentationRunnerArguments.tor=true`) |

### Not done yet

- The manual checklist on a physical Galaxy Z Fold7 (Flex mode panel, cover-screen continuity setting, DeX), and the Android 8 and 9 emulator runs (CI runs them; this machine had only an API 36 image).
- The first signed release: it needs the keystore and secrets from [releasing.md](releasing.md).
- v1.x items from §4.2 (home-screen widget, app lock, remote mode, definition editor on large screens, Tor bridges).
- Translations: the UI strings are English and live in the Compose code.

