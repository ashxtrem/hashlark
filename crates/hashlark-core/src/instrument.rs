// SPDX-License-Identifier: GPL-3.0-or-later

//! Wraps a provider with result caching, a timeout, and health recording.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;
use sqlx::Row;

use crate::error::{ErrorKind, ProviderError};
use crate::health;
use crate::model::{Category, DownloadTarget, SearchQuery, SearchResult};
use crate::provider::{ProviderCtx, ProviderInfo, SearchProvider};
use crate::store::{Store, now_ms};
use crate::util::fnv1a64;

/// A provider as run by the engine.
///
/// The timeout lives here (rather than only in the aggregator) so that
/// timeouts are recorded in the provider's health like any other failure.
#[derive(Debug)]
pub struct Instrumented {
    inner: Arc<dyn SearchProvider>,
    store: Store,
    cache_ttl: Duration,
    timeout: Duration,
}

impl Instrumented {
    pub fn new(
        inner: Arc<dyn SearchProvider>,
        store: Store,
        cache_ttl: Duration,
        timeout: Duration,
    ) -> Self {
        Self {
            inner,
            store,
            cache_ttl,
            timeout,
        }
    }

    async fn record(&self, started: Instant, error: Option<ErrorKind>) {
        let latency = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if let Err(e) =
            health::record(&self.store, self.inner.info().id.as_str(), latency, error).await
        {
            tracing::warn!(provider = %self.inner.info().id, error = %e, "could not record health");
        }
    }

    async fn cache_get(&self, key: &str) -> Option<Vec<SearchResult>> {
        let row =
            sqlx::query("SELECT payload FROM result_cache WHERE cache_key = ? AND expires_at > ?")
                .bind(key)
                .bind(now_ms())
                .fetch_optional(self.store.pool())
                .await
                .ok()??;
        let payload: String = row.try_get("payload").ok()?;
        serde_json::from_str(&payload).ok()
    }

    async fn cache_put(&self, key: &str, results: &[SearchResult]) {
        let Ok(payload) = serde_json::to_string(results) else {
            return;
        };
        let now = now_ms();
        let ttl = i64::try_from(self.cache_ttl.as_millis()).unwrap_or(i64::MAX);
        let outcome = async {
            sqlx::query("DELETE FROM result_cache WHERE expires_at <= ?")
                .bind(now)
                .execute(self.store.pool())
                .await?;
            sqlx::query(
                "INSERT INTO result_cache (cache_key, payload, expires_at) VALUES (?, ?, ?)
                 ON CONFLICT (cache_key) DO UPDATE SET payload = excluded.payload,
                                                       expires_at = excluded.expires_at",
            )
            .bind(key)
            .bind(payload)
            .bind(now.saturating_add(ttl))
            .execute(self.store.pool())
            .await
        }
        .await;
        if let Err(e) = outcome {
            tracing::debug!(error = %e, "could not cache results");
        }
    }
}

/// Parts of a query that change a provider's answer. Sort order and the
/// provider filter are applied after merging, so they're left out.
#[derive(Serialize)]
struct CacheKey<'a> {
    text: String,
    categories: Vec<Category>,
    imdb_id: Option<&'a str>,
    page: u32,
}

fn cache_key(provider_id: &str, query: &SearchQuery) -> String {
    let mut categories = query.categories.clone();
    categories.sort_by_key(|c| c.as_str());
    categories.dedup();
    let key = CacheKey {
        text: query.text.trim().to_lowercase(),
        categories,
        imdb_id: query.imdb_id.as_deref(),
        page: query.page,
    };
    let json = serde_json::to_string(&key).unwrap_or_default();
    format!("search:{provider_id}:{:016x}", fnv1a64(&json))
}

#[async_trait]
impl SearchProvider for Instrumented {
    fn info(&self) -> &ProviderInfo {
        self.inner.info()
    }

    async fn search(
        &self,
        ctx: &ProviderCtx,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        let key = cache_key(self.inner.info().id.as_str(), query);
        if !self.cache_ttl.is_zero()
            && let Some(cached) = self.cache_get(&key).await
        {
            return Ok(cached);
        }

        let started = Instant::now();
        let result = tokio::time::timeout(self.timeout, self.inner.search(ctx, query))
            .await
            .unwrap_or(Err(ProviderError::Timeout));
        self.record(started, result.as_ref().err().map(ProviderError::kind))
            .await;
        if let Ok(results) = &result
            && !self.cache_ttl.is_zero()
        {
            self.cache_put(&key, results).await;
        }
        result
    }

    async fn resolve(
        &self,
        ctx: &ProviderCtx,
        result: &SearchResult,
    ) -> Result<DownloadTarget, ProviderError> {
        tokio::time::timeout(self.timeout, self.inner.resolve(ctx, result))
            .await
            .unwrap_or(Err(ProviderError::Timeout))
    }

    fn settings(&self) -> Vec<crate::definition::spec::SettingDef> {
        self.inner.settings()
    }

    async fn test(&self, ctx: &ProviderCtx) -> Result<usize, ProviderError> {
        let started = Instant::now();
        let result = tokio::time::timeout(self.timeout, self.inner.test(ctx))
            .await
            .unwrap_or(Err(ProviderError::Timeout));
        self.record(started, result.as_ref().err().map(ProviderError::kind))
            .await;
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::health::HealthState;
    use crate::provider::{Capabilities, ProviderKind};
    use crate::registry::{self, ProviderRecord};

    #[derive(Debug)]
    struct Counting {
        info: ProviderInfo,
        calls: AtomicUsize,
        delay: Duration,
    }

    #[async_trait]
    impl SearchProvider for Counting {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn search(
            &self,
            _: &ProviderCtx,
            query: &SearchQuery,
        ) -> Result<Vec<SearchResult>, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            Ok(vec![SearchResult::new("p".into(), query.text.clone())])
        }
    }

    async fn setup(delay: Duration, ttl: Duration) -> (Store, Arc<Counting>, Instrumented) {
        let store = Store::open_in_memory().await.unwrap();
        registry::insert(&store, &ProviderRecord::new("p", ProviderKind::Native, "P"))
            .await
            .unwrap();
        let inner = Arc::new(Counting {
            info: ProviderInfo {
                id: "p".into(),
                name: "P".into(),
                kind: ProviderKind::Native,
                description: String::new(),
                categories: vec![],
                capabilities: Capabilities {
                    text_search: true,
                    imdb_search: false,
                    paging: false,
                },
            },
            calls: AtomicUsize::new(0),
            delay,
        });
        let wrapped = Instrumented::new(
            inner.clone(),
            store.clone(),
            ttl,
            Duration::from_millis(100),
        );
        (store, inner, wrapped)
    }

    #[tokio::test]
    async fn caches_identical_queries() {
        let (store, inner, wrapped) = setup(Duration::ZERO, Duration::from_secs(60)).await;
        let ctx = ProviderCtx::new().unwrap();

        wrapped
            .search(&ctx, &SearchQuery::text("Ubuntu"))
            .await
            .unwrap();
        let again = wrapped
            .search(&ctx, &SearchQuery::text(" ubuntu "))
            .await
            .unwrap();
        assert_eq!(again[0].title, "Ubuntu");
        assert_eq!(
            inner.calls.load(Ordering::SeqCst),
            1,
            "second call served from cache"
        );

        wrapped
            .search(&ctx, &SearchQuery::text("debian"))
            .await
            .unwrap();
        assert_eq!(inner.calls.load(Ordering::SeqCst), 2);

        let health = health::summary(&store, "p").await.unwrap();
        assert_eq!(health.state, HealthState::Ok);
    }

    #[tokio::test]
    async fn zero_ttl_disables_cache() {
        let (_, inner, wrapped) = setup(Duration::ZERO, Duration::ZERO).await;
        let ctx = ProviderCtx::new().unwrap();
        wrapped.search(&ctx, &SearchQuery::text("x")).await.unwrap();
        wrapped.search(&ctx, &SearchQuery::text("x")).await.unwrap();
        assert_eq!(inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn timeouts_are_recorded_in_health() {
        let (store, _, wrapped) = setup(Duration::from_secs(5), Duration::ZERO).await;
        let err = wrapped
            .search(&ProviderCtx::new().unwrap(), &SearchQuery::text("x"))
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Timeout));
        let health = health::summary(&store, "p").await.unwrap();
        assert_eq!(health.last_error_kind, Some(ErrorKind::Timeout));
    }

    #[test]
    fn cache_key_ignores_sort_and_normalises_text() {
        let mut a = SearchQuery::text("Ubuntu ");
        a.categories = vec![
            crate::model::Category::Software,
            crate::model::Category::Other,
        ];
        let mut b = SearchQuery::text("ubuntu");
        b.categories = vec![
            crate::model::Category::Other,
            crate::model::Category::Software,
        ];
        b.sort = crate::model::SortOrder::Seeders;
        assert_eq!(cache_key("p", &a), cache_key("p", &b));
        assert_ne!(cache_key("p", &a), cache_key("q", &a));
    }
}
