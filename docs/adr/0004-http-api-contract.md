# 0004. Local HTTP API (REST + SSE) as the UI ↔ core contract

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

Several front ends (desktop UI, browser UI in headless mode, CLI, possibly remote Android later, Sonarr/Radarr) need to talk to the core.

## Decision

The core is exposed through an **HTTP API** (`hashlark-server`, built on axum): REST for commands, **Server-Sent Events** for streaming search results, and a Torznab-compatible endpoint. The API is described by an OpenAPI 3.1 spec, from which the TypeScript (and later Kotlin) clients are generated. The desktop app runs this server inside its own process on `127.0.0.1` with a random port and a random bearer token.

## Consequences

- The desktop, headless and browser front ends use one identical contract.
- The local server has to be protected against other local software and websites: bearer token, strict `Host` header check, no permissive CORS.
- Android (standalone) calls the core through FFI rather than HTTP ([0003](0003-standalone-android-uniffi.md)), so the core's Rust API has to stay clean independently of the HTTP layer.
