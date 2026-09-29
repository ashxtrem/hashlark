// SPDX-License-Identifier: GPL-3.0-or-later

//! Starter definitions for `hashlark-cli defs new`.

/// Kinds of starter template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarterKind {
    Html,
    Json,
    Rss,
}

/// A commented starter definition with the given id.
pub fn starter(kind: StarterKind, id: &str) -> String {
    let body = match kind {
        StarterKind::Html => HTML,
        StarterKind::Json => JSON,
        StarterKind::Rss => RSS,
    };
    body.replace("__ID__", id)
}

const HTML: &str = r#"# Hashlark provider definition. Reference: docs/definition-format.md
schema: 1
id: __ID__
name: __ID__
description: Describe the site in one sentence.
version: 1
links:
  - https://example.org/        # mirrors are tried in order
caps:
  categories:                   # Hashlark category -> the site's category id
    movies: "1"
    software: "2"
  paging: { start: 1 }
search:
  # Templates can use: query.text, query.page, query.offset,
  # query.categories, query.site_categories, query.imdb, cfg.<setting>
  path: /search/{{ query.text | urlencode }}/{{ query.page }}/
  response: html
  rows: "table.results > tbody > tr"      # one element per result
  fields:
    title:    { selector: "a.name" }
    details:  { selector: "a.name", attr: href }
    magnet:   { selector: "a[href^='magnet:']", attr: href, optional: true }
    size:     { selector: "td.size" }              # "1.4 GiB" is understood
    seeders:  { selector: "td.seeds" }
    leechers: { selector: "td.leeches" }
    date:     { selector: "td.date", optional: true }
# If the magnet is only on the details page, remove `magnet` above and use:
# download:
#   selector: "a[href^='magnet:']"
test:
  query: ubuntu
"#;

const JSON: &str = r#"# Hashlark provider definition. Reference: docs/definition-format.md
schema: 1
id: __ID__
name: __ID__
description: Describe the site in one sentence.
version: 1
links:
  - https://api.example.org/
search:
  path: /v1/search
  params:
    q: "{{ query.text }}"
    page: "{{ query.page }}"
  response: json
  rows: "$.results[*]"          # JSONPath; inside fields `$` is one result
  fields:
    title:     { path: "$.name" }
    info_hash: { path: "$.info_hash" }
    size:      { path: "$.size_bytes" }
    seeders:   { path: "$.seeders" }
    leechers:  { path: "$.leechers" }
    date:      { path: "$.added", filters: [unix_ts] }
    details:   { path: "$.url", optional: true }
test:
  query: ubuntu
"#;

const RSS: &str = r#"# Hashlark provider definition. Reference: docs/definition-format.md
schema: 1
id: __ID__
name: __ID__
description: Describe the site in one sentence.
version: 1
links:
  - https://example.org/
search:
  path: /feed/rss.xml
  response: xml
  # The feed doesn't depend on the query: fetch it every few hours and
  # filter locally. Remove this for feeds that take a search parameter.
  static_feed: { ttl_hours: 6 }
  rows: "//item"
  fields:
    title:    { path: "title" }
    download: { path: "enclosure/@url" }     # or: { path: "link" }
    size:     { path: "enclosure/@length", optional: true }
    date:     { path: "pubDate", optional: true }
    details:  { path: "guid", optional: true }
test:
  query: linux
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::load;

    #[test]
    fn starters_compile() {
        for kind in [StarterKind::Html, StarterKind::Json, StarterKind::Rss] {
            let yaml = starter(kind, "my-site");
            let compiled = load(&yaml).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert_eq!(compiled.spec.id, "my-site");
        }
    }
}
