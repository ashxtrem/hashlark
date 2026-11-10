# 0010. No telemetry

- **Status:** Accepted, amended by [0017](0017-built-in-torrent-client.md)
- **Date:** 2026-09-28

## Context

Provider-health telemetry could help maintain definitions, but it conflicts with user privacy and would require running a backend.

## Decision

**No telemetry.** No analytics, crash reports or usage data are collected. The only outgoing requests are:

1. searches and resolves sent to providers the user enabled;
2. definition-repo sync;
3. tracker-list refresh (if configured);
4. the update check against GitHub Releases, which users can turn off;
5. torrent traffic of the built-in client (ADR 0017): peers, trackers, DHT and UPnP. It goes direct, never through Tor or the search proxy.

## Consequences

- Nothing to host, and an easy privacy story, stated in the About screen.
- Broken definitions are found by our own nightly canary job and by user reports, not by field data.
