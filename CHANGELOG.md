# Changelog

All notable changes to Hashlark are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

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
