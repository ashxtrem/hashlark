# 0008. Hand downloads to the OS handler only

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

Hashlark finds torrents but doesn't download them in v1. We considered integrating with specific clients (e.g. the qBittorrent Web API).

## Decision

Hand off through the **OS only**. Magnet links open in whatever app is registered for `magnet:`. `.torrent` files are saved to the configured folder and opened with the default app. **Copy magnet** is always available.

## Consequences

- Works with any client, and there's no client-specific code to maintain.
- If no app is registered for `magnet:`, the UI must say so clearly and offer Copy magnet.
- Client API integrations can be revisited after v1.0 with a new ADR.
