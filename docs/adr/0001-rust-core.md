# 0001. Rust for the core

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

The search logic (providers, aggregation, definitions, networking, storage) has to run in three places: the Windows/macOS/Linux desktop app, a headless server, and a standalone Android app. We considered Python, Rust and Kotlin Multiplatform.

## Decision

Write the core in **Rust** (edition 2024, stable toolchain).

## Consequences

- One small native binary on desktop and server, with no runtime to bundle.
- Compiles to a native library that Android loads through FFI ([0003](0003-standalone-android-uniffi.md)); Python can't do this sensibly.
- Native crates exist for Tor (`arti-client`) and a future torrent engine (`librqbit`).
- Steeper learning curve and slower iteration than Python. This is offset by keeping most site-specific logic in YAML definitions ([0005](0005-provider-model.md)) instead of code.
