# Hashlark — Project Plan

> **Hashlark**: a fast, cross-platform torrent **search** app. It asks many indexers at once, merges the results and hands the magnet link or `.torrent` file to the user's download client.
> Working folder name: `torseek`. Product name: **Hashlark** (see [§1](#1-name)).

| | |
|---|---|
| Status | **Phase 1 implemented (M0–M8, v1.1)**; not yet released. See [Implementation status](#implementation-status). Decisions in [§19](#19-decision-log-and-open-questions) |
| Date | 2026-09-28 |
| Platforms | Phase 1: Windows (primary), macOS, Linux, plus a headless server · Phase 2: Android (standalone) |
| Core stack | Rust core + local HTTP API · Tauri 2 + Svelte 5 desktop · Kotlin/Compose Android via UniFFI |
| Licence | GPL-3.0-or-later |
| Telemetry | None |

---

## Table of contents

1. [Name](#1-name)
2. [Goals and non-goals](#2-goals-and-non-goals)
3. [Key decisions](#3-key-decisions)
4. [Architecture](#4-architecture)
5. [Repository layout](#5-repository-layout)
6. [Core design (`hashlark-core`)](#6-core-design-hashlark-core)
7. [Provider definition format](#7-provider-definition-format)
8. [Network layer](#8-network-layer)
9. [HTTP API (`hashlark-server`)](#9-http-api-hashlark-server)
10. [Desktop app (Tauri + Svelte)](#10-desktop-app-tauri--svelte)
11. [Android app (Phase 2)](#11-android-app-phase-2)
12. [Storage](#12-storage)
13. [Security](#13-security)
14. [Legal and distribution posture](#14-legal-and-distribution-posture)
15. [Testing strategy](#15-testing-strategy)
16. [CI/CD and releases](#16-cicd-and-releases)
17. [Milestones](#17-milestones)
18. [Risks and mitigations](#18-risks-and-mitigations)
19. [Decision log and open questions](#19-decision-log-and-open-questions)

---

## 1. Name

**Decided: Hashlark.** *Hash* comes from the infohash, the fingerprint of every torrent. *Lark* is a songbird, and "on a lark" means doing something for fun. The name is short, easy to say, neutral, and says nothing about piracy, which helps with store review.

Availability checked on 2026-09-28:

| Check | Result |
|---|---|
| Web search `"Hashlark"` | No product, company or project found |
| crates.io `hashlark` | Free |
| npm `hashlark` | Free |
| PyPI `hashlark` | Free |
| GitHub user/org `hashlark` | Free |
| `hashlark.com` | Did not resolve (probably unregistered) |

**Backups:** `Seekrift` (free on crates, npm, PyPI and GitHub; the .com appears registered) and `Lodebay`.

**Before going public:** run a trademark search (USPTO, EUIPO, WIPO Global Brand DB and the Indian IP office), register the domain, and create the GitHub org and crates.io names.

**Why not "torseek":** "Tor" in the name suggests a link to the Tor Project, which has a trademark policy. The folder can keep the name; the product shouldn't.

---

## 2. Goals and non-goals

### Goals
- **G1**: Search many indexers in **parallel** and show results **as they arrive**, with duplicates merged and a ranking that makes sense.
- **G2**: **Pluggable providers**: Torznab endpoints (Jackett/Prowlarr), declarative **definition files** (HTML/JSON/XML) and a few native providers. Users add providers; the app ships only legal ones by default.
- **G3**: **Updatable over the air.** When a site changes, a definition update fixes it without an app release.
- **G4**: **Robust networking**: DoH by default, mirror fallback, optional proxy and Tor, per-provider health checks and automatic disabling of broken providers.
- **G5**: **One core, many front ends**: the same Rust core runs the desktop app (through an HTTP API), a headless server mode and the Android app (embedded through FFI).
- **G6**: A small, fast desktop app: installer under about 15 MB, idle memory under about 80 MB, first results in under 2 s on a healthy network.

### Non-goals (v1)
- A built-in BitTorrent downloader. We hand off to qBittorrent, Transmission, the OS magnet handler and so on. Possible in Phase 3 with `librqbit`.
- Automated CAPTCHA or bot-challenge solving. Where a site presents a challenge, the **user** completes it in a webview (see [§8.5](#85-bot-challenge-pages-user-assisted)).
- Hosting any server-side relay or proxy for users.
- iOS.

---

## 3. Key decisions

| # | Decision | Choice | Rationale |
|---|---|---|---|
| D1 | Core language | **Rust** (edition 2024, stable) | One binary; runs inside Android through FFI; low memory; Tor (Arti) and a torrent engine (librqbit) exist as native crates |
| D2 | Desktop UI | **Tauri 2 + Svelte 5 (SvelteKit, static adapter) + TypeScript** | Small bundle, uses the system webview (WebView2 on Windows), Rust-native |
| D3 | Android | **Standalone**: Kotlin + Jetpack Compose, core embedded through **UniFFI** | Works without a PC or server; same search logic as desktop |
| D4 | UI ↔ core contract (desktop) | **Local HTTP API (REST + SSE)**, described by OpenAPI | The same contract serves the desktop UI, server mode and future clients; generated TS and Kotlin clients |
| D5 | Provider model | Native + Definition (YAML) + Torznab. **No script plugins in v1.** | Definitions can't run code, so importing them from a URL is safe |
| D6 | Storage | **SQLite** (`sqlx`) | Embedded, works on every platform |
| D7 | Default providers | **Legal sources only** (Internet Archive, Academic Torrents, Linux distros) | Keeps app-store policies and legal exposure manageable |
| D8 | Download handoff | **OS handler only**: magnets open in whatever app the OS has registered for `magnet:`; `.torrent` files are saved or opened | Simplest option, works with any client, no client-specific code |
| D9 | Headless server | **Ships in v1.0** (standalone binary + Docker image) and also serves the Svelte web UI | Lets the app run on a NAS or home server, be used from a browser, and feed Sonarr/Radarr |
| D10 | Telemetry | **None** | Privacy; no backend to run |
| D11 | Licence | **GPL-3.0-or-later** | Keeps forks open; normal for this kind of tool; all planned dependencies (MIT/Apache/BSD) are compatible |

---

## 4. Architecture

```
                         ┌─────────────────────────────────────────────┐
                         │               hashlark-core (Rust)          │
                         │                                             │
                         │  ┌────────────┐   ┌──────────────────────┐  │
                         │  │ Aggregator │──▶│ Provider registry     │  │
                         │  │ fan-out,   │   │ ├ NativeProvider      │  │
                         │  │ dedupe,    │   │ ├ DefinitionProvider  │  │
                         │  │ rank, SSE  │   │ └ TorznabProvider     │  │
                         │  └────────────┘   └──────────┬───────────┘  │
                         │                              │              │
                         │  ┌───────────────┐  ┌────────▼───────────┐  │
                         │  │ Definition    │  │ Network layer      │  │
                         │  │ engine + repo │  │ DoH · mirrors ·    │  │
                         │  │ sync (signed) │  │ proxy · Tor · rate │  │
                         │  └───────────────┘  │ limit · cookies    │  │
                         │                     └────────────────────┘  │
                         │  ┌──────────┐ ┌───────────┐ ┌────────────┐  │
                         │  │ SQLite   │ │ Health    │ │ Magnet     │  │
                         │  │ store    │ │ tracker   │ │ builder    │  │
                         │  └──────────┘ └───────────┘ └────────────┘  │
                         └───────────┬─────────────────────┬───────────┘
                                     │                     │
                  ┌──────────────────▼──────┐     ┌────────▼──────────────┐
                  │ hashlark-server (axum)  │     │ hashlark-ffi (UniFFI) │
                  │ REST + SSE + Torznab    │     │ Kotlin bindings       │
                  └───┬───────────────┬─────┘     └────────┬──────────────┘
                      │               │                    │
          ┌───────────▼─────┐   ┌─────▼────────────┐  ┌────▼──────────────┐
          │ Desktop (Tauri  │   │ Headless server  │  │ Android app       │
          │ + Svelte),      │   │ mode (NAS/PC),   │  │ (Kotlin/Compose), │
          │ server runs     │   │ Sonarr/Radarr    │  │ fully standalone  │
          │ in-process      │   │ via Torznab      │  │                   │
          └─────────────────┘   └──────────────────┘  └───────────────────┘
```

### Ways to run it
| Mode | Binary | Who uses it |
|---|---|---|
| **Desktop** | Tauri app; `hashlark-server` runs inside the app on `127.0.0.1:<random>` with a random token | End users on Windows, macOS and Linux |
| **Headless** | `hashlark-server` standalone (listens on the LAN with auth), also serving the Svelte web UI | Power users on a NAS or home server; browser access; Sonarr/Radarr through Torznab |
| **Embedded** | `libhashlark_ffi.so` inside the APK | Android |
| **CLI** | `hashlark-cli` | Development, testing definitions, scripting |

---

## 5. Repository layout

```
torseek/                         # repo root (can rename to hashlark/)
├── Cargo.toml                   # workspace
├── crates/
│   ├── hashlark-core/           # domain, providers, aggregator, definitions, network, store
│   ├── hashlark-server/         # axum API, SSE, Torznab endpoint, OpenAPI (utoipa)
│   ├── hashlark-cli/            # search / defs lint / defs test / health
│   └── hashlark-ffi/            # UniFFI bindings (Phase 2)
├── apps/
│   ├── desktop/                 # Tauri 2 + SvelteKit
│   │   ├── src/                 # Svelte UI
│   │   └── src-tauri/           # Tauri shell, embeds hashlark-server
│   └── android/                 # Gradle project, Kotlin + Compose (Phase 2)
├── definitions/                 # first-party definitions (legal sources) + JSON Schema
│   ├── schema/definition.schema.json
│   └── builtin/*.yml
├── fixtures/                    # recorded HTML/JSON/XML responses for tests
├── docs/
│   ├── PLAN.md                  # this file
│   ├── adr/                     # architecture decision records
│   └── definition-format.md     # format reference (from §7)
└── .github/workflows/
```

---

## 6. Core design (`hashlark-core`)

### 6.1 Main crates
| Concern | Crate |
|---|---|
| Async runtime | `tokio` |
| HTTP client | `reqwest` (rustls, cookies, gzip/brotli, socks) |
| DNS / DoH | `hickory-resolver` (DoH over rustls) plugged into reqwest |
| HTML | `scraper` (CSS selectors) |
| JSON | `serde_json` + `serde_json_path` (JSONPath, RFC 9535) |
| XML (RSS/Torznab) | `quick-xml` |
| YAML | a maintained serde YAML crate (check the options at M0; `serde_yaml` is archived) |
| Templating | `minijinja` |
| DB | `sqlx` (SQLite) |
| Rate limiting | `governor` |
| Fuzzy matching | `strsim` |
| Signatures | `ed25519-dalek` (or minisign format) |
| Errors / logging | `thiserror`, `tracing` |
| Paths | `directories` |
| Tor (v1.1) | `arti-client` |

### 6.2 Domain model

```rust
pub struct SearchQuery {
    pub text: String,
    pub categories: Vec<Category>,       // Movies, Tv, Music, Books, Software, Games, Anime, Other
    pub providers: Option<Vec<ProviderId>>, // None = all enabled
    pub imdb_id: Option<String>,          // for providers that support it
    pub page: u32,
    pub sort: SortOrder,                  // Relevance | Seeders | Size | Date
}

pub struct SearchResult {
    pub id: ResultId,                     // stable hash of (provider, infohash|details_url)
    pub title: String,
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub info_hash: Option<InfoHash>,      // normalized 40-char lowercase hex
    pub magnet: Option<String>,
    pub torrent_url: Option<Url>,
    pub details_url: Option<Url>,
    pub published: Option<OffsetDateTime>,
    pub category: Option<Category>,
    pub provider_id: ProviderId,
    pub needs_resolve: bool,              // magnet is only on the details page
}

pub struct MergedResult {                 // what the UI shows
    pub primary: SearchResult,
    pub sources: Vec<ProviderId>,         // every provider that returned this infohash
    pub seeders: Option<u32>,             // max over sources
    pub score: f32,
}

pub enum DownloadTarget { Magnet(String), TorrentFile(Url) }
```

### 6.3 Provider trait

```rust
#[async_trait]
pub trait SearchProvider: Send + Sync {
    fn info(&self) -> &ProviderInfo;      // id, name, kind, categories, caps, requires_auth
    async fn search(&self, ctx: &Ctx, q: &SearchQuery) -> Result<Vec<SearchResult>, ProviderError>;
    async fn resolve(&self, ctx: &Ctx, r: &SearchResult) -> Result<DownloadTarget, ProviderError>;
    async fn test(&self, ctx: &Ctx) -> Result<HealthReport, ProviderError>;   // used by health checks
}
```

`Ctx` carries the shared HTTP client (with the network policy applied), the per-provider rate limiter, the cookie jar, credentials from the secret store, and a cancellation token.

`ProviderError` values are typed: `Timeout`, `Blocked` (DNS/SNI/IP symptoms), `ChallengeRequired`, `AuthFailed`, `RateLimited{retry_after}`, `ParseFailed{field}`, `Http{status}`, `Network`. The UI shows a specific message for each and the health tracker counts each separately.

### 6.4 Provider kinds
| Kind | Implementation | v1 contents |
|---|---|---|
| **Native** | Rust code, compiled in | Internet Archive (advancedsearch JSON + `_archive.torrent`), Academic Torrents |
| **Definition** | Generic engine interprets a YAML definition (see §7) | First-party: Linux distro trackers, public-domain sources. Users can import others from definition repos they add |
| **Torznab** | Generic Torznab/Newznab XML client | Any Jackett / Prowlarr / Bitmagnet endpoint the user adds (URL + API key) |

### 6.5 Aggregator
1. Pick providers: the enabled ones that match the categories, not in auto-disabled state, and supporting the query type (text or IMDb).
2. **Fan out** with a `JoinSet`: at most N concurrent searches overall (default 16) and one per host, with a per-provider timeout (default 10 s, configurable).
3. **Stream**: each provider's results are normalized and deduplicated as they arrive, then emitted as events:
   - `provider_started {provider}`
   - `results {provider, items: [MergedResult delta]}`
   - `provider_finished {provider, count, latency_ms}` / `provider_failed {provider, error_kind, message}`
   - `done {total, duration_ms}`
4. **Dedupe** by infohash (normalize base32 → hex, lowercase). If there's no infohash, fall back to the details URL. When merging: keep the longest clean title, use the largest seeder count, and combine the source lists.
5. **Rank** (Relevance sort):
   `score = 0.55·title_match + 0.35·log10(seeders+1)/4 + 0.10·recency`
   - `title_match`: token overlap plus Jaro-Winkler on normalized titles
   - `recency`: decays over roughly 2 years
   - Results with 0 seeders move down but stay visible
6. **Cancellation**: a new search, or the client disconnecting, cancels the search that's still running.
7. **Cache**: identical query and provider set → reuse results for 10 minutes (configurable).

### 6.6 Magnet builder
- If a result only has an infohash, build `magnet:?xt=urn:btih:<hash>&dn=<title>&tr=...` using the **tracker list**.
- The tracker list is a bundled default plus an optional refresh URL the user can set (e.g. a public tracker list), refreshed daily and stored in SQLite.
- Existing magnets are kept, with any missing default trackers added (user setting).

### 6.7 Health tracker
- For each provider, keep a rolling window of the last 20 attempts: success rate, p50/p95 latency, and the most recent error kind.
- **Auto-disable** after 5 failures in a row. Retry in the background with backoff (15 m → 1 h → 6 h). Show the state in the UI.
- The results screen shows a status chip for each provider (✓ 23 results · 1.2 s / ✗ blocked / ⚠ challenge).

---

## 7. Provider definition format

Definitions describe a site as **data only**; they can't contain code. The format is loosely based on Jackett's Cardigann format so porting definitions is easy, but it's simpler and covers JSON and XML as well as HTML.

```yaml
schema: 1
id: example-html
name: Example HTML Index
description: Example of an HTML-scraped index
language: en
type: public                     # public | semi-private | private
links:                           # tried in order; last working one is remembered
  - https://example.org
  - https://mirror.example.net
  - http://exampleabcdefghij.onion   # used only when Tor is enabled
caps:
  categories: { movies: "201", tv: "205", software: "300" }   # our category → site id
  modes: [search]                # search | imdb
  paging: { param: page, start: 1, size: 30 }
login:                           # optional
  method: form                   # form | cookie | apikey
  path: /login
  inputs: { username: "{{ cfg.username }}", password: "{{ cfg.password }}" }
  test: { path: /, selector: "a.logout" }
settings:                        # user-configurable fields shown in UI
  - { name: username, type: text }
  - { name: password, type: password }
search:
  method: get
  path: /search/{{ query.text | urlencode }}/{{ query.page }}/
  params: { cat: "{{ query.categories | map_cats | join(',') }}" }
  response: html                 # html | json | xml
  rows: "table.results > tbody > tr"
  fields:
    title:    { selector: "td.name a:nth-child(2)" }
    details:  { selector: "td.name a:nth-child(2)", attr: href, filters: [urljoin] }
    magnet:   { selector: "a[href^='magnet:']", attr: href, optional: true }
    size:     { selector: "td.size", filters: [parse_size] }
    seeders:  { selector: "td.seeds", filters: [to_int] }
    leechers: { selector: "td.leeches", filters: [to_int] }
    date:     { selector: "td.date", filters: [{ date: "%Y-%m-%d" }] }
download:                        # only if magnet isn't in the row
  from: details
  selector: "a[href^='magnet:']"
  attr: href
```

JSON example (API-style site):

```yaml
search:
  path: /api/search?q={{ query.text | urlencode }}
  response: json
  rows: "$.data[*]"              # JSONPath
  fields:
    title:     { path: "$.name" }
    info_hash: { path: "$.info_hash" }
    size:      { path: "$.size", filters: [to_int] }
    seeders:   { path: "$.seeders", filters: [to_int] }
    leechers:  { path: "$.leechers", filters: [to_int] }
    date:      { path: "$.added", filters: [unix_ts] }
```

**Filters (v1):** `trim`, `lower`, `to_int`, `parse_size` (e.g. "1.4 GiB" → bytes), `date(fmt)`, `relative_date` ("3 days ago"), `unix_ts`, `urljoin`, `regex(pattern, group)`, `replace(a,b)`, `infohash_from_magnet`, `default(v)`.

**Tooling:**
- `definitions/schema/definition.schema.json` provides editor autocomplete and validation.
- `hashlark-cli defs new <id> --type html|json|rss`: creates a commented template ready to fill in.
- `hashlark-cli defs lint <file>`: checks against the schema and checks selectors compile.
- `hashlark-cli defs test <file> --fixture fixtures/<id>/search.html`: runs the definition offline against a saved page.
- `hashlark-cli defs test <file> --live "ubuntu"`: runs a real search and prints the parsed results as a table. If a field fails, it names the field and the selector.
- `hashlark-cli defs test <file> --live "ubuntu" --record`: saves the live response as a fixture so the test can run offline from then on.
- **Hot reload**: in dev mode the core watches the local definitions folder (`notify` crate). Saving a `.yml` reloads that provider in the running app without a restart.

### 7.1 How easy adding and updating is

| Provider kind | Add a new indexer | Fix a broken one | App release needed? |
|---|---|---|---|
| Torznab (Jackett/Prowlarr) | About 30 s: paste URL + API key in the UI | Nothing; Jackett/Prowlarr maintain their indexers | No |
| Definition (YAML), **expected to cover about 90% of indexers** | About 15–60 min: a 30–50 line YAML file | About 5–15 min: usually one domain or a few selectors | No, users get it through repo sync |
| Native (Rust) | About 1–3 days | Code change | Yes |

**Adding a new indexer:** run `defs new`, copy selectors from the browser dev tools (or JSON field names for an API), then `defs test --live`, then `--record` to save a fixture, then either import the `.yml` locally or commit it to a repo.

**Common breakages and their fixes:**

| Symptom | How it's detected | Fix |
|---|---|---|
| Domain moved | Health tracker: unreachable on every mirror | Add the new URL to `links:` (1 line) |
| Layout changed | `ParseFailed{field}` names the missing field | Update 1–3 selectors |
| API field renamed | `ParseFailed{field}` | Change one `path:` |
| Bot challenge added | `ChallengeRequired` | No edit; the user-assisted webview flow handles it (§8.5) |
| Site shut down | Every mirror fails over time | Remove the definition from the repo |

Fixes reach users like this: edit the YAML, increase its `version`, re-sign the repo index, push, and every client picks it up on its next sync (within 24 h, or straight away with "Sync now").

**What definitions can't do** (use a native provider or Jackett instead): pages built entirely by JavaScript that don't expose a JSON API behind them, logins with 2FA or CAPTCHAs, and requests with signed or encrypted parameters.

### 7.2 Definition repositories
- A repo is a URL serving `index.json`:
  ```json
  { "name": "...", "version": 42, "public_key": "ed25519:...",
    "definitions": [{ "id": "...", "version": 7, "url": "defs/x.yml", "sha256": "..." }],
    "signature": "..." }
  ```
- The user adds repos in Settings. The key is trusted the first time it's seen and the user is warned if it changes. The index signature and each file's sha256 must match, otherwise the update is rejected.
- Repos sync on startup plus every 24 h (Android: WorkManager). Users can also import a single `.yml` file.
- The app ships **only** the first-party repo, which contains legal sources.
- Anyone, including community groups, can host their own repo. They maintain their definitions independently of Hashlark releases.

### 7.3 Later: definition editor and Cardigann importer (M8)
- **In-app definition editor**: paste a search URL, the page opens in a webview, the user **clicks elements** to pick title, magnet, size and seeders, and the preview table updates live. It saves a normal `.yml`, so people who don't write code can add indexers.
- **Cardigann importer**: converts Jackett-style YAML definitions into our format (fields that map directly are converted; anything that doesn't is flagged for manual review).
  - **Licensing:** Jackett's definitions are GPL-2.0 licensed. The importer converts files **on the user's own machine**. We don't bundle or redistribute converted Jackett definitions in our repos unless their licence permits it.
  - Definitions are **data loaded at runtime, not code linked into the app**, so a definitions repo can carry its own licence, separate from the app's GPL-3.0-or-later. For example, a community repo of Jackett-derived definitions could stay GPL-2.0.
  - Prowlarr's `Indexers` repo declared **no licence** as of 2026-09-28, which means all rights are reserved. Don't copy from it.

---

## 8. Network layer

All provider traffic goes through a single `NetworkPolicy`-aware client builder in the core.

### 8.1 DNS
- **DoH is on by default**, choosing from Cloudflare, Quad9, Google or a custom URL. It can fall back to system DNS (setting).
- Resolver results are cached according to their TTL.

### 8.2 Mirrors
- Try `links[]` in order. A failure of kind `Blocked`/`Network`/`Timeout` moves on to the next mirror.
- The mirror that last worked is stored in the DB and tried first next time.

### 8.3 Proxy (v1)
- A global or per-provider HTTP / HTTPS / SOCKS5 proxy (`reqwest::Proxy`), with DNS resolved through the proxy when it's SOCKS5h.

### 8.4 Tor (v1.1)
- An embedded `arti-client` exposes a Tor stream connector. Providers can be set to "via Tor" or "Tor only", and `.onion` mirrors are enabled only when Tor is available.
- Alternative: point the SOCKS proxy at an existing Tor or Orbot instance (`127.0.0.1:9050`).

### 8.5 Bot-challenge pages (user-assisted)
- If a response looks like a challenge page, the provider returns `ChallengeRequired`.
- **Desktop**: the UI offers "Open site to continue". This opens a Tauri webview window at the mirror, where the **user** completes the check. Afterwards the app reads the site cookies (Tauri 2 webview cookie API / WebView2 CookieManager; confirm at M5) and the user-agent, and `POST`s them to `/api/v1/providers/{id}/session`.
- **Android**: the same flow using a `WebView` activity and `CookieManager`.
- Cookies expire when the site says so; the flow repeats when needed.
- No automated solving.

### 8.6 Politeness and rate limits
- A per-host token bucket (`governor`), by default 1 request/s with a burst of 3. `Retry-After` is respected.
- A proper `User-Agent`. Response bodies are capped at 5 MB.

---

## 9. HTTP API (`hashlark-server`)

Base: `/api/v1`. JSON everywhere except SSE and Torznab (XML). The OpenAPI 3.1 spec is served at `/api/v1/openapi.json`, and the TS and Kotlin clients are generated from it.

### 9.1 Auth
- **Desktop mode**: binds to `127.0.0.1:0` (random port). A random 256-bit token is created at startup and sent as `Authorization: Bearer <token>`. The server also **checks the `Host` header** (blocks DNS rebinding) and sends **no permissive CORS**.
- **Headless mode**: `--bind 0.0.0.0:8787`, with API keys stored hashed in the DB and optional TLS. See [§9.4](#94-headless-server-mode-ships-in-v10).

### 9.2 Endpoints
| Method | Path | Purpose |
|---|---|---|
| GET | `/search?q=&cat=&providers=&imdb=&page=&sort=` | **SSE stream** of the events in §6.5 |
| POST | `/search` | Same, with the body as the query (non-streaming; returns the merged list when finished) |
| POST | `/resolve` | `{ result_id }` → `{ magnet? , torrent_url? }` |
| GET | `/providers` | List providers with state, health and settings schema |
| POST | `/providers` | Add: `{kind:"torznab", url, api_key}` · `{kind:"definition", yaml}` |
| PATCH | `/providers/{id}` | Enable/disable, mirror order, per-provider network policy, credentials |
| DELETE | `/providers/{id}` | Remove a user-added provider |
| POST | `/providers/{id}/test` | Run a health check now |
| POST | `/providers/{id}/session` | Save cookies and UA from a user-completed challenge |
| GET/POST/DELETE | `/repos` | Manage definition repos; `POST /repos/{id}/sync` |
| GET/PUT | `/settings` | Network (DoH/proxy/Tor), trackers, timeouts, UI prefs |
| GET | `/trackers` · POST `/trackers/refresh` | Tracker list used for magnets |
| GET/DELETE | `/history` | Search history |
| GET/POST/DELETE | `/favorites` | Saved results |
| GET | `/health` | Liveness plus version |
| GET | `/torznab/api?t=caps\|search&q=&cat=&apikey=` | **Torznab-compatible** endpoint for Sonarr/Radarr |

### 9.3 SSE event example
```
event: results
data: {"search_id":"s_81f","provider":"internet-archive","items":[{ "id":"r_1a2", "title":"Ubuntu 24.04 LTS", "size_bytes":6114770944, "seeders":412, "sources":["internet-archive"], "score":0.91, "needs_resolve":false, "magnet":"magnet:?xt=..." }]}

event: provider_failed
data: {"search_id":"s_81f","provider":"example-html","error_kind":"blocked","message":"DNS resolution blocked; try enabling a proxy"}
```

### 9.4 Headless server mode (ships in v1.0)
- **Binaries**: `hashlark-server` for Windows, macOS and Linux (x86_64 and arm64), plus a **Docker image** (`ghcr.io/<org>/hashlark`, multi-arch, so it runs on NAS boxes and Raspberry Pi).
- **Config**: `hashlark.toml` covering bind address, data directory, TLS certificate and key, and log level. Every option can also be set with an environment variable (`HASHLARK_*`).
- **Web UI**: the server serves the **same Svelte build** as the desktop app at `/`. On first open the browser shows a login screen that asks for an API key. Magnet links open through the browser, which hands them to the OS of the device you're browsing on.
- **Auth**: the first run prints a one-time admin API key; more keys can be created and revoked in Settings, and are stored hashed. Login attempts are rate-limited.
- **Running as a service**: example files for a systemd unit, a Windows service (via `sc`/NSSM) and a docker-compose setup, in `docs/deploy/`.
- **Torznab**: `/torznab/api` lets Sonarr, Radarr and Prowlarr use Hashlark as an indexer.

---

## 10. Desktop app (Tauri + Svelte)

### 10.1 How the processes fit together
- A single process: the Tauri app starts `hashlark-server` on a Tokio task during `setup()`.
- The UI makes one Tauri command call, `get_api_endpoint()`, which returns `{ base_url, token }`. **After that, everything goes over the HTTP API**, the same way it would for a remote client.
- Only a few desktop-only features are Tauri commands: opening a magnet with the OS handler, opening the challenge webview window, choosing a save location for `.torrent` files, tray and notifications.

### 10.2 Front-end stack
- Svelte 5 (runes) with SvelteKit `adapter-static`, TypeScript, Vite, pnpm.
- Styling: Tailwind CSS 4 plus a small set of our own components. Light and dark themes follow the OS.
- API client: `openapi-typescript` + `openapi-fetch`. SSE is read with `fetch` streaming, because `EventSource` can't send an auth header.
- State: Svelte stores per feature (search, providers, settings).
- Tests: Vitest + Testing Library; Playwright for end-to-end runs against the UI served over a real headless server.

### 10.3 Screens
1. **Search**: query bar, category chips, provider filter, sort. Results list with virtual scrolling showing title, size, S/L, age and source badges. A **provider status bar** (per-provider chips). Row actions: **Open magnet**, **Copy magnet**, **Download .torrent**, **Details**, **Favourite**.
2. **Result details**: all sources, trackers, the full magnet, and a link to the details page.
3. **Providers**: list with health, enable/disable, "Add provider" (Torznab / import YAML / add repo), per-provider settings (credentials, mirrors, network policy), "Test".
4. **Repositories**: added repos, last sync, trust-key fingerprint.
5. **Settings**: Network (DoH provider, proxy, Tor), Magnets (tracker list, append defaults), Downloads (default folder for `.torrent` files), Search (timeouts, concurrency, cache TTL), Appearance, About. In headless mode there is also API keys.
6. **History and favourites.**
7. **First-run**: a short welcome, a legal notice, and the default legal providers already enabled.

### 10.4 Handing downloads to a client (decision D8: OS handler only)
- **Magnet**: `tauri-plugin-opener` passes the `magnet:` link to the OS, which opens whichever client is registered (qBittorrent, Transmission, Deluge and so on).
- **No client registered**: show a clear message ("No app is set to open magnet links") with **Copy magnet** and a link to help on installing a client.
- **.torrent**: download it to the configured folder, then open it with the OS default app.
- **Copy magnet** is always available as a fallback.
- Integrations with specific clients' APIs (e.g. qBittorrent Web API) are **not** planned; this can be revisited after v1.0 if needed.

### 10.5 Packaging
- **Windows**: NSIS installer and MSI through the Tauri bundler. WebView2 is preinstalled on Windows 11; the installer bootstraps it on Windows 10. Plan for **code signing** (an Authenticode certificate, or Azure Trusted Signing) to reduce SmartScreen warnings.
- **macOS**: `.dmg`, notarized (needs an Apple Developer account).
- **Linux**: AppImage and `.deb`.
- **Auto-update**: `tauri-plugin-updater` with signed update manifests on GitHub Releases.

---

## 11. Android app (Phase 2)

### 11.1 Integration
- `hashlark-ffi` wraps the core with **UniFFI** (proc-macro mode) and generates Kotlin bindings.
- Built with `cargo-ndk` for `arm64-v8a`, `armeabi-v7a` and `x86_64`, and packaged as an AAR module inside the Gradle project.
- Async: UniFFI async functions map to Kotlin `suspend` functions. Streaming search uses a callback interface:
  ```kotlin
  interface SearchListener {
      fun onResults(provider: String, items: List<MergedResult>)
      fun onProviderStatus(status: ProviderStatus)
      fun onDone(summary: SearchSummary)
  }
  ```
  A small Kotlin wrapper turns this into `Flow<SearchEvent>`.
- Secrets: the core defines a `SecretStore` trait, implemented in Kotlin with Android Keystore / EncryptedSharedPreferences and passed in when the core starts.

### 11.2 App stack
- Kotlin, Jetpack Compose, Material 3, a ViewModel for each screen, and Hilt (or manual DI; keep it simple).
- WorkManager for definition-repo sync and tracker-list refresh.
- The same screens as desktop, laid out for phones.
- Magnet handoff: `Intent(ACTION_VIEW, Uri.parse(magnet))`. If nothing handles it, fall back to copy and share.
- Bot challenges: an in-app `WebView` activity, then `CookieManager`, then the core session API.

### 11.3 Distribution: **OPEN**, to be decided before A3
- Options: GitHub Releases, F-Droid, Obtainium and/or Google Play. Google Play is only realistic with legal-only defaults, and review risk is still high.
- This is deliberately left undecided while Phase 1 (desktop) is the focus.
- Target: APK under about 20 MB per ABI split.

---

## 12. Storage

SQLite in the OS app-data directory (`directories`), e.g. `%APPDATA%\Hashlark\hashlark.db` on Windows. Migrations are run by `sqlx migrate`.

| Table | Key columns |
|---|---|
| `providers` | id, kind, name, enabled, definition_id?, config_json, network_policy_json, created_at |
| `provider_mirrors` | provider_id, url, position, last_ok_at, last_error |
| `provider_health` | provider_id, ts, ok, latency_ms, error_kind |
| `provider_sessions` | provider_id, cookies_json, user_agent, expires_at |
| `definitions` | id, repo_id, version, sha256, yaml, updated_at |
| `definition_repos` | id, url, name, public_key, last_sync_at, last_version |
| `settings` | key, value_json |
| `trackers` | url, source, added_at |
| `search_history` | id, query_json, ts, result_count |
| `favorites` | result_id, snapshot_json, ts |
| `result_cache` | cache_key, payload, expires_at |

Credentials are **never** stored in SQLite. They go in the OS keychain (`keyring` crate: Windows Credential Manager / macOS Keychain / Secret Service) or the Android Keystore.

---

## 13. Security

| Threat | Mitigation |
|---|---|
| A website calling the local API (CSRF, DNS rebinding) | Random port, bearer token, strict `Host` check, no CORS |
| A malicious definition | Definitions are data only; templates are sandboxed (minijinja without file or env access); response size and row count limits; regexes compiled with size limits |
| A tampered repo update | Ed25519-signed index, per-file sha256, trust on first use with a warning if the key changes |
| Stolen credentials | OS keychain / Android Keystore; values hidden in logs (`tracing` redaction) |
| XSS through result titles in the webview | Svelte escapes by default; no `{@html}` on provider data; strict CSP in the Tauri config |
| A malicious `.torrent` download | We never parse or run it; we only save it or hand it off |
| Supply chain | `cargo deny` (licenses and advisories), `pnpm audit`, Dependabot, pinned lockfiles |

---

## 14. Legal and distribution posture

- Hashlark is a **neutral search tool**. It hosts no content and does not proxy traffic through any server we run.
- **The app ships enabled only with legal sources** (Internet Archive, Academic Torrents, Linux distributions, public-domain collections). Anything else is added by the user (Torznab endpoints, definition files or third-party repos).
- The first-party definitions repo contains only legal sources.
- First-run notice: users are responsible for complying with the law and copyright where they live.
- DoH, proxy and Tor are presented as **privacy and network settings**, not "unblocking".
- **No telemetry** (D10). The app sends no analytics, crash reports or usage data. The only outgoing requests are:
  1. searches and resolves sent to the providers the user enabled;
  2. definition-repo sync;
  3. tracker-list refresh (if configured);
  4. the update check against GitHub Releases, which can be turned off in Settings.

  The privacy notice in About states this.
- **Licence: GPL-3.0-or-later** (D11). There's a `LICENSE` file at the repo root, SPDX headers in source files, and a `cargo deny` licence allow-list (MIT, Apache-2.0, BSD, ISC, MPL-2.0, Zlib, Unicode, GPL-3.0-compatible). The first-party definitions repo uses the same licence.
- Before publishing to any store, have the listing and the default provider set reviewed against that store's policy.

---

## 15. Testing strategy

| Layer | Approach |
|---|---|
| Core units | Filters, size and date parsing, infohash normalization, dedupe and ranking (property tests with `proptest` for parsers) |
| Providers | **Fixture tests**: recorded responses in `fixtures/<provider>/`, replayed through a mock HTTP server (`wiremock`). Every definition must have at least one fixture test |
| Definition engine | Golden tests: definition + fixture → expected `SearchResult` JSON (`insta` snapshots) |
| Network | Tests for mirror fallback, timeouts, retry-after and challenge detection with `wiremock` |
| API | `axum` tests via `tower::ServiceExt`; SSE stream checks; OpenAPI spec snapshot |
| Desktop UI | Vitest component tests; Playwright end-to-end against the Svelte app with a headless server and fixture providers |
| Android | JUnit tests for the Kotlin wrapper; Compose UI tests; FFI smoke test on an emulator in CI |
| Live canary | A nightly CI job runs `hashlark-cli health` against the first-party definitions and opens an issue on failures (never blocks PRs) |

---

## 16. CI/CD and releases

- **GitHub Actions**:
  - `ci.yml` (every PR): `cargo fmt --check`, `clippy -D warnings`, `cargo test`, `cargo deny`, `pnpm lint && pnpm test`, and `defs lint` over `definitions/`. Runs on Windows, macOS and Ubuntu.
  - `release.yml` (tag `v*`): `tauri-action` builds and signs installers for each OS, publishes a GitHub Release and the updater manifest. It also builds the `hashlark-server` binaries and pushes the multi-arch Docker image to GHCR.
  - `android.yml` (Phase 2): `cargo-ndk` + Gradle builds a signed APK/AAB.
  - `canary.yml` (nightly): the live provider health check.
- Versioning: SemVer. A shared version for core, server and desktop; definitions are versioned separately.
- Conventional commits with a generated changelog.

---

## 17. Milestones

Estimates assume **one developer working part time** (about 15–20 h/week). Adjust as needed.

### Phase 1: Desktop

| M | Name | Scope | Exit criteria | Est. |
|---|---|---|---|---|
| **M0** ✅ | Skeleton | Cargo workspace, crates, CI, `tracing`, error types, SQLite + migrations, ADRs for D1–D11, `LICENSE` (GPL-3.0-or-later), register the domain, GitHub org and crates.io names | `cargo test` passes in CI on 3 OSes | 1 wk |
| **M1** ✅ | Core search | Domain model, `SearchProvider` trait, **Internet Archive** native provider, aggregator (fan-out, dedupe, rank, cancel), magnet builder, `hashlark-cli search` | `hashlark-cli search "ubuntu"` streams ranked results | 2 wks |
| **M2** ✅ | API | `hashlark-server`: `/search` SSE, `/resolve`, `/providers`, `/settings`, token auth, Host check, OpenAPI | Can search with `curl`; OpenAPI spec produced | 1–2 wks |
| **M3** ✅ | Desktop MVP | Tauri shell running the server in-process, Svelte search screen with streaming results, provider status bar, open/copy magnet, settings basics | Installable Windows build; search → click → magnet opens in qBittorrent | 2–3 wks |
| **M4** ✅ | Definition engine | YAML schema, HTML/JSON/XML engines, filters, login (form/cookie/apikey), `defs new/lint/test` (incl. `--record`), **hot reload**, fixtures, first-party legal definitions, import `.yml` | 3+ first-party definitions pass fixture tests; editing a `.yml` updates the running app without a restart | 3 wks |
| **M5** ✅ | Torznab + network | Torznab client, Torznab server endpoint, **DoH**, mirror fallback, proxy, rate limiting, health tracker + auto-disable, challenge webview flow | Jackett/Prowlarr work as sources; DoH on by default; health chips in UI | 2–3 wks |
| **M6** ✅ | Repos + polish | Signed definition repos + sync, history, favourites, "no magnet handler" fallback, keychain credentials, first-run, themes, accessibility pass | Feature-complete desktop v1 | 2 wks |
| **M6.5** ✅ | Headless server | `hashlark.toml` + env config, LAN bind + TLS, API-key management, login screen in the web UI, Svelte build served by axum, Docker image (multi-arch), service examples in `docs/deploy/` | `docker run` → open a browser on another device → search → magnet opens | 1–2 wks |
| **M7** ✅ | Release v1.0 | Code signing, auto-update (with an off switch), macOS/Linux builds, server binaries + Docker image published, docs (user guide, definition-format reference, deploy guide), canary job | Signed v1.0 desktop + server on GitHub Releases / GHCR | 1 wk |
| **v1.1** ✅ | Tor | Embedded Arti, per-provider Tor policy, `.onion` mirrors | Search works in "Tor only" mode | 2 wks |
| **M8** ✅ | Authoring tools | In-app **definition editor** (click-to-pick selectors, live preview), **Cardigann importer** (see §7.3) | Someone who doesn't code can build a working definition for a simple HTML site in the UI | 3 wks |

**Phase 1 total (through M7): about 16–20 weeks part time.** v1.1 and M8 come after that.

### Phase 2: Android (standalone)

| M | Name | Scope | Exit criteria | Est. |
|---|---|---|---|---|
| **A1** | FFI | `hashlark-ffi` with UniFFI, callback streaming, `SecretStore` bridge, `cargo-ndk` build, AAR | Kotlin unit test runs a search through the FFI on an emulator | 2 wks |
| **A2** | App MVP | Compose screens (search, providers, settings), Flow wrapper, magnet intent, WorkManager sync | Search → magnet opens in an Android torrent client | 3–4 wks |
| **A3** | Parity + release | Challenge WebView, repos, history and favourites, Tor, signing, F-Droid metadata, store decision | v1.0 APK released | 2–3 wks |

### Phase 3 (optional)
- Built-in downloader (`librqbit`) with streaming preview.
- Remote mode: the Android app as a client of a headless server.
- Community definitions tooling (web-based definition tester).

---

## 18. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Sites change their HTML often | Providers break | Definitions updated over the air, fixture tests, nightly canary, auto-disable + clear UI state |
| Sites disappear or move domains | Providers break | Mirror lists, repo updates, health tracking |
| Bot-challenge pages spread | Fewer usable HTML providers | Prefer JSON/Torznab sources; user-assisted webview flow; Jackett/Prowlarr handle their own |
| ISP / country blocking | Providers unreachable | DoH on by default, mirrors, proxy, Tor (v1.1) |
| Store rejection / legal complaints | Distribution blocked | Legal-only defaults, user-added providers, neutral branding, GitHub/F-Droid first |
| Rust learning curve | Slower M1–M4 | Most site logic lives in YAML; keep the core API small; ADRs keep decisions clear |
| UniFFI async/streaming quirks | Android delays | Spike FFI streaming early (a short spike during M2) |
| Windows SmartScreen / AV flags | Users can't install easily | Code signing, reproducible builds, no packers |
| Webview cookie API gaps in Tauri | Challenge flow harder | Confirm during M5; fall back to a WebView2 COM call on Windows |
| Headless server exposed to the internet by mistake | Unauthorised use | Required API key, login rate limit, LAN-only default bind in docs, TLS support, warning when bound to `0.0.0.0` without TLS |
| Licence conflicts (GPL-2.0-only definitions, incompatible dependencies) | Can't distribute legally | `cargo deny` allow-list; Cardigann importer converts on the user's machine only (§7.3) |

---

## 19. Decision log and open questions

### Decided (2026-09-28)
| # | Question | Decision | See |
|---|---|---|---|
| 1 | Product name | **Hashlark** | §1 |
| 2 | Core / UI / Android approach | Rust core · Svelte (Tauri) · standalone Android | D1–D3 |
| 3 | Download handoff | **OS handler only** (magnet → registered app; `.torrent` → save/open) | D8, §10.4 |
| 4 | Headless server mode | **Ships in v1.0**, with web UI and Docker image | D9, §9.4, M6.5 |
| 5 | Telemetry | **None** | D10, §14 |
| 6 | Licence | **GPL-3.0-or-later** | D11, §14 |
| 7 | Authoring tools | `defs new` + hot reload in M4; definition editor + Cardigann importer in M8 | §7, §7.3 |

### Open
1. **Android store strategy** (GitHub / F-Droid / Obtainium / Google Play). **Deferred until Phase 2**; decide before A3.

---

## Implementation status

Updated 2026-09-29. Phase 1 (desktop and headless server) is implemented and tested locally; nothing has been released yet.

**Verified**
- 135 Rust tests and 13 UI tests pass.
- clippy (`-D warnings`), `cargo deny` and definition lint pass.
- Live checks from Windows:
  - search across all four built-in providers (also with DoH only);
  - SSE streaming;
  - the desktop app starting and serving its UI;
  - the headless server: sign-in with an API key, the served web UI, favourites;
  - signed repository add and sync;
  - hot reload;
  - the definition editor (pick from a live page, then preview);
  - the Windows NSIS installer (8.3 MB) with its updater signature.

**Not yet verified / follow-ups**
- **CI and release workflows** have not run (the repo has no GitHub remote yet). The Docker image hasn't been built (Docker isn't installed on the dev machine). macOS and Linux builds haven't been tried.
- **Built-in Tor** starts, but couldn't bootstrap from the development network (stuck fetching a consensus). Add Tor **bridges / pluggable transports** (Arti `pt-client`, e.g. obfs4 or Snowflake) for networks that block Tor. External Tor mode is unaffected.
- **Browser-check flow** (`open_challenge` / `finish_challenge`): the engine side is tested; the desktop window flow hasn't been exercised against a real challenge page.
- **Code signing:** Windows (Authenticode / Azure Trusted Signing) and macOS notarization need accounts and secrets (see `docs/releasing.md`).
- **Registrations:** domain, GitHub org and crates.io name (§1).
- **Phase 2 (Android)** hasn't started. The core is ready for UniFFI; `Engine` is the intended FFI surface.
