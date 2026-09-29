# 0015. The FFI boundary carries the HTTP API's JSON

- **Status:** Accepted
- **Date:** 2026-09-29

## Context

[ADR 0003](0003-standalone-android-uniffi.md) embeds the core in the Android app through UniFFI. The core has about forty serialisable types (search results, provider views, settings, repositories, ...). The plan proposed mirroring them as UniFFI records with `From` conversions.

## Decision

`hashlark-ffi` exposes **one object, `HashlarkEngine`, whose methods take and return the same JSON documents as the HTTP API** ([ADR 0004](0004-http-api-contract.md)). Only what cannot be JSON crosses as typed values:

- `SearchListener` (a callback interface): receives each `SearchEvent` as JSON and the Kotlin wrapper turns it into a cold `Flow` that cancels the search when collection stops;
- `SecretStore` (a callback interface): implemented in Kotlin with the Android Keystore;
- `HashlarkError`: one error type with the HTTP API's stable `code`, a description, the details of an invalid definition and, for provider failures, the provider error kind;
- bytes (`fetch_torrent`), strings and integers.

The Kotlin side parses with kotlinx.serialization models that mirror the OpenAPI schema (`core/Models.kt`).

## Consequences

- An API change lands in one place, the core; the FFI crate stays about 400 lines with no per-type conversions, and `unsafe_code = "allow"` is confined to that crate.
- The desktop, the server and Android share one contract, one set of field names and one test surface.
- A field added to the core appears in Kotlin as a new property with a default; unknown fields are ignored, so an older app never breaks on a newer core.
- The cost is one JSON encode and decode per call. Searches send batches, not single results, so this is not measurable next to network time.
- The Kotlin models must be kept in step with the schema by hand. `ModelsTest` parses documents shaped like the core's output to catch drift.
