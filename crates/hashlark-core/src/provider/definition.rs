// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs a YAML provider definition (ADR 0005).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::header::{COOKIE, HeaderName, HeaderValue};
use reqwest::{Method, RequestBuilder};
use scraper::Html;
use url::Url;

use super::{
    Capabilities, ProviderCtx, ProviderInfo, ProviderKind, SearchProvider, default_resolve,
    read_text, send,
};
use crate::definition::compile::{CompiledDefinition, DefinitionError, load};
use crate::definition::extract::{self, QueryContext, TemplateContext};
use crate::definition::filters;
use crate::definition::spec::{HttpMethod, LoginMethod, Mode};
use crate::error::ProviderError;
use crate::model::{DownloadTarget, ProviderId, SearchQuery, SearchResult};

/// A cached static feed.
#[derive(Debug)]
struct Feed {
    fetched: Instant,
    body: Arc<String>,
    url: Url,
}

#[derive(Debug)]
pub struct DefinitionProvider {
    def: Arc<CompiledDefinition>,
    info: ProviderInfo,
    logged_in: tokio::sync::Mutex<bool>,
    feed: Mutex<Option<Feed>>,
}

impl DefinitionProvider {
    pub fn new(def: CompiledDefinition) -> Self {
        let spec = &def.spec;
        let info = ProviderInfo {
            id: ProviderId::new(spec.id.clone()),
            name: spec.name.clone(),
            kind: ProviderKind::Definition,
            description: spec.description.clone(),
            categories: spec.caps.categories.keys().copied().collect(),
            capabilities: Capabilities {
                text_search: spec.caps.modes.contains(&Mode::Search),
                imdb_search: spec.caps.modes.contains(&Mode::Imdb),
                paging: spec.caps.paging.is_some() && spec.search.static_feed.is_none(),
            },
        };
        Self {
            def: Arc::new(def),
            info,
            logged_in: tokio::sync::Mutex::new(false),
            feed: Mutex::new(None),
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, DefinitionError> {
        Ok(Self::new(load(yaml)?))
    }

    pub fn definition(&self) -> &CompiledDefinition {
        &self.def
    }

    /// Parses a search page without any network access (fixture tests,
    /// `defs test`).
    pub fn parse_page(
        &self,
        body: &str,
        page_url: &Url,
        query: &SearchQuery,
        cfg: &BTreeMap<String, String>,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        let qctx = QueryContext::new(&self.def, query);
        extract::parse_page(&self.def, &self.info.id, body, page_url, &qctx, cfg)
    }

    /// Links to try, the one that last worked first. `.onion` links only
    /// when Tor is in use.
    fn links(&self, ctx: &ProviderCtx) -> Vec<Url> {
        let mut links: Vec<Url> = self
            .def
            .links
            .iter()
            .filter(|u| ctx.onion || !u.host_str().is_some_and(|h| h.ends_with(".onion")))
            .cloned()
            .collect();
        if let Some(preferred) = ctx.mirrors.as_ref().and_then(|m| m.preferred())
            && let Some(i) = links.iter().position(|u| u.as_str() == preferred)
        {
            let url = links.remove(i);
            links.insert(0, url);
        }
        links
    }

    fn template_ctx<'a>(
        &self,
        ctx: &'a ProviderCtx,
        qctx: &QueryContext,
        base: &Url,
    ) -> TemplateContext<'a> {
        TemplateContext {
            query: qctx.clone(),
            cfg: &ctx.config,
            row: BTreeMap::new(),
            base: base.to_string(),
        }
    }

    fn check_settings(&self, ctx: &ProviderCtx) -> Result<(), ProviderError> {
        for setting in &self.def.spec.settings {
            if setting.required && ctx.config.get(&setting.name).is_none_or(|v| v.is_empty()) {
                return Err(ProviderError::Definition(format!(
                    "the setting `{}` is required; set it on the Providers page",
                    setting.label.as_deref().unwrap_or(&setting.name)
                )));
            }
        }
        Ok(())
    }

    /// Adds definition headers and the login cookie to a request.
    fn decorate(
        &self,
        ctx: &ProviderCtx,
        mut request: RequestBuilder,
        tctx: &TemplateContext<'_>,
        extra: &BTreeMap<String, String>,
    ) -> Result<RequestBuilder, ProviderError> {
        for name in self.def.spec.headers.keys().chain(extra.keys()) {
            let value = extract::render(&self.def, &format!("headers.{name}"), tctx)?;
            let name = HeaderName::try_from(name.as_str())
                .map_err(|_| ProviderError::Definition(format!("invalid header name `{name}`")))?;
            let value = HeaderValue::try_from(value).map_err(|_| {
                ProviderError::Definition(format!("invalid value for header `{name}`"))
            })?;
            request = request.header(name, value);
        }
        if matches!(
            self.def.spec.login.as_ref().map(|l| l.method),
            Some(LoginMethod::Cookie)
        ) && let Some(cookie) = ctx.config.get("cookie")
        {
            request = request.header(COOKIE, cookie.as_str());
        }
        Ok(request)
    }

    async fn ensure_login(&self, ctx: &ProviderCtx, base: &Url) -> Result<(), ProviderError> {
        let Some(login) = &self.def.spec.login else {
            return Ok(());
        };
        if login.method != LoginMethod::Form {
            return Ok(());
        }
        let mut logged_in = self.logged_in.lock().await;
        if *logged_in {
            return Ok(());
        }
        let qctx = QueryContext::new(&self.def, &SearchQuery::text(""));
        let tctx = self.template_ctx(ctx, &qctx, base);
        let path = extract::render(&self.def, "login.path", &tctx)?;
        let url = join(base, &path)?;
        let mut form = Vec::new();
        for name in login.inputs.keys() {
            form.push((
                name.clone(),
                extract::render(&self.def, &format!("login.inputs.{name}"), &tctx)?,
            ));
        }
        let request =
            self.decorate(ctx, ctx.http.post(url).form(&form), &tctx, &BTreeMap::new())?;
        send(request).await.map_err(|e| match e {
            ProviderError::Http { status: 401 | 403 } => ProviderError::AuthFailed,
            other => other,
        })?;

        if let (Some(test), Some(selector)) = (&login.test, &self.def.login_test_selector) {
            let path = extract::render(&self.def, "login.test.path", &tctx)?;
            let request = self.decorate(
                ctx,
                ctx.http.get(join(base, &path)?),
                &tctx,
                &BTreeMap::new(),
            )?;
            let body = read_text(send(request).await?).await?;
            if Html::parse_document(&body)
                .select(selector)
                .next()
                .is_none()
            {
                tracing::debug!(provider = %self.info.id, check = %test.selector, "login check failed");
                return Err(ProviderError::AuthFailed);
            }
        }
        *logged_in = true;
        Ok(())
    }

    /// Fetches the raw search page for `query` (used by `defs test --record`).
    pub async fn fetch_page(
        &self,
        ctx: &ProviderCtx,
        query: &SearchQuery,
    ) -> Result<(Arc<String>, Url), ProviderError> {
        self.check_settings(ctx)?;
        self.fetch(ctx, &QueryContext::new(&self.def, query)).await
    }

    /// Fetches the search page from the first link that answers.
    async fn fetch(
        &self,
        ctx: &ProviderCtx,
        qctx: &QueryContext,
    ) -> Result<(Arc<String>, Url), ProviderError> {
        if let Some(feed) = &self.def.spec.search.static_feed {
            let ttl = Duration::from_secs(u64::from(feed.ttl_hours) * 3600);
            let cached = self.feed.lock().expect("lock");
            if let Some(f) = cached.as_ref().filter(|f| f.fetched.elapsed() < ttl) {
                return Ok((Arc::clone(&f.body), f.url.clone()));
            }
        }

        let links = self.links(ctx);
        let mut last_error = ProviderError::Network("no usable links".into());
        for base in &links {
            match self.fetch_from(ctx, qctx, base).await {
                Ok((body, url)) => {
                    if let Some(memory) = &ctx.mirrors {
                        memory.remember(base.as_str());
                    }
                    let body = Arc::new(body);
                    if self.def.spec.search.static_feed.is_some() {
                        *self.feed.lock().expect("lock") = Some(Feed {
                            fetched: Instant::now(),
                            body: Arc::clone(&body),
                            url: url.clone(),
                        });
                    }
                    return Ok((body, url));
                }
                Err(e) if is_mirror_problem(&e) && links.len() > 1 => {
                    tracing::debug!(provider = %self.info.id, mirror = %base, error = %e, "mirror failed, trying the next");
                    last_error = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_error)
    }

    async fn fetch_from(
        &self,
        ctx: &ProviderCtx,
        qctx: &QueryContext,
        base: &Url,
    ) -> Result<(String, Url), ProviderError> {
        self.ensure_login(ctx, base).await?;
        let search = &self.def.spec.search;
        let tctx = self.template_ctx(ctx, qctx, base);
        let path = extract::render(&self.def, "search.path", &tctx)?;
        let url = join(base, &path)?;
        let mut params = Vec::new();
        for name in search.params.keys() {
            params.push((
                name.clone(),
                extract::render(&self.def, &format!("search.params.{name}"), &tctx)?,
            ));
        }
        let request = match search.method {
            HttpMethod::Get => ctx.http.request(Method::GET, url.clone()).query(&params),
            HttpMethod::Post => ctx.http.request(Method::POST, url.clone()).form(&params),
        };
        let request = self.decorate(ctx, request, &tctx, &search.headers)?;
        let response = send(request).await?;
        let final_url = response.url().clone();
        let body = read_text(response).await?;
        Ok((body, final_url))
    }
}

/// Whether trying another mirror might help.
fn is_mirror_problem(e: &ProviderError) -> bool {
    match e {
        ProviderError::Network(_)
        | ProviderError::Timeout
        | ProviderError::Blocked { .. }
        | ProviderError::ChallengeRequired => true,
        ProviderError::Http { status } => *status >= 500 || *status == 404 || *status == 403,
        _ => false,
    }
}

fn join(base: &Url, path: &str) -> Result<Url, ProviderError> {
    let url = base
        .join(path.trim())
        .map_err(|e| ProviderError::Definition(format!("bad URL `{path}`: {e}")))?;
    if matches!(url.scheme(), "http" | "https") {
        Ok(url)
    } else {
        Err(ProviderError::Definition(format!(
            "URL `{url}` is not http(s)"
        )))
    }
}

#[async_trait]
impl SearchProvider for DefinitionProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn search(
        &self,
        ctx: &ProviderCtx,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        self.check_settings(ctx)?;
        let qctx = QueryContext::new(&self.def, query);
        let (body, url) = self.fetch(ctx, &qctx).await?;
        let first = extract::parse_page(&self.def, &self.info.id, &body, &url, &qctx, &ctx.config);
        let session_expired = matches!(
            first,
            Err(ProviderError::ParseFailed { .. } | ProviderError::AuthFailed)
        ) && self.def.spec.login.is_some()
            && std::mem::replace(&mut *self.logged_in.lock().await, false);
        if !session_expired {
            return first;
        }
        // A logged-in session may have expired: log in again once.
        let (body, url) = self.fetch(ctx, &qctx).await?;
        extract::parse_page(&self.def, &self.info.id, &body, &url, &qctx, &ctx.config)
    }

    /// For results found without a download link, reads it from the
    /// details page.
    async fn resolve(
        &self,
        ctx: &ProviderCtx,
        result: &SearchResult,
    ) -> Result<DownloadTarget, ProviderError> {
        if !result.needs_resolve {
            return default_resolve(ctx, result);
        }
        let (Some(download), Some(details)) = (&self.def.download, &result.details_url) else {
            return default_resolve(ctx, result);
        };
        let qctx = QueryContext::new(&self.def, &SearchQuery::text(""));
        let tctx = self.template_ctx(ctx, &qctx, details);
        let request = self.decorate(ctx, ctx.http.get(details.clone()), &tctx, &BTreeMap::new())?;
        let response = send(request).await?;
        let page_url = response.url().clone();
        let body = read_text(response).await?;
        let link = {
            let doc = Html::parse_document(&body);
            doc.select(&download.selector)
                .next()
                .and_then(|el| el.value().attr(&download.attr).map(str::to_owned))
        };
        let link = filters::apply(&download.filters, link, &page_url).ok_or_else(|| {
            ProviderError::ParseFailed {
                field: "download".into(),
            }
        })?;
        if link.trim_start().starts_with("magnet:") {
            Ok(DownloadTarget::Magnet(link.trim().to_owned()))
        } else {
            Ok(DownloadTarget::TorrentFile(join(&page_url, &link)?))
        }
    }

    fn settings(&self) -> Vec<crate::definition::spec::SettingDef> {
        self.def.spec.settings.clone()
    }

    async fn test(&self, ctx: &ProviderCtx) -> Result<usize, ProviderError> {
        let text = self
            .def
            .spec
            .test
            .as_ref()
            .map_or("linux", |t| t.query.as_str());
        self.search(ctx, &SearchQuery::text(text))
            .await
            .map(|r| r.len())
    }
}
