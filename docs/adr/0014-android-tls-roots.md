# 0014. Bundled root certificates for TLS on Android

- **Status:** Accepted
- **Date:** 2026-09-29

## Context

`reqwest` verifies HTTPS certificates with `rustls-platform-verifier` on every platform. On Android that verifier can only work after it has been given a JNI environment and the app `Context`, and it needs a companion Kotlin class (`org.rustls.platformverifier.CertificateVerifier`) inside the APK. The Rust core is loaded by UniFFI through JNA, which never runs `JNI_OnLoad`, so there is no natural place to hand it a `JNIEnv`, and a missed initialisation is a **panic on the first request**.

This was the open question of the A0 spike (docs/android-plan.md §5.2).

## Decision

On `target_os = "android"` the core builds every HTTP client with `tls_certs_only(...)`, trusting Mozilla's root certificates from the `webpki-root-certs` crate. Other platforms are unchanged and keep using the operating system's verifier. All clients are created through `net::client_builder()`, so the DoH resolver and the provider clients cannot forget it.

## Consequences

- Nothing to initialise: TLS works on the first call, on every supported Android version (26+), with no extra Gradle artefact.
- Certificate authorities the user installed on the device (a corporate proxy, a self-hosted Jackett behind a private CA) are **not** trusted. Plain-HTTP Torznab URLs and public certificates work as before. If this turns out to matter, the platform verifier can replace this later without touching callers.
- The trust list changes with the crate version, not with Android security updates. Dependabot/`cargo update` keeps it current, and every release picks up the newest list.
- `webpki-root-certs` adds roughly 200 KB to the native library.
- Built-in Tor is not affected: Arti authenticates relays with Tor's own protocol, not the web PKI.
