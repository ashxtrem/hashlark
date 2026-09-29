# 0002. Tauri 2 + Svelte 5 for the desktop UI

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

We need a desktop UI for Windows (primary), macOS and Linux that is small, fast, and works well with a Rust core. The same UI should also be servable to browsers by the headless server ([0009](0009-headless-server-v1.md)).

## Decision

Use **Tauri 2** as the shell and **Svelte 5 + SvelteKit (static adapter) + TypeScript** for the UI.

## Consequences

- Uses the system webview (WebView2 on Windows, WKWebView on macOS), so installers stay around 10–15 MB.
- The Rust core runs in the same process as the shell.
- Because the UI is a static web build that talks only to the HTTP API, the headless server can serve the same build to browsers.
- Rendering differs slightly between webviews and needs testing on each OS. On Windows 10 the installer must bootstrap WebView2.
