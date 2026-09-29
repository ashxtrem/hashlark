// SPDX-License-Identifier: GPL-3.0-or-later

//! Torznab: the API of Jackett, Prowlarr and Bitmagnet. One Torznab provider
//! can stand for hundreds of indexers behind a self-hosted aggregator.

use async_trait::async_trait;
use reqwest::header::LOCATION;
use roxmltree::{Document, Node};
use url::Url;

use super::{
    Capabilities, ProviderCtx, ProviderInfo, ProviderKind, SearchProvider, default_resolve,
    newznab, read_text, send,
};
use crate::definition::filters::parse_date_any;
use crate::definition::spec::{SettingDef, SettingKind};
use crate::definition::xmlpath::{XmlPath, text_of};
use crate::error::ProviderError;
use crate::magnet::infohash_from_magnet;
use crate::model::{DownloadTarget, InfoHash, ProviderId, SearchQuery, SearchResult};

/// Name of the setting holding the API key.
pub const API_KEY: &str = "api_key";
const PAGE_SIZE: u32 = 100;

#[derive(Debug)]
pub struct TorznabProvider {
    info: ProviderInfo,
    endpoint: Url,
}

impl TorznabProvider {
    /// `endpoint` is the Torznab API URL, e.g.
    /// `http://127.0.0.1:9117/api/v2.0/indexers/all/results/torznab/api`
    /// (Jackett) or `http://127.0.0.1:9696/1/api` (Prowlarr).
    pub fn new(id: &str, name: &str, endpoint: Url) -> Self {
        Self {
            info: ProviderInfo {
                id: ProviderId::new(id),
                name: name.to_owned(),
                kind: ProviderKind::Torznab,
                description: format!("Torznab endpoint at {}", endpoint.host_str().unwrap_or("?")),
                categories: Vec::new(),
                capabilities: Capabilities {
                    text_search: true,
                    imdb_search: true,
                    paging: true,
                },
            },
            endpoint,
        }
    }

    fn request_url(&self, ctx: &ProviderCtx, query: &SearchQuery) -> Url {
        let mut url = self.endpoint.clone();
        {
            let mut q = url.query_pairs_mut();
            match query.imdb_id.as_deref().filter(|i| !i.is_empty()) {
                Some(imdb) => {
                    q.append_pair("t", "movie");
                    q.append_pair("imdbid", imdb.trim_start_matches("tt"));
                }
                None => {
                    q.append_pair("t", "search");
                }
            }
            if !query.text.trim().is_empty() {
                q.append_pair("q", query.text.trim());
            }
            if !query.categories.is_empty() {
                let cats: Vec<String> = query
                    .categories
                    .iter()
                    .map(|c| newznab::id_of(*c).to_string())
                    .collect();
                q.append_pair("cat", &cats.join(","));
            }
            q.append_pair("limit", &PAGE_SIZE.to_string());
            q.append_pair("offset", &((query.page.max(1) - 1) * PAGE_SIZE).to_string());
            if let Some(key) = ctx.config.get(API_KEY).filter(|k| !k.is_empty()) {
                q.append_pair("apikey", key);
            }
        }
        url
    }

    /// Parses a Torznab RSS response.
    pub fn parse(&self, body: &str) -> Result<Vec<SearchResult>, ProviderError> {
        let doc = Document::parse_with_options(
            body,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .map_err(|_| ProviderError::ParseFailed {
            field: "body (not XML)".into(),
        })?;
        let root = doc.root_element();
        if root.tag_name().name() == "error" {
            let code: u32 = root
                .attribute("code")
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            let description = root.attribute("description").unwrap_or("unknown error");
            return Err(match code {
                100..=102 => ProviderError::AuthFailed,
                _ => ProviderError::Definition(format!("Torznab error {code}: {description}")),
            });
        }
        let items = XmlPath::parse("//item").expect("valid path");
        Ok(items
            .select_from_document(&doc)
            .into_iter()
            .filter_map(|item| self.item(item))
            .collect())
    }

    fn item(&self, item: Node<'_, '_>) -> Option<SearchResult> {
        let child = |name: &str| {
            item.children()
                .find(|c| c.is_element() && c.tag_name().name() == name)
                .map(text_of)
                .filter(|t| !t.is_empty())
        };
        let attr = |name: &str| {
            item.children()
                .filter(|c| c.is_element() && c.tag_name().name() == "attr")
                .find(|c| c.attribute("name") == Some(name))
                .and_then(|c| c.attribute("value"))
                .map(str::to_owned)
        };
        let enclosure = item
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "enclosure");

        let title = child("title")?;
        let link = child("link").or_else(|| {
            enclosure
                .and_then(|e| e.attribute("url"))
                .map(str::to_owned)
        });
        let magnet =
            attr("magneturl").or_else(|| link.clone().filter(|l| l.starts_with("magnet:")));
        let torrent_url = link
            .filter(|l| !l.starts_with("magnet:"))
            .and_then(|l| Url::parse(&l).ok());
        let info_hash: Option<InfoHash> = attr("infohash")
            .and_then(|h| h.parse().ok())
            .or_else(|| magnet.as_deref().and_then(infohash_from_magnet));
        let seeders: Option<u32> = attr("seeders").and_then(|s| s.parse().ok());
        let peers: Option<u32> = attr("peers").and_then(|s| s.parse().ok());
        let size = child("size")
            .or_else(|| attr("size"))
            .or_else(|| {
                enclosure
                    .and_then(|e| e.attribute("length"))
                    .map(str::to_owned)
            })
            .and_then(|s| s.parse().ok())
            .filter(|s| *s > 0);
        let category = item
            .children()
            .filter(|c| c.is_element())
            .filter_map(|c| match c.tag_name().name() {
                "attr" if c.attribute("name") == Some("category") => {
                    c.attribute("value").map(str::to_owned)
                }
                "category" => Some(text_of(c)),
                _ => None,
            })
            .filter_map(|v| v.parse::<u32>().ok())
            .find_map(newznab::category_of);

        if magnet.is_none() && torrent_url.is_none() && info_hash.is_none() {
            return None;
        }
        Some(SearchResult {
            title,
            size_bytes: size,
            seeders,
            leechers: match (peers, seeders) {
                (Some(p), Some(s)) => Some(p.saturating_sub(s)),
                _ => attr("leechers").and_then(|s| s.parse().ok()),
            },
            info_hash,
            magnet,
            torrent_url,
            details_url: child("comments")
                .or_else(|| child("guid").filter(|g| g.starts_with("http")))
                .and_then(|u| Url::parse(&u).ok()),
            published: child("pubDate").and_then(|d| parse_date_any(&d)),
            category,
            provider_id: self.info.id.clone(),
            needs_resolve: false,
        })
    }
}

#[async_trait]
impl SearchProvider for TorznabProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn search(
        &self,
        ctx: &ProviderCtx,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        let response = send(ctx.http.get(self.request_url(ctx, query))).await?;
        self.parse(&read_text(response).await?)
    }

    /// Aggregators often hand out a download URL that redirects to a
    /// magnet; follow that one hop by hand.
    async fn resolve(
        &self,
        ctx: &ProviderCtx,
        result: &SearchResult,
    ) -> Result<DownloadTarget, ProviderError> {
        if result.magnet.is_some() || result.info_hash.is_some() {
            return default_resolve(ctx, result);
        }
        let Some(link) = &result.torrent_url else {
            return default_resolve(ctx, result);
        };
        crate::net::throttle(link).await;
        // Redirects are the point here, so don't treat 3xx as an error.
        let response = ctx.http_no_redirect.get(link.clone()).send().await;
        let location = match &response {
            Ok(r) if r.status().is_redirection() => r
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            _ => None,
        };
        match location {
            Some(to) if to.starts_with("magnet:") => Ok(DownloadTarget::Magnet(to)),
            Some(to) => Ok(DownloadTarget::TorrentFile(
                link.join(&to).unwrap_or_else(|_| link.clone()),
            )),
            None => Ok(DownloadTarget::TorrentFile(link.clone())),
        }
    }

    fn settings(&self) -> Vec<SettingDef> {
        vec![SettingDef {
            name: API_KEY.into(),
            kind: SettingKind::Password,
            label: Some("API key".into()),
            default: None,
            options: Default::default(),
            required: false,
        }]
    }

    async fn test(&self, ctx: &ProviderCtx) -> Result<usize, ProviderError> {
        self.search(ctx, &SearchQuery::text("ubuntu"))
            .await
            .map(|r| r.len())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::model::Category;

    const FEED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/torznab/search.xml"
    ));

    fn ctx_with_key(key: &str) -> ProviderCtx {
        let config: BTreeMap<String, String> = [(API_KEY.to_owned(), key.to_owned())].into();
        ProviderCtx {
            config: Arc::new(config),
            ..ProviderCtx::new().unwrap()
        }
    }

    #[test]
    fn parses_jackett_style_feed() {
        let p = TorznabProvider::new("j", "Jackett", "http://x/api".parse().unwrap());
        let results = p.parse(FEED).unwrap();
        assert_eq!(results.len(), 2);

        let a = &results[0];
        assert_eq!(a.title, "Big Buck Bunny 1080p");
        assert_eq!(a.seeders, Some(120));
        assert_eq!(a.leechers, Some(30), "peers minus seeders");
        assert_eq!(a.size_bytes, Some(725_106_140));
        assert_eq!(a.category, Some(Category::Movies));
        assert!(a.magnet.is_some() && a.info_hash.is_some());
        assert_eq!(a.published.unwrap().year(), 2026);
        assert!(a.details_url.as_ref().unwrap().as_str().contains("details"));

        let b = &results[1];
        assert!(b.magnet.is_none());
        assert!(b.torrent_url.as_ref().unwrap().as_str().contains("/dl/"));
        assert_eq!(b.category, Some(Category::Tv));
    }

    #[test]
    fn maps_torznab_errors() {
        let p = TorznabProvider::new("j", "Jackett", "http://x/api".parse().unwrap());
        assert!(matches!(
            p.parse(r#"<error code="100" description="Incorrect user credentials"/>"#),
            Err(ProviderError::AuthFailed)
        ));
        assert!(
            p.parse(r#"<error code="900" description="Indexer down"/>"#)
                .unwrap_err()
                .to_string()
                .contains("Indexer down")
        );
    }

    #[tokio::test]
    async fn searches_with_key_categories_and_paging_then_resolves_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api"))
            .and(query_param("t", "search"))
            .and(query_param("q", "bunny"))
            .and(query_param("cat", "2000,5000"))
            .and(query_param("offset", "100"))
            .and(query_param("apikey", "secret"))
            .respond_with(ResponseTemplate::new(200).set_body_string(FEED))
            .mount(&server)
            .await;
        Mock::given(path("/dl/2"))
            .respond_with(ResponseTemplate::new(302).insert_header(
                "location",
                "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c",
            ))
            .mount(&server)
            .await;

        let p = TorznabProvider::new(
            "j",
            "Jackett",
            format!("{}/api", server.uri()).parse().unwrap(),
        );
        let query = SearchQuery {
            categories: vec![Category::Movies, Category::Tv],
            page: 2,
            ..SearchQuery::text("bunny")
        };
        // The fixture's second item links to the mock server's /dl/2.
        let feed = FEED.replace("http://127.0.0.1:9117", &server.uri());
        Mock::given(path("/api2"))
            .respond_with(ResponseTemplate::new(200).set_body_string(feed))
            .mount(&server)
            .await;
        let ctx = ctx_with_key("secret");
        assert_eq!(p.search(&ctx, &query).await.unwrap().len(), 2);

        let p2 = TorznabProvider::new(
            "j",
            "Jackett",
            format!("{}/api2", server.uri()).parse().unwrap(),
        );
        let results = p2.search(&ctx, &SearchQuery::text("x")).await.unwrap();
        let target = p2.resolve(&ctx, &results[1]).await.unwrap();
        assert!(matches!(target, DownloadTarget::Magnet(m) if m.contains("dd8255ec")));
    }
}
