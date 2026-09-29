// SPDX-License-Identifier: GPL-3.0-or-later

//! Internet Archive (<https://archive.org>): public-domain and openly
//! licensed films, audio, books and software. Every item with the
//! "Archive BitTorrent" format has a `.torrent` with HTTP web seeds, and the
//! search API returns its infohash directly.

use async_trait::async_trait;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

use super::{Capabilities, ProviderCtx, ProviderInfo, ProviderKind, SearchProvider, get_json};
use crate::error::ProviderError;
use crate::model::{Category, DownloadTarget, ProviderId, SearchQuery, SearchResult};

pub const ID: &str = "internet-archive";
const BASE_URL: &str = "https://archive.org/";
const ROWS_PER_PAGE: u32 = 50;
const FIELDS: &[&str] = &[
    "identifier",
    "title",
    "item_size",
    "publicdate",
    "mediatype",
    "btih",
];

/// Internet Archive native provider.
#[derive(Debug)]
pub struct InternetArchive {
    info: ProviderInfo,
    base: Url,
}

impl Default for InternetArchive {
    fn default() -> Self {
        Self::new()
    }
}

impl InternetArchive {
    pub fn new() -> Self {
        Self::with_base_url(Url::parse(BASE_URL).expect("valid base URL"))
    }

    /// Points the provider at another host (used by tests).
    pub fn with_base_url(base: Url) -> Self {
        Self {
            info: ProviderInfo {
                id: ProviderId::new(ID),
                name: "Internet Archive".into(),
                kind: ProviderKind::Native,
                description: "Public-domain and openly licensed films, audio, books and software"
                    .into(),
                categories: vec![
                    Category::Movies,
                    Category::Tv,
                    Category::Music,
                    Category::Books,
                    Category::Software,
                    Category::Games,
                    Category::Other,
                ],
                capabilities: Capabilities {
                    text_search: true,
                    imdb_search: false,
                    paging: true,
                },
            },
            base,
        }
    }

    fn url(&self, path: &str) -> Url {
        self.base.join(path).expect("relative path joins onto base")
    }

    fn to_result(&self, doc: Doc) -> Option<SearchResult> {
        let id = doc.identifier;
        // Identifiers are restricted to [A-Za-z0-9._-]; skip anything else
        // rather than build a broken URL.
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return None;
        }
        Some(SearchResult {
            title: doc
                .title
                .and_then(OneOrMany::first)
                .unwrap_or_else(|| id.clone()),
            size_bytes: doc.item_size.and_then(NumOrString::into_u64),
            info_hash: doc
                .btih
                .and_then(OneOrMany::first)
                .and_then(|h| h.parse().ok()),
            torrent_url: Some(self.url(&format!("download/{id}/{id}_archive.torrent"))),
            details_url: Some(self.url(&format!("details/{id}"))),
            published: doc
                .publicdate
                .and_then(|d| OffsetDateTime::parse(&d, &Rfc3339).ok()),
            category: doc.mediatype.as_deref().map(category_of_mediatype),
            ..SearchResult::new(self.info.id.clone(), String::new())
        })
    }
}

#[async_trait]
impl SearchProvider for InternetArchive {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn search(
        &self,
        ctx: &ProviderCtx,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        let mut params: Vec<(&str, String)> = vec![("q", build_query(query))];
        params.extend(FIELDS.iter().map(|f| ("fl[]", (*f).to_owned())));
        params.push(("rows", ROWS_PER_PAGE.to_string()));
        params.push(("page", query.page.max(1).to_string()));
        params.push(("output", "json".into()));

        let response: Response =
            get_json(ctx.http.get(self.url("advancedsearch.php")).query(&params)).await?;
        Ok(response
            .response
            .docs
            .into_iter()
            .filter_map(|doc| self.to_result(doc))
            .collect())
    }

    /// Prefers the `.torrent`: unlike a bare magnet it carries the Archive's
    /// HTTP web seeds, so downloads start even with no peers.
    async fn resolve(
        &self,
        _ctx: &ProviderCtx,
        result: &SearchResult,
    ) -> Result<DownloadTarget, ProviderError> {
        result
            .torrent_url
            .clone()
            .map(DownloadTarget::TorrentFile)
            .ok_or(ProviderError::ParseFailed {
                field: "torrent_url".into(),
            })
    }
}

/// Builds the Lucene query: the text against title and subject, restricted to
/// items that have a torrent, and to the requested categories.
fn build_query(query: &SearchQuery) -> String {
    let text = escape_lucene(query.text.trim());
    let mut q = format!(r#"(title:({text}) OR subject:({text})) AND format:"Archive BitTorrent""#);

    let mut mediatypes: Vec<&str> = query
        .categories
        .iter()
        .flat_map(|c| mediatypes_of_category(*c).iter().copied())
        .collect();
    mediatypes.sort_unstable();
    mediatypes.dedup();
    if !mediatypes.is_empty() {
        q.push_str(&format!(" AND mediatype:({})", mediatypes.join(" OR ")));
    }
    q
}

fn escape_lucene(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '+' | '-'
                | '&'
                | '|'
                | '!'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
                | '^'
                | '"'
                | '~'
                | '*'
                | '?'
                | ':'
                | '\\'
                | '/'
        ) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

fn mediatypes_of_category(category: Category) -> &'static [&'static str] {
    match category {
        // The Archive files TV under "movies".
        Category::Movies | Category::Tv => &["movies"],
        Category::Music => &["audio", "etree"],
        Category::Books => &["texts"],
        Category::Software | Category::Games => &["software"],
        Category::Other => &["image", "data"],
        Category::Anime => &[],
    }
}

fn category_of_mediatype(mediatype: &str) -> Category {
    match mediatype {
        "movies" => Category::Movies,
        "audio" | "etree" => Category::Music,
        "texts" => Category::Books,
        "software" => Category::Software,
        _ => Category::Other,
    }
}

#[derive(Debug, Deserialize)]
struct Response {
    response: Docs,
}

#[derive(Debug, Deserialize)]
struct Docs {
    docs: Vec<Doc>,
}

#[derive(Debug, Deserialize)]
struct Doc {
    identifier: String,
    title: Option<OneOrMany>,
    item_size: Option<NumOrString>,
    publicdate: Option<String>,
    mediatype: Option<String>,
    btih: Option<OneOrMany>,
}

/// The Archive returns some fields as either a string or a list of strings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn first(self) -> Option<String> {
        match self {
            Self::One(s) => Some(s),
            Self::Many(v) => v.into_iter().next(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NumOrString {
    Num(u64),
    Str(String),
}

impl NumOrString {
    fn into_u64(self) -> Option<u64> {
        match self {
            Self::Num(n) => Some(n),
            Self::Str(s) => s.trim().parse().ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/internet-archive/search.json"
    ));

    #[test]
    fn query_escapes_text_and_filters_categories() {
        let query = SearchQuery {
            categories: vec![Category::Music, Category::Movies, Category::Tv],
            ..SearchQuery::text(" AC/DC: live ")
        };
        assert_eq!(
            build_query(&query),
            r#"(title:(AC\/DC\: live) OR subject:(AC\/DC\: live)) AND format:"Archive BitTorrent" AND mediatype:(audio OR etree OR movies)"#
        );
    }

    #[tokio::test]
    async fn parses_search_fixture() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/advancedsearch.php"))
            .and(query_param("output", "json"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE))
            .expect(1)
            .mount(&server)
            .await;

        let provider =
            InternetArchive::with_base_url(format!("{}/", server.uri()).parse().unwrap());
        let ctx = ProviderCtx::new().unwrap();
        let results = provider
            .search(&ctx, &SearchQuery::text("night of the living dead"))
            .await
            .unwrap();

        assert_eq!(results.len(), 5);
        let first = &results[0];
        assert_eq!(
            first.title,
            "Civic Theatre \"Night of the Living Dead\" Newscast"
        );
        assert_eq!(first.size_bytes, Some(12_955_769));
        assert_eq!(
            first.info_hash.unwrap().to_string(),
            "488a7bc4f9b8c3eeef3bb33329376661542e4500"
        );
        assert_eq!(first.category, Some(Category::Movies));
        assert_eq!(first.published.unwrap().year(), 2016);
        assert!(
            first
                .torrent_url
                .as_ref()
                .unwrap()
                .path()
                .ends_with("/download/Civic_Theatre_Night_of_the_Living_Dead_Newscast/Civic_Theatre_Night_of_the_Living_Dead_Newscast_archive.torrent")
        );
        assert_eq!(results[2].category, Some(Category::Music));
        assert!(results.iter().all(|r| r.provider_id.as_str() == ID));
    }

    #[tokio::test]
    async fn maps_server_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let provider =
            InternetArchive::with_base_url(format!("{}/", server.uri()).parse().unwrap());
        let err = provider
            .search(&ProviderCtx::new().unwrap(), &SearchQuery::text("x"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, ProviderError::Http { status: 503 }),
            "{err:?}"
        );
    }

    #[test]
    fn tolerates_list_fields_and_string_sizes() {
        let doc: Doc = serde_json::from_value(serde_json::json!({
            "identifier": "item",
            "title": ["First title", "Second"],
            "item_size": "42",
            "btih": ["488a7bc4f9b8c3eeef3bb33329376661542e4500"]
        }))
        .unwrap();
        let result = InternetArchive::new().to_result(doc).unwrap();
        assert_eq!(result.title, "First title");
        assert_eq!(result.size_bytes, Some(42));
        assert!(result.info_hash.is_some());
    }

    #[test]
    fn skips_items_with_unsafe_identifiers() {
        let doc: Doc =
            serde_json::from_value(serde_json::json!({ "identifier": "../evil" })).unwrap();
        assert!(InternetArchive::new().to_result(doc).is_none());
    }
}
