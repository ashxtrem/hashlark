# Provider definition format (schema 1)

A **definition** describes how to search one site, as YAML. Hashlark's engine reads it; a definition can't contain code, so importing one is safe.

Most sites can be added in 15–60 minutes, and fixing a site that changed usually means editing a selector or a link. See [PLAN.md §7](PLAN.md#7-provider-definition-format) for the design.

## Quick start

```bash
hashlark-cli defs new my-site --type html        # or json / rss
hashlark-cli defs test my-site.yml --live         # fetch and show parsed results
hashlark-cli defs test my-site.yml --live --record   # also save the page as a test fixture
hashlark-cli defs lint my-site.yml                # check for errors
hashlark-cli import my-site.yml                   # add it to your Hashlark
```

**Live reload while editing:** start the desktop app or server with `HASHLARK_DEFINITIONS_DIR=<folder>`, or run `hashlark-server --definitions-dir <folder>`. Every `.yml` in the folder is loaded, and reloaded each time you save.

**Editor autocomplete:** point your YAML editor at the schema. With the VS Code YAML extension, put this line at the top of the file:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/ashxtrem/hashlark/main/definitions/schema/definition.schema.json
```

Regenerate the schema with `hashlark-cli defs schema > definitions/schema/definition.schema.json`.

## Top level

| Key | Required | Meaning |
|---|---|---|
| `schema` | yes | Always `1`. |
| `id` | yes | 2–64 chars: lowercase letters, digits, `-`. Must be unique. |
| `name` | yes | Display name. |
| `description` | | One sentence shown under the name. |
| `language` | | Site language, default `en`. |
| `type` | | `public` (default), `semi_private` or `private`. |
| `version` | | Integer, default `1`. **Increase it on every change** so repositories can deliver the update. |
| `links` | yes | Base URLs, tried in order. If one fails (network error, timeout, 5xx, 403/404, block or browser check), the next is tried, and the one that worked is tried first next time. `.onion` links are used only when Tor is enabled. |
| `caps` | | What the site supports. See below. |
| `settings` | | Values the user fills in. See below. |
| `headers` | | Extra headers on every request, e.g. `Referer`. Values are templates. |
| `login` | | How to log in. See below. |
| `search` | yes | How to search and read results. See below. |
| `download` | | How to find the download link on a details page. |
| `test` | | `query:` used by **Test** and health checks (default `linux`). |

## `caps`

```yaml
caps:
  categories:          # Hashlark category -> the site's category id
    movies: "201"
    tv: "205"
    software: "300"
  modes: [search]      # add `imdb` if a template uses {{ query.imdb }}
  paging:
    start: 1           # the site's number for the first page
    size: 50           # results per page, for offset paging ({{ query.offset }})
```

- Hashlark's categories are `movies`, `tv`, `music`, `books`, `software`, `games`, `anime` and `other`.
- A definition with **no** categories is searched for every category.
- One with categories is searched only when the user picks one of them, or picks none.

## `settings`

```yaml
settings:
  - { name: username, label: "User name", required: true }
  - { name: password, type: password, required: true }
  - { name: sort, type: select, options: { seeders: "Most seeders", date: "Newest" }, default: seeders }
```

- Types are `text`, `password`, `checkbox` and `select`.
- Templates read settings as `{{ cfg.username }}`.
- **Passwords go to the OS keychain**, never to Hashlark's database.
- A `required` setting that isn't filled in stops the search with a clear message.

## `login`

```yaml
login:
  method: form                 # post a form once; the session cookie is kept
  path: /login.php
  inputs:
    username: "{{ cfg.username }}"
    password: "{{ cfg.password }}"
  test:                        # optional: prove the login worked
    path: /index.php
    selector: "a[href*='logout']"
```

`method: cookie` sends the `cookie` setting as the `Cookie` header on every request. The definition must then have a setting named `cookie`.

If a logged-in search stops parsing (for example because the session expired), Hashlark logs in once more and retries.

## `search`

```yaml
search:
  method: get                  # or post (params become form fields)
  path: /search/{{ query.text | urlencode }}/{{ query.page }}/
  params:                      # query string (GET) or form (POST); values are templates
    cat: "{{ query.site_categories | join(',') }}"
  headers: {}
  response: html               # html | json | xml
  rows: "table#results > tbody > tr"
  fields: { ... }              # see below
  error_selector: ".error"     # html only: the site reported an error
  max_rows: 200                # default 1000
  client_filter: false         # keep only rows whose title has every query word
  static_feed:                 # the page doesn't depend on the query (a full feed)
    ttl_hours: 6               # fetched at most this often, filtered locally
```

- `path` may also be a full URL.
- A `static_feed` is fetched once per `ttl_hours` and filtered locally. Use it for sites that publish one RSS/XML file of everything (and ask you not to scrape).

### Rows

| `response` | `rows` is | Example |
|---|---|---|
| `html` | a CSS selector | `tr.result`, `div.torrent-row` |
| `json` | a JSONPath | `$.data[*]`, `$.torrents[*]` |
| `xml` | an XML path (below) | `//item`, `channel/item` |

Rows without a `title` are skipped (header rows, ads), so a broad selector like `tr` is fine.

### Fields

| Field | Meaning |
|---|---|
| `title` | **Required.** |
| `details` | Link to the result's page (relative links are resolved). |
| `download` | Link to the `.torrent` (a magnet link here is also accepted). |
| `magnet` | Magnet link. |
| `info_hash` | 40-char hex or 32-char base32. With it, Hashlark builds the magnet itself and merges duplicates across sites. |
| `size` | Bytes, or text like `1.4 GiB`, `700 MB`, `1,234.5 KB`, `1,5 GB`. |
| `seeders`, `leechers` | Numbers; text like `1,204` works. |
| `date` | RFC 3339, RFC 2822 (RSS `pubDate`), Unix seconds or ms, `YYYY-MM-DD[ HH:MM[:SS]]`, `Feb 15, 2026`, `15 Feb 2026`, `15.02.2026`, or relative text such as `3 days ago` or `yesterday`. |
| `category` | The site's category id (mapped back through `caps.categories`) or a Hashlark category name. |

At least one way to download is required: `magnet`, `download` or `info_hash`, or `details` together with a `download:` section. That section tells Hashlark to open the details page when the user picks the result.

Each field takes **one** source:

```yaml
title:    { selector: "a.name" }                 # html: text of the first match in the row
details:  { selector: "a.name", attr: href }     # html: an attribute
size:     { path: "$.size" }                     # json: JSONPath inside the row ($ = the row)
seeders:  { path: "torznab:attr[@name='seeders']/@value" }   # xml path
magnet:   { text: "magnet:?xt=urn:btih:{{ row.info_hash }}" } # template
title:    {}                                     # html: the row's own text
```

Add `optional: true` to fields that may be missing. When a non-optional field is missing from **every** row, the provider reports `parse_failed` and names the field. That usually means the site changed its layout.

### XML paths

This is a small subset of XPath:

- `channel/item`: child steps (also `rss/channel/item`)
- `//item`: any descendant named `item`
- `enclosure/@url`: an attribute of the selected element
- `torznab:attr[@name='seeders']/@value`: a predicate on an attribute
- `*`: any element; `.`: the element itself

Names may use the prefix from the document (`torznab:attr`). A name without a prefix matches regardless of namespace.

## Filters

Filters are applied in order: `filters: [trim, { regex: "id=(\\d+)" }]`.

| Filter | Example | Result |
|---|---|---|
| `trim`, `lower`, `upper` | | |
| `to_int` | `1,234 seeds` | `1234` |
| `parse_size` | `1.4 GiB` | bytes |
| `date` or `{ date: "%d.%m.%Y" }` | strftime format, or `rfc3339` / `rfc2822` | RFC 3339 |
| `relative_date` | `2 hours ago` | RFC 3339 |
| `unix_ts` | `1700000000` | RFC 3339 |
| `urljoin` | `../x.torrent` | absolute URL |
| `{ regex: "pattern" }` / `{ regex: ["pattern", "2"] }` | | capture group 1 (or N) |
| `{ replace: ["from", "to"] }` | | plain text replace |
| `{ re_replace: ["\\s+", " "] }` | | regex replace |
| `{ prepend: "x" }`, `{ append: "x" }` | | |
| `{ split: ["|", "-1"] }` | `a | b | c` | `c` (negative counts from the end) |
| `{ default: "0" }` | missing / empty | `0` |
| `infohash_from_magnet` | magnet | 40-char hex |
| `{ querystring: "id" }` | `/dl.php?id=42` | `42` |

## Templates

Templates use [MiniJinja](https://docs.rs/minijinja) syntax (`{{ value | filter }}`). These values are available:

| Value | Meaning |
|---|---|
| `query.text` | The search text. Use `| urlencode` inside paths; `params` are encoded automatically. |
| `query.page` | The site's page number: `caps.paging.start` + page − 1. |
| `query.offset` | (page − 1) × `caps.paging.size`. |
| `query.categories` | Hashlark category names requested. |
| `query.site_categories` | The site's ids for them, e.g. `{{ query.site_categories | join(',') }}`. |
| `query.imdb` | IMDb id, when searching by IMDb. |
| `cfg.<name>` | Setting values. |
| `row.<field>` | In a field's `text`: fields extracted earlier in the row, in the order title, details, download, magnet, info_hash, size, seeders, leechers, date, category. |
| `base` | The link in use. |

## `download` (details page)

```yaml
download:
  selector: "a[href^='magnet:'], a.download-torrent"
  attr: href          # default
  filters: []
```

The details page is fetched only when the user opens the result. Its link can be a magnet or a `.torrent` URL.

## Complete examples

The definitions that ship with Hashlark are in [`definitions/builtin/`](../definitions/builtin/):

- [`linuxtracker.yml`](../definitions/builtin/linuxtracker.yml): HTML; the infohash comes from the details link.
- [`academic-torrents.yml`](../definitions/builtin/academic-torrents.yml): XML static feed.
- [`foss-torrents.yml`](../definitions/builtin/foss-torrents.yml): RSS static feed with `.torrent` links.

## Testing and fixtures

- Every definition in this repository must have a recorded page in `fixtures/<id>/`, and a test that parses it offline (see `crates/hashlark-core/src/definition/tests.rs`).
- Record a page with `defs test --live --record`.
- Replay it with `defs test <file> --fixture fixtures/<id>/search.html --query <text>`.
