// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::sync::Arc;

use url::Url;
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::error::ProviderError;
use crate::model::{Category, DownloadTarget, SearchQuery};
use crate::provider::{DefinitionProvider, MirrorMemory, ProviderCtx, SearchProvider};

fn builtin(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../definitions/builtin/{name}.yml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn fixture(path: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn cfg() -> BTreeMap<String, String> {
    BTreeMap::new()
}

// ----- first-party definitions against recorded pages ----------------------

#[test]
fn linuxtracker_fixture() {
    let provider = DefinitionProvider::from_yaml(&builtin("linuxtracker")).unwrap();
    let url: Url = "https://linuxtracker.org/index.php?page=torrents&search=ubuntu"
        .parse()
        .unwrap();
    let results = provider
        .parse_page(
            &fixture("linuxtracker/search.html"),
            &url,
            &SearchQuery::text("ubuntu"),
            &cfg(),
        )
        .unwrap();
    assert!(results.len() >= 10, "got {}", results.len());
    let first = &results[0];
    assert!(
        first.title.to_lowercase().contains("ubuntu"),
        "{}",
        first.title
    );
    assert!(first.info_hash.is_some());
    assert!(first.size_bytes.unwrap() > 1_000_000);
    assert!(first.seeders.is_some() && first.leechers.is_some());
    assert!(first.published.is_some());
    assert!(
        first
            .details_url
            .as_ref()
            .unwrap()
            .as_str()
            .starts_with("https://linuxtracker.org/torrents/")
    );
    assert!(!first.needs_resolve);
}

#[test]
fn academic_torrents_fixture() {
    let provider = DefinitionProvider::from_yaml(&builtin("academic-torrents")).unwrap();
    let url: Url = "https://academictorrents.com/database.xml".parse().unwrap();
    let body = fixture("academic-torrents/search.xml");

    let all = provider
        .parse_page(&body, &url, &SearchQuery::text("mit"), &cfg())
        .unwrap();
    assert!(!all.is_empty());
    assert!(
        all.iter()
            .all(|r| crate::ranking::tokens(&r.title).contains(&"mit".to_owned()))
    );
    assert!(all.iter().all(|r| r.info_hash.is_some()));
    assert!(all[0].size_bytes.is_some());

    let none = provider
        .parse_page(&body, &url, &SearchQuery::text("zzqqxx"), &cfg())
        .unwrap();
    assert!(none.is_empty(), "static feeds are filtered by the query");
}

#[test]
fn foss_torrents_fixture() {
    let provider = DefinitionProvider::from_yaml(&builtin("foss-torrents")).unwrap();
    let url: Url = "https://fosstorrents.com/feed/torrents.xml"
        .parse()
        .unwrap();
    let results = provider
        .parse_page(
            &fixture("foss-torrents/search.xml"),
            &url,
            &SearchQuery::text("almalinux"),
            &cfg(),
        )
        .unwrap();
    assert!(results.len() >= 3);
    let first = &results[0];
    assert!(
        first
            .torrent_url
            .as_ref()
            .unwrap()
            .as_str()
            .ends_with(".torrent")
    );
    assert!(first.published.is_some());
    assert_eq!(
        provider.info().categories,
        vec![Category::Software, Category::Games]
    );
}

// ----- engine behaviour against a mock server -----------------------------

const HTML_DEF: &str = r#"
schema: 1
id: mock-html
name: Mock HTML
links: [__BASE__]
caps:
  categories: { movies: "10", tv: "20" }
  paging: { start: 0 }
search:
  path: "/search/{{ query.text | urlencode }}/{{ query.page }}"
  params: { cat: "{{ query.site_categories | join(',') }}" }
  response: html
  rows: "tr.row"
  error_selector: ".error"
  fields:
    title: { selector: "a.name" }
    details: { selector: "a.name", attr: href }
    magnet: { selector: "a.magnet", attr: href, optional: true }
    size: { selector: ".size" }
    seeders: { selector: ".s", filters: [to_int] }
    category: { selector: ".cat", optional: true }
download:
  selector: "a.dl"
"#;

const PAGE: &str = r#"<table>
<tr class="row"><td><a class="name" href="/t/1">Big Buck Bunny</a></td>
  <td><a class="magnet" href="magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=bbb">m</a></td>
  <td class="size">263 MiB</td><td class="s">1,204</td><td class="cat">10</td></tr>
<tr class="row"><td><a class="name" href="/t/2">Sintel</a></td>
  <td class="size">1.1 GB</td><td class="s">7</td><td class="cat">20</td></tr>
<tr class="row"><td>no title here</td></tr>
</table>"#;

fn def_for(server: &MockServer, yaml: &str) -> DefinitionProvider {
    DefinitionProvider::from_yaml(&yaml.replace("__BASE__", &server.uri())).unwrap()
}

#[tokio::test]
async fn html_search_paging_categories_and_resolve() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search/big%20buck/1"))
        .and(query_param("cat", "10,20"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PAGE))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/t/2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"<a class="dl" href="/files/sintel.torrent">get</a>"#),
        )
        .mount(&server)
        .await;

    let provider = def_for(&server, HTML_DEF);
    let ctx = ProviderCtx::new().unwrap();
    let query = SearchQuery {
        categories: vec![Category::Movies, Category::Tv],
        page: 2,
        ..SearchQuery::text("big buck")
    };
    let results = provider.search(&ctx, &query).await.unwrap();
    assert_eq!(results.len(), 2, "the row without a title is dropped");

    let bbb = &results[0];
    assert_eq!(bbb.seeders, Some(1204));
    assert_eq!(bbb.size_bytes, Some(263 * 1024 * 1024));
    assert_eq!(bbb.category, Some(Category::Movies));
    assert_eq!(
        bbb.info_hash.unwrap().to_string(),
        "dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c"
    );
    assert!(!bbb.needs_resolve);

    let sintel = &results[1];
    assert!(
        sintel.needs_resolve,
        "no magnet in the row, so the details page is needed"
    );
    assert_eq!(sintel.category, Some(Category::Tv));
    match provider.resolve(&ctx, sintel).await.unwrap() {
        DownloadTarget::TorrentFile(url) => {
            assert!(url.as_str().ends_with("/files/sintel.torrent"))
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn site_errors_and_changed_layouts_are_reported() {
    let server = MockServer::start().await;
    Mock::given(path("/search/err/0"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"<p class="error">Search is down</p>"#),
        )
        .mount(&server)
        .await;
    Mock::given(path("/search/changed/0"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"<table><tr class="row"><td><a class="name" href="/t/1">x</a></td><td class="size">1 MB</td></tr></table>"#,
        ))
        .mount(&server)
        .await;
    let provider = def_for(&server, HTML_DEF);
    let ctx = ProviderCtx::new().unwrap();

    let err = provider
        .search(&ctx, &SearchQuery::text("err"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Search is down"), "{err}");

    let err = provider
        .search(&ctx, &SearchQuery::text("changed"))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ProviderError::ParseFailed { field } if field == "seeders"),
        "the missing required field is named: {err:?}"
    );
}

#[tokio::test]
async fn falls_back_to_the_next_mirror_and_remembers_it() {
    #[derive(Debug, Default)]
    struct Memory(std::sync::Mutex<Option<String>>);
    impl MirrorMemory for Memory {
        fn preferred(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        fn remember(&self, url: &str) {
            *self.0.lock().unwrap() = Some(url.to_owned());
        }
    }

    let dead = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&dead)
        .await;
    let alive = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PAGE))
        .mount(&alive)
        .await;

    let yaml = HTML_DEF.replace("[__BASE__]", &format!("[{}, {}]", dead.uri(), alive.uri()));
    let provider = DefinitionProvider::from_yaml(&yaml).unwrap();
    let memory = Arc::new(Memory::default());
    let ctx = ProviderCtx {
        mirrors: Some(memory.clone()),
        ..ProviderCtx::new().unwrap()
    };
    let results = provider
        .search(&ctx, &SearchQuery::text("x"))
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(memory.preferred().unwrap(), format!("{}/", alive.uri()));
}

#[tokio::test]
async fn static_feeds_are_fetched_once() {
    let server = MockServer::start().await;
    Mock::given(path("/feed.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"<rss><channel>
              <item><title>Debian 13 netinst</title><link>/d.torrent</link></item>
              <item><title>Fedora 43</title><link>/f.torrent</link></item>
            </channel></rss>"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let yaml = format!(
        r#"
schema: 1
id: feed
name: Feed
links: [{}]
search:
  path: feed.xml
  response: xml
  static_feed: {{ ttl_hours: 1 }}
  rows: "//item"
  fields:
    title: {{ path: title }}
    download: {{ path: link }}
"#,
        server.uri()
    );
    let provider = DefinitionProvider::from_yaml(&yaml).unwrap();
    let ctx = ProviderCtx::new().unwrap();
    let debian = provider
        .search(&ctx, &SearchQuery::text("debian"))
        .await
        .unwrap();
    assert_eq!(debian.len(), 1);
    assert!(
        debian[0]
            .torrent_url
            .as_ref()
            .unwrap()
            .as_str()
            .ends_with("/d.torrent")
    );
    let fedora = provider
        .search(&ctx, &SearchQuery::text("fedora"))
        .await
        .unwrap();
    assert_eq!(fedora.len(), 1, "served from the cached feed");
}

#[tokio::test]
async fn json_with_settings_and_form_login() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login"))
        .and(body_string_contains("user=ada"))
        .and(body_string_contains("pass=s3cret"))
        .respond_with(ResponseTemplate::new(200).insert_header("set-cookie", "sid=abc; Path=/"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/me"))
        .and(header("cookie", "sid=abc"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"<a class="logout">x</a>"#))
        .mount(&server)
        .await;
    Mock::given(path("/api"))
        .and(query_param("key", "k-123"))
        .and(header("cookie", "sid=abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                { "name": "Ubuntu", "hash": "DD8255ECDC7CA55FB0BBF81323D87062DB1F6D1C", "size": 42, "seeds": 3, "added": 1700000000 },
                { "name": null, "hash": "x" }
            ]
        })))
        .mount(&server)
        .await;

    let yaml = format!(
        r#"
schema: 1
id: private-json
name: Private JSON
type: private
links: [{}]
settings:
  - {{ name: username, required: true }}
  - {{ name: password, type: password, required: true }}
  - {{ name: api_key, required: true }}
login:
  method: form
  path: /login
  inputs: {{ user: "{{{{ cfg.username }}}}", pass: "{{{{ cfg.password }}}}" }}
  test: {{ path: /me, selector: "a.logout" }}
search:
  path: /api
  params: {{ key: "{{{{ cfg.api_key }}}}", q: "{{{{ query.text }}}}" }}
  response: json
  rows: "$.data[*]"
  fields:
    title: {{ path: "$.name" }}
    info_hash: {{ path: "$.hash" }}
    size: {{ path: "$.size" }}
    seeders: {{ path: "$.seeds" }}
    date: {{ path: "$.added", filters: [unix_ts] }}
"#,
        server.uri()
    );
    let provider = DefinitionProvider::from_yaml(&yaml).unwrap();

    let missing = provider
        .search(&ProviderCtx::new().unwrap(), &SearchQuery::text("ubuntu"))
        .await
        .unwrap_err();
    assert!(missing.to_string().contains("required"), "{missing}");

    let jar = Arc::new(reqwest::cookie::Jar::default());
    let http = crate::net::build_client(&crate::net::ClientOptions {
        cookies: Some(jar),
        ..Default::default()
    })
    .unwrap();
    let config: BTreeMap<String, String> = [
        ("username", "ada"),
        ("password", "s3cret"),
        ("api_key", "k-123"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    let ctx = ProviderCtx {
        config: Arc::new(config),
        ..ProviderCtx::with_client(http)
    };
    let results = provider
        .search(&ctx, &SearchQuery::text("ubuntu"))
        .await
        .unwrap();
    assert_eq!(results.len(), 1, "the row without a name is dropped");
    assert_eq!(results[0].size_bytes, Some(42));
    assert_eq!(results[0].published.unwrap().year(), 2023);
    // Logged in once; the second search reuses the session.
    provider
        .search(&ctx, &SearchQuery::text("ubuntu"))
        .await
        .unwrap();
}

// ----- validation --------------------------------------------------------

#[test]
fn validation_reports_every_problem() {
    let yaml = r#"
schema: 2
id: Bad_ID
name: ""
links: ["ftp://x"]
settings:
  - { name: a }
  - { name: a }
search:
  path: "{{ query.text"
  response: html
  rows: "[[["
  fields:
    size: { path: "$.x" }
"#;
    let err = load(yaml).unwrap_err();
    let all = err.errors.join("\n");
    for expected in [
        "schema version",
        "id `Bad_ID`",
        "name must not be empty",
        "not an http(s) URL",
        "listed twice",
        "invalid CSS selector",
        "must include `title`",
        "need a way to download",
        "html responses use `selector`",
        "invalid template",
    ] {
        assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
    }
}

#[test]
fn yaml_errors_have_locations_and_unknown_keys_are_rejected() {
    let err = parse_yaml("schema: 1\nid: x\n  bad: [").unwrap_err();
    assert!(err.to_string().contains("invalid YAML"), "{err}");
    let err = parse_yaml(
        "schema: 1\nid: ab\nname: x\nlinks: [http://a]\ntypo_field: 1\nsearch: {path: /, response: html, rows: tr, fields: {title: {}}}",
    )
    .unwrap_err();
    assert!(err.to_string().contains("typo_field"), "{err}");
}
