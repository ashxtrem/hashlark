// SPDX-License-Identifier: GPL-3.0-or-later

//! The provider abstraction every indexer implements.

pub mod definition;
pub mod internet_archive;
pub mod newznab;
pub mod torznab;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{RequestBuilder, Response, StatusCode, header};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{ProviderError, Result};
use crate::magnet::{DEFAULT_TRACKERS, build_magnet};
use crate::model::{Category, DownloadTarget, ProviderId, SearchQuery, SearchResult};

pub use definition::DefinitionProvider;
pub use internet_archive::InternetArchive;
pub use torznab::TorznabProvider;

/// How a provider is implemented (see ADR 0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Native,
    Definition,
    Torznab,
}

/// What kinds of query a provider supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Capabilities {
    pub text_search: bool,
    pub imdb_search: bool,
    pub paging: bool,
}

/// Static description of a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProviderInfo {
    pub id: ProviderId,
    pub name: String,
    pub kind: ProviderKind,
    pub description: String,
    /// Categories the provider has content for. Empty means "any".
    pub categories: Vec<Category>,
    pub capabilities: Capabilities,
}

impl ProviderInfo {
    /// Whether this provider should take part in `query`.
    pub fn matches(&self, query: &SearchQuery) -> bool {
        if let Some(wanted) = &query.providers
            && !wanted.contains(&self.id)
        {
            return false;
        }
        let has_text = !query.text.trim().is_empty();
        let has_imdb = query.imdb_id.as_deref().is_some_and(|id| !id.is_empty());
        let can_query = (has_text && self.capabilities.text_search)
            || (has_imdb && self.capabilities.imdb_search);
        if !can_query {
            return false;
        }
        query.categories.is_empty()
            || self.categories.is_empty()
            || query.categories.iter().any(|c| self.categories.contains(c))
    }
}

/// Remembers which of a provider's mirrors last worked, so it's tried
/// first next time.
pub trait MirrorMemory: Send + Sync + fmt::Debug {
    fn preferred(&self) -> Option<String>;
    fn remember(&self, url: &str);
}

/// Shared services handed to providers on every call.
#[derive(Clone)]
pub struct ProviderCtx {
    pub http: reqwest::Client,
    /// Same network route as `http`, but doesn't follow redirects (to see
    /// where a download link points, e.g. to a magnet).
    pub http_no_redirect: reqwest::Client,
    /// Trackers appended to magnets built from a bare infohash.
    pub trackers: Arc<[String]>,
    /// The provider's settings (`cfg.*` in definitions), secrets included.
    pub config: Arc<BTreeMap<String, String>>,
    pub mirrors: Option<Arc<dyn MirrorMemory>>,
    /// `.onion` mirrors may be used (traffic goes through Tor).
    pub onion: bool,
}

impl fmt::Debug for ProviderCtx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderCtx")
            .field("trackers", &self.trackers.len())
            .field("config_keys", &self.config.keys().collect::<Vec<_>>())
            .field("onion", &self.onion)
            .finish_non_exhaustive()
    }
}

impl ProviderCtx {
    /// A context with a default HTTP client and tracker list, and no
    /// settings.
    pub fn new() -> Result<Self> {
        let options = crate::net::ClientOptions::default();
        let mut ctx = Self::with_client(crate::net::build_client(&options)?);
        ctx.http_no_redirect = crate::net::build_client(&crate::net::ClientOptions {
            follow_redirects: false,
            ..options
        })?;
        Ok(ctx)
    }

    /// A context around one client (used for both redirect modes).
    pub fn with_client(http: reqwest::Client) -> Self {
        Self {
            http_no_redirect: http.clone(),
            http,
            trackers: DEFAULT_TRACKERS.iter().map(|t| (*t).to_owned()).collect(),
            config: Arc::default(),
            mirrors: None,
            onion: false,
        }
    }
}

/// Providers compiled into the app and enabled by default. Only legal sources
/// belong here (ADR 0007).
pub fn builtin_providers() -> Vec<Arc<dyn SearchProvider>> {
    vec![Arc::new(InternetArchive::new())]
}

/// An indexer that can be searched.
#[async_trait]
pub trait SearchProvider: Send + Sync + fmt::Debug {
    fn info(&self) -> &ProviderInfo;

    /// Runs one search. Implementations should return results as parsed and
    /// leave de-duplication and ranking to the aggregator.
    async fn search(
        &self,
        ctx: &ProviderCtx,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, ProviderError>;

    /// Turns a result into something the OS can open.
    ///
    /// The default uses what the result already carries: its magnet, else a
    /// magnet built from its infohash, else its `.torrent` URL. Providers
    /// whose links live on a details page override this.
    async fn resolve(
        &self,
        ctx: &ProviderCtx,
        result: &SearchResult,
    ) -> Result<DownloadTarget, ProviderError> {
        default_resolve(ctx, result)
    }

    /// Values the user can configure (credentials, API keys). Values reach
    /// the provider through [`ProviderCtx::config`].
    fn settings(&self) -> Vec<crate::definition::spec::SettingDef> {
        Vec::new()
    }

    /// Checks the provider works by running a small search. Returns the
    /// number of results. Providers with a better test query override this.
    async fn test(&self, ctx: &ProviderCtx) -> Result<usize, ProviderError> {
        self.search(ctx, &SearchQuery::text("linux"))
            .await
            .map(|results| results.len())
    }
}

/// The download target a result already carries: its magnet, else a magnet
/// built from its infohash, else its `.torrent` URL.
pub fn default_resolve(
    ctx: &ProviderCtx,
    result: &SearchResult,
) -> Result<DownloadTarget, ProviderError> {
    if let Some(magnet) = &result.magnet {
        return Ok(DownloadTarget::Magnet(magnet.clone()));
    }
    if let Some(hash) = &result.info_hash {
        return Ok(DownloadTarget::Magnet(build_magnet(
            hash,
            &result.title,
            &ctx.trackers,
        )));
    }
    if let Some(url) = &result.torrent_url {
        return Ok(DownloadTarget::TorrentFile(url.clone()));
    }
    Err(ProviderError::ParseFailed {
        field: "magnet".into(),
    })
}

/// Largest response body a provider will read.
pub(crate) const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Reads a response body as text, up to [`MAX_BODY_BYTES`], decoding the
/// charset named in `Content-Type` (UTF-8 otherwise).
pub(crate) async fn read_text(mut response: Response) -> Result<String, ProviderError> {
    let charset = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| {
            ct.split(';')
                .find_map(|p| p.trim().strip_prefix("charset="))
                .map(|c| c.trim_matches('"').to_owned())
        });
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(map_reqwest_error)? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_BODY_BYTES {
            return Err(ProviderError::Network("response too large".into()));
        }
    }
    let encoding = charset
        .and_then(|c| encoding_rs::Encoding::for_label(c.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(&bytes);
    Ok(text.into_owned())
}

/// Sends a request (after the per-host rate limit) and maps transport
/// failures and error statuses to [`ProviderError`].
pub(crate) async fn send(request: RequestBuilder) -> Result<Response, ProviderError> {
    let (client, request) = request.build_split();
    let request = request.map_err(map_reqwest_error)?;
    crate::net::throttle(request.url()).await;
    let response = client.execute(request).await.map_err(map_reqwest_error)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    if matches!(status.as_u16(), 403 | 429 | 503) && is_challenge(response).await {
        return Err(ProviderError::ChallengeRequired);
    }
    Err(match status {
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited { retry_after },
        StatusCode::UNAUTHORIZED => ProviderError::AuthFailed,
        _ => ProviderError::Http {
            status: status.as_u16(),
        },
    })
}

/// Whether an error response is a bot check ("Just a moment…") that a
/// person has to complete in a browser. Consumes the response.
async fn is_challenge(response: Response) -> bool {
    let headers = response.headers();
    if headers
        .get("cf-mitigated")
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"challenge"))
    {
        return true;
    }
    let server = headers
        .get(header::SERVER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !(server.contains("cloudflare") || server.contains("ddos-guard")) {
        return false;
    }
    let Ok(body) = response.text().await else {
        return false;
    };
    const MARKERS: [&str; 6] = [
        "just a moment",
        "cf-chl",
        "challenge-platform",
        "cf_chl_opt",
        "checking your browser",
        "ddos-guard",
    ];
    let head: String = body
        .chars()
        .take(64 * 1024)
        .collect::<String>()
        .to_lowercase();
    MARKERS.iter().any(|m| head.contains(m))
}

/// Sends a request and parses the JSON body.
pub(crate) async fn get_json<T: DeserializeOwned>(
    request: RequestBuilder,
) -> Result<T, ProviderError> {
    let body = send(request)
        .await?
        .bytes()
        .await
        .map_err(map_reqwest_error)?;
    serde_json::from_slice(&body).map_err(|e| {
        tracing::debug!(error = %e, "response body is not the expected JSON");
        ProviderError::ParseFailed {
            field: "body".into(),
        }
    })
}

pub(crate) fn map_reqwest_error(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        return ProviderError::Timeout;
    }
    if let Some(status) = err.status() {
        return ProviderError::Http {
            status: status.as_u16(),
        };
    }
    classify_network_error(error_chain(&err))
}

/// Recognises the usual signs of blocking in a transport error.
fn classify_network_error(message: String) -> ProviderError {
    let lower = message.to_lowercase();
    let dns = [
        "dns error",
        "failed to lookup address",
        "no address found",
        "name or service not known",
        "no such host",
    ];
    let reset = [
        "connection reset",
        "forcibly closed",
        "tls handshake eof",
        "unexpected eof",
    ];
    if dns.iter().any(|m| lower.contains(m)) {
        ProviderError::Blocked {
            reason: "the site's address could not be looked up (it may be down, or blocked by DNS; try encrypted DNS)".into(),
        }
    } else if reset.iter().any(|m| lower.contains(m)) {
        ProviderError::Blocked {
            reason: "the connection was cut off (possibly blocked; try a proxy or Tor)".into(),
        }
    } else if lower.contains("timed out") {
        ProviderError::Timeout
    } else {
        ProviderError::Network(message)
    }
}

/// Joins an error and its sources, since reqwest's top-level message alone
/// ("error sending request") hides the useful part.
fn error_chain(err: &dyn std::error::Error) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(categories: Vec<Category>, imdb: bool) -> ProviderInfo {
        ProviderInfo {
            id: "p".into(),
            name: "P".into(),
            kind: ProviderKind::Native,
            description: String::new(),
            categories,
            capabilities: Capabilities {
                text_search: true,
                imdb_search: imdb,
                paging: false,
            },
        }
    }

    #[test]
    fn matches_filters_by_provider_category_and_capability() {
        let p = info(vec![Category::Movies], false);
        assert!(p.matches(&SearchQuery::text("x")));

        let other_provider = SearchQuery {
            providers: Some(vec!["q".into()]),
            ..SearchQuery::text("x")
        };
        assert!(!p.matches(&other_provider));

        let music = SearchQuery {
            categories: vec![Category::Music],
            ..SearchQuery::text("x")
        };
        assert!(!p.matches(&music));
        assert!(
            info(vec![], false).matches(&music),
            "empty categories means any"
        );

        let imdb_only = SearchQuery {
            imdb_id: Some("tt1".into()),
            ..SearchQuery::text("")
        };
        assert!(!p.matches(&imdb_only));
        assert!(info(vec![], true).matches(&imdb_only));
    }

    #[derive(Debug)]
    struct Bare(ProviderInfo);

    #[async_trait]
    impl SearchProvider for Bare {
        fn info(&self) -> &ProviderInfo {
            &self.0
        }
        async fn search(
            &self,
            _: &ProviderCtx,
            _: &SearchQuery,
        ) -> Result<Vec<SearchResult>, ProviderError> {
            Ok(vec![])
        }
    }

    #[test]
    fn classifies_blocking_errors() {
        assert!(matches!(
            classify_network_error(
                "error sending request: dns error: failed to lookup address".into()
            ),
            ProviderError::Blocked { .. }
        ));
        assert!(matches!(
            classify_network_error(
                "An existing connection was forcibly closed by the remote host".into()
            ),
            ProviderError::Blocked { .. }
        ));
        assert!(matches!(
            classify_network_error("connection refused".into()),
            ProviderError::Network(_)
        ));
    }

    #[tokio::test]
    async fn detects_bot_challenges() {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/cf"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("server", "cloudflare")
                    .set_body_string("<title>Just a moment...</title>"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/mitigated"))
            .respond_with(ResponseTemplate::new(503).insert_header("cf-mitigated", "challenge"))
            .mount(&server)
            .await;
        Mock::given(path("/plain"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;
        let http = ProviderCtx::new().unwrap().http;
        let get = |p: &str| send(http.get(format!("{}{p}", server.uri())));
        assert!(matches!(
            get("/cf").await,
            Err(ProviderError::ChallengeRequired)
        ));
        assert!(matches!(
            get("/mitigated").await,
            Err(ProviderError::ChallengeRequired)
        ));
        assert!(matches!(
            get("/plain").await,
            Err(ProviderError::Http { status: 403 })
        ));
    }

    #[tokio::test]
    async fn default_resolve_prefers_magnet_then_hash_then_torrent() {
        let ctx = ProviderCtx::new().unwrap();
        let p = Bare(info(vec![], false));
        let mut result = SearchResult::new("p".into(), "Title");

        assert!(p.resolve(&ctx, &result).await.is_err());

        let url: url::Url = "https://example.org/x.torrent".parse().unwrap();
        result.torrent_url = Some(url.clone());
        assert_eq!(
            p.resolve(&ctx, &result).await.unwrap(),
            DownloadTarget::TorrentFile(url)
        );

        result.info_hash = Some("1dc689206d6f6ef6021d558d95350ed09004b40c".parse().unwrap());
        let DownloadTarget::Magnet(built) = p.resolve(&ctx, &result).await.unwrap() else {
            panic!("expected magnet");
        };
        assert!(built.starts_with("magnet:?xt=urn:btih:1dc6"));
        assert!(built.contains("&tr="));

        result.magnet = Some("magnet:?xt=urn:btih:given".into());
        assert_eq!(
            p.resolve(&ctx, &result).await.unwrap(),
            DownloadTarget::Magnet("magnet:?xt=urn:btih:given".into())
        );
    }
}
