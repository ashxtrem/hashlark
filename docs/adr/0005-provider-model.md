# 0005. Native + declarative definitions + Torznab providers; no script plugins

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

Indexers change layouts and domains often. We want adding or fixing an indexer to be quick, safe, and possible without an app release.

## Decision

Support three provider kinds behind one `SearchProvider` trait:

1. **Native**: Rust code, for complex or first-party sources.
2. **Definition**: a YAML file interpreted by a generic engine (HTML/CSS selectors, JSON/JSONPath, XML), distributed through signed definition repositories.
3. **Torznab**: any Jackett, Prowlarr or Bitmagnet endpoint.

**No script plugins** (JS/Lua/Python) in v1.

## Consequences

- Most indexers become data files. Fixes reach users through repo sync, with no app release.
- Definitions can't run code, so importing them from a URL is safe.
- Sites that need custom logic (JS-only pages, 2FA logins, signed requests) need a native provider or Jackett.
- We have to maintain a definition engine, a JSON Schema, and tooling (`defs new/lint/test`, hot reload).
