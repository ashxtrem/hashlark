# Architecture Decision Records

Each ADR records one decision: the context, what we chose and what follows from it. ADRs are never edited after acceptance except to change their status. A reversed decision gets a new ADR that supersedes the old one.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-rust-core.md) | Rust for the core | Accepted |
| [0002](0002-tauri-svelte-desktop.md) | Tauri 2 + Svelte 5 for the desktop UI | Accepted |
| [0003](0003-standalone-android-uniffi.md) | Standalone Android app with the core embedded via UniFFI | Accepted |
| [0004](0004-http-api-contract.md) | Local HTTP API (REST + SSE) as the UI ↔ core contract | Accepted |
| [0005](0005-provider-model.md) | Native + declarative definitions + Torznab providers; no script plugins | Accepted |
| [0006](0006-sqlite-storage.md) | SQLite via sqlx for storage | Accepted |
| [0007](0007-legal-default-providers.md) | Ship only legal sources as default providers | Accepted |
| [0008](0008-os-download-handoff.md) | Hand downloads to the OS handler only | Accepted |
| [0009](0009-headless-server-v1.md) | Ship headless server mode in v1.0 | Accepted |
| [0010](0010-no-telemetry.md) | No telemetry | Accepted |
| [0011](0011-licence-gpl-3.md) | Licence: GPL-3.0-or-later | Accepted |
| [0012](0012-own-doh-resolver.md) | Our own DNS-over-HTTPS resolver | Accepted |
| [0013](0013-embedded-tor-arti.md) | Built-in Tor with Arti behind a local SOCKS bridge | Accepted |

New ADRs: copy [template.md](template.md) and use the next number.
