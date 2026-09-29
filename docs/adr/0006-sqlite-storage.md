# 0006. SQLite via sqlx for storage

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

We need local persistence for providers, definitions, settings, health, history, favourites and a result cache on desktop, server and Android.

## Decision

Use **SQLite** through **sqlx** (async, Tokio runtime), with embedded migrations in `crates/hashlark-core/migrations/`. Timestamps are stored as Unix epoch milliseconds. Credentials are **never** stored in SQLite; they go to the OS keychain / Android Keystore.

## Consequences

- One embedded database file per installation, with nothing to install.
- WAL mode allows the UI to read while searches write.
- Migrations run automatically on start-up. They must be forward-only and tested.
- Queries use runtime-checked `sqlx::query` (not the compile-time macros), so building doesn't need a live database.
