# 0009. Ship headless server mode in v1.0

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

Some users want Hashlark running on a NAS or home server, reachable from any browser, and usable as an indexer by Sonarr/Radarr.

## Decision

Ship `hashlark-server` as a standalone product in **v1.0**: binaries for Windows, macOS and Linux (x86_64 and arm64), a multi-arch Docker image, a `hashlark.toml` config file with `HASHLARK_*` environment overrides, API-key authentication, optional TLS, and serving of the same Svelte web UI.

## Consequences

- Adds milestone M6.5 (about 1–2 weeks) before the v1.0 release.
- Exposing the server beyond localhost brings security work: hashed API keys, login rate limiting, a warning when bound to `0.0.0.0` without TLS.
- The UI must not rely on Tauri-only features for core flows. Magnets in browser mode open through the browser.
