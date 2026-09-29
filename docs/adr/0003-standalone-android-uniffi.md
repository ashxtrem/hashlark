# 0003. Standalone Android app with the core embedded via UniFFI

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

Phase 2 adds an Android app written in Kotlin. It could either be a thin client of a desktop/headless instance, or run the search logic itself.

## Decision

The Android app is **standalone**. The Rust core is compiled with `cargo-ndk` and exposed to Kotlin through **UniFFI** bindings (`hashlark-ffi`).

## Consequences

- Works without a PC or server, and uses exactly the same search logic as desktop.
- Streaming search results cross the FFI boundary through a callback interface, which a Kotlin wrapper turns into a `Flow`.
- Platform services the core needs (secret storage) are passed in from Kotlin as traits.
- APK size grows by the native library (target: under about 20 MB per ABI split).
- A remote-client mode can be added later without changing this decision.
