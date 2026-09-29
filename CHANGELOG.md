# Changelog

All notable changes to Hashlark are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-09-29

### Added

- **Android app** (Kotlin, Jetpack Compose, Material 3; Android 8.0 and newer), standalone: the Rust core runs inside the app through UniFFI.
  - Every desktop feature except API keys and the click-to-pick definition editor: streaming search with provider status, filters and paging, result details, magnet handoff to a torrent client (with copy and share as fallback), `.torrent` saving to Downloads (a "Save as" picker on Android 9 and older), providers with health, logins, tests and per-provider network route, Torznab and definition import, signed definition repositories (with a trust-on-first-use key prompt), favourites, history, browser checks in a `WebView`, encrypted DNS, proxy, built-in Tor or Orbot, and update notices.
  - **Adaptive layouts** chosen by window size and fold posture, never by device: bottom bar, rail or drawer; one, two or three panes; a results table on wide windows; tabletop and book postures for foldables; and no state lost when folding, rotating or resizing.
  - Android extras: search from the share sheet or the text-selection menu, IMDb links become exact searches, `hashlark://` links and `.yml` files, launcher shortcuts (including the last searches), background repository sync with WorkManager, Material You colours, drag and drop of a magnet into another app, keyboard and mouse support.
  - Signed APKs per ABI plus a universal APK on GitHub Releases, with `SHA256SUMS` and the signing-certificate fingerprint.
- Core: `Engine::fetch_torrent`, `Engine::preview_repo` (shows a repository's signing key before it is trusted), and bundled root certificates for TLS on Android.
- **Search engine**
  - Searches all enabled providers in parallel, with per-provider timeouts.
  - Results stream in as each provider answers; duplicates are merged by infohash and ranked.
- **Providers**
  - Internet Archive (built in).
  - Definitions for Linuxtracker, Academic Torrents and FOSS Torrents.
  - Torznab endpoints (Jackett, Prowlarr, Bitmagnet).
  - Import your own YAML definitions.
- **Definition engine**
  - HTML (CSS selectors), JSON (JSONPath) and XML/RSS.
  - Filters, templates and form or cookie login.
  - Static feeds, mirror fallback, and details-page resolution.
  - Tooling: `defs new`, `defs lint`, `defs test` (with `--record`), `defs schema`, and hot reload.
- **Definition repositories**: signed with Ed25519, with the signing key pinned on first use; synced daily.
- **Network**
  - DNS-over-HTTPS on by default (Cloudflare, Quad9, Google or a custom resolver).
  - Global and per-provider proxy, and Tor through a SOCKS address.
  - A per-site rate limit.
  - Detection of blocking and of browser checks, with a desktop flow to pass a browser check.
- **Provider health**: success rate and latency are tracked; failing providers are paused and retried with backoff.
- **Desktop app** for Windows, macOS and Linux (Tauri + Svelte):
  - search, details, favourites, history, providers, repositories and settings;
  - light and dark themes;
  - hands magnets to the OS; saves `.torrent` files;
  - signed auto-updates.
- **Headless server**
  - Web UI and REST/SSE API (OpenAPI).
  - Torznab endpoint for Sonarr and Radarr.
  - API keys and TLS.
  - Docker image and systemd unit.
- **Privacy**: no telemetry. Credentials are kept in the OS keychain, or in an owner-only file on servers.
