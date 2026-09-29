// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs a search across providers in parallel and streams merged results.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use time::OffsetDateTime;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, ErrorKind, ProviderError, Result};
use crate::model::{DownloadTarget, MergedResult, ProviderId, SearchQuery, SearchResult};
use crate::provider::{ProviderCtx, SearchProvider};
use crate::ranking;

/// Progress of a running search, in the order it happens. Front ends render
/// these as they arrive; the HTTP API forwards them as SSE events.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SearchEvent {
    ProviderStarted {
        provider: ProviderId,
    },
    /// New or updated merged results. An item whose `id` was sent before
    /// replaces the earlier version.
    Results {
        provider: ProviderId,
        items: Vec<MergedResult>,
    },
    ProviderFinished {
        provider: ProviderId,
        count: usize,
        latency_ms: u64,
    },
    ProviderFailed {
        provider: ProviderId,
        error_kind: ErrorKind,
        message: String,
        latency_ms: u64,
    },
    Done {
        total: usize,
        duration_ms: u64,
    },
}

impl SearchEvent {
    /// The event's type name, as used in its `event` tag and as the SSE
    /// event name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::ProviderStarted { .. } => "provider_started",
            Self::Results { .. } => "results",
            Self::ProviderFinished { .. } => "provider_finished",
            Self::ProviderFailed { .. } => "provider_failed",
            Self::Done { .. } => "done",
        }
    }
}

/// Tuning for [`Aggregator`].
#[derive(Debug, Clone)]
pub struct AggregatorConfig {
    /// How long one provider may take before it's reported as timed out.
    pub provider_timeout: Duration,
    /// Maximum providers searched at the same time.
    pub max_concurrency: usize,
}

impl Default for AggregatorConfig {
    fn default() -> Self {
        Self {
            provider_timeout: Duration::from_secs(10),
            max_concurrency: 16,
        }
    }
}

/// Result of one provider in a completed search.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProviderOutcome {
    pub provider: ProviderId,
    pub latency_ms: u64,
    #[serde(flatten)]
    pub status: ProviderStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProviderStatus {
    Ok {
        count: usize,
    },
    Failed {
        error_kind: ErrorKind,
        message: String,
    },
}

/// A completed search: sorted results plus how each provider did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchOutcome {
    pub results: Vec<MergedResult>,
    pub providers: Vec<ProviderOutcome>,
    pub duration_ms: u64,
}

/// A provider together with the context it runs in. Each provider can have
/// its own HTTP client (proxy, cookies) and credentials.
#[derive(Debug, Clone)]
pub struct BoundProvider {
    pub provider: Arc<dyn SearchProvider>,
    pub ctx: ProviderCtx,
}

impl BoundProvider {
    pub fn new(provider: Arc<dyn SearchProvider>, ctx: ProviderCtx) -> Self {
        Self { provider, ctx }
    }

    pub fn id(&self) -> &ProviderId {
        &self.provider.info().id
    }
}

/// Fans a query out to providers and merges what comes back.
#[derive(Debug, Clone)]
pub struct Aggregator {
    providers: Arc<[BoundProvider]>,
    config: AggregatorConfig,
}

impl Aggregator {
    pub fn new(providers: Vec<BoundProvider>, config: AggregatorConfig) -> Self {
        Self {
            providers: providers.into(),
            config,
        }
    }

    /// Binds every provider to the same context.
    pub fn with_shared_ctx(
        providers: Vec<Arc<dyn SearchProvider>>,
        ctx: &ProviderCtx,
        config: AggregatorConfig,
    ) -> Self {
        Self::new(
            providers
                .into_iter()
                .map(|p| BoundProvider::new(p, ctx.clone()))
                .collect(),
            config,
        )
    }

    pub fn providers(&self) -> &[BoundProvider] {
        &self.providers
    }

    pub fn provider(&self, id: &ProviderId) -> Option<&BoundProvider> {
        self.providers.iter().find(|p| p.id() == id)
    }

    /// Starts a search and returns its event stream. The search stops early
    /// when `cancel` fires or the receiver is dropped. The last event is
    /// always [`SearchEvent::Done`] unless the search was stopped.
    pub fn search(
        &self,
        query: SearchQuery,
        cancel: CancellationToken,
    ) -> mpsc::Receiver<SearchEvent> {
        let (tx, rx) = mpsc::channel(64);
        let selected: Vec<_> = if query.is_empty() {
            Vec::new()
        } else {
            self.providers
                .iter()
                .filter(|p| p.provider.info().matches(&query))
                .cloned()
                .collect()
        };
        tokio::spawn(drive(selected, self.config.clone(), query, cancel, tx));
        rx
    }

    /// Runs a search to completion and returns the results sorted by the
    /// query's sort order.
    pub async fn search_all(&self, query: SearchQuery) -> SearchOutcome {
        let order = query.sort;
        let mut rx = self.search(query, CancellationToken::new());
        let mut latest: HashMap<String, MergedResult> = HashMap::new();
        let mut providers = Vec::new();
        let mut duration_ms = 0;

        while let Some(event) = rx.recv().await {
            match event {
                SearchEvent::ProviderStarted { .. } => {}
                SearchEvent::Results { items, .. } => {
                    for item in items {
                        latest.insert(item.id.clone(), item);
                    }
                }
                SearchEvent::ProviderFinished {
                    provider,
                    count,
                    latency_ms,
                } => providers.push(ProviderOutcome {
                    provider,
                    latency_ms,
                    status: ProviderStatus::Ok { count },
                }),
                SearchEvent::ProviderFailed {
                    provider,
                    error_kind,
                    message,
                    latency_ms,
                } => providers.push(ProviderOutcome {
                    provider,
                    latency_ms,
                    status: ProviderStatus::Failed {
                        error_kind,
                        message,
                    },
                }),
                SearchEvent::Done {
                    duration_ms: took, ..
                } => duration_ms = took,
            }
        }

        let mut results: Vec<_> = latest.into_values().collect();
        ranking::sort(&mut results, order);
        SearchOutcome {
            results,
            providers,
            duration_ms,
        }
    }

    /// Asks the result's provider for a download target.
    pub async fn resolve(&self, result: &SearchResult) -> Result<DownloadTarget> {
        let bound = self
            .provider(&result.provider_id)
            .ok_or_else(|| Error::UnknownProvider(result.provider_id.to_string()))?;
        tokio::time::timeout(
            self.config.provider_timeout,
            bound.provider.resolve(&bound.ctx, result),
        )
        .await
        .unwrap_or(Err(ProviderError::Timeout))
        .map_err(|source| Error::Provider {
            provider: result.provider_id.to_string(),
            source,
        })
    }
}

type TaskOutput = (Result<Vec<SearchResult>, ProviderError>, Duration);

async fn drive(
    providers: Vec<BoundProvider>,
    config: AggregatorConfig,
    query: SearchQuery,
    cancel: CancellationToken,
    tx: mpsc::Sender<SearchEvent>,
) {
    let started = Instant::now();
    let query = Arc::new(query);
    let permits = Arc::new(Semaphore::new(config.max_concurrency.max(1)));
    let mut tasks: JoinSet<TaskOutput> = JoinSet::new();
    let mut task_provider = HashMap::new();

    for BoundProvider { provider, ctx } in providers {
        let id = provider.info().id.clone();
        if tx
            .send(SearchEvent::ProviderStarted {
                provider: id.clone(),
            })
            .await
            .is_err()
        {
            return;
        }
        let (query, permits, timeout) = (
            Arc::clone(&query),
            Arc::clone(&permits),
            config.provider_timeout,
        );
        let handle = tasks.spawn(async move {
            let _permit = permits.acquire_owned().await;
            let t0 = Instant::now();
            let result = tokio::time::timeout(timeout, provider.search(&ctx, &query))
                .await
                .unwrap_or(Err(ProviderError::Timeout));
            (result, t0.elapsed())
        });
        task_provider.insert(handle.id(), id);
    }

    let mut merger = Merger::new(&query.text);
    loop {
        let joined = tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            () = tx.closed() => return,
            joined = tasks.join_next_with_id() => joined,
        };
        let Some(joined) = joined else { break };

        let events = match joined {
            Ok((task_id, (Ok(results), latency))) => {
                let provider = task_provider.remove(&task_id).expect("spawned task");
                let count = results.len();
                let items = merger.add(results);
                let mut events = Vec::with_capacity(2);
                if !items.is_empty() {
                    events.push(SearchEvent::Results {
                        provider: provider.clone(),
                        items,
                    });
                }
                events.push(SearchEvent::ProviderFinished {
                    provider,
                    count,
                    latency_ms: millis(latency),
                });
                events
            }
            Ok((task_id, (Err(err), latency))) => {
                let provider = task_provider.remove(&task_id).expect("spawned task");
                tracing::debug!(%provider, error = %err, "provider failed");
                vec![SearchEvent::ProviderFailed {
                    provider,
                    error_kind: err.kind(),
                    message: err.to_string(),
                    latency_ms: millis(latency),
                }]
            }
            Err(join_err) => {
                let provider = task_provider.remove(&join_err.id()).expect("spawned task");
                tracing::error!(%provider, error = %join_err, "provider task panicked");
                vec![SearchEvent::ProviderFailed {
                    provider,
                    error_kind: ErrorKind::Network,
                    message: "provider crashed".into(),
                    latency_ms: millis(started.elapsed()),
                }]
            }
        };
        for event in events {
            if tx.send(event).await.is_err() {
                return;
            }
        }
    }

    let _ = tx
        .send(SearchEvent::Done {
            total: merger.len(),
            duration_ms: millis(started.elapsed()),
        })
        .await;
}

fn millis(d: Duration) -> u64 {
    d.as_millis().try_into().unwrap_or(u64::MAX)
}

/// De-duplicates results across providers.
struct Merger {
    query_text: String,
    now: OffsetDateTime,
    index: HashMap<String, usize>,
    results: Vec<MergedResult>,
}

impl Merger {
    fn new(query_text: &str) -> Self {
        Self {
            query_text: query_text.to_owned(),
            now: OffsetDateTime::now_utc(),
            index: HashMap::new(),
            results: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.results.len()
    }

    /// Merges a batch and returns the new or changed merged results.
    fn add(&mut self, batch: Vec<SearchResult>) -> Vec<MergedResult> {
        let mut changed: Vec<usize> = Vec::new();
        for result in batch {
            let key = merge_key(&result);
            let idx = match self.index.get(&key) {
                Some(&idx) => {
                    merge_into(&mut self.results[idx], result);
                    idx
                }
                None => {
                    let idx = self.results.len();
                    self.results.push(MergedResult {
                        id: format!("r_{:016x}", crate::util::fnv1a64(&key)),
                        sources: vec![result.provider_id.clone()],
                        seeders: result.seeders,
                        leechers: result.leechers,
                        primary: result,
                        score: 0.0,
                    });
                    self.index.insert(key, idx);
                    idx
                }
            };
            let merged = &mut self.results[idx];
            merged.score = ranking::score(&self.query_text, merged, self.now);
            if !changed.contains(&idx) {
                changed.push(idx);
            }
        }
        changed
            .into_iter()
            .map(|i| self.results[i].clone())
            .collect()
    }
}

/// Results with the same infohash are the same torrent. Without one, fall
/// back to the details page, then to provider + title.
fn merge_key(result: &SearchResult) -> String {
    if let Some(hash) = &result.info_hash {
        format!("btih:{hash}")
    } else if let Some(url) = &result.details_url {
        format!("url:{url}")
    } else {
        format!(
            "title:{}:{}",
            result.provider_id,
            ranking::tokens(&result.title).join(" ")
        )
    }
}

/// Folds another source's view of a torrent into `merged`: counts take the
/// maximum, and fields the primary lacks are filled in. The first title seen
/// is kept.
fn merge_into(merged: &mut MergedResult, other: SearchResult) {
    if !merged.sources.contains(&other.provider_id) {
        merged.sources.push(other.provider_id.clone());
    }
    merged.seeders = merged.seeders.max(other.seeders);
    merged.leechers = merged.leechers.max(other.leechers);

    let p = &mut merged.primary;
    if p.title.is_empty() {
        p.title = other.title;
    }
    p.size_bytes = p.size_bytes.or(other.size_bytes);
    p.info_hash = p.info_hash.or(other.info_hash);
    p.magnet = p.magnet.take().or(other.magnet);
    p.torrent_url = p.torrent_url.take().or(other.torrent_url);
    p.details_url = p.details_url.take().or(other.details_url);
    p.published = p.published.or(other.published);
    p.category = p.category.or(other.category);
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::model::Category;
    use crate::provider::{Capabilities, ProviderInfo, ProviderKind};

    const HASH_A: &str = "1dc689206d6f6ef6021d558d95350ed09004b40c";

    #[derive(Debug)]
    struct Fake {
        info: ProviderInfo,
        delay: Duration,
        outcome: std::result::Result<Vec<SearchResult>, ProviderError>,
    }

    impl Fake {
        fn ok(id: &str, results: Vec<SearchResult>) -> Arc<dyn SearchProvider> {
            Self::with(id, Duration::ZERO, Ok(results))
        }

        fn with(
            id: &str,
            delay: Duration,
            outcome: std::result::Result<Vec<SearchResult>, ProviderError>,
        ) -> Arc<dyn SearchProvider> {
            Arc::new(Self {
                info: ProviderInfo {
                    id: id.into(),
                    name: id.into(),
                    kind: ProviderKind::Native,
                    description: String::new(),
                    categories: vec![Category::Software],
                    capabilities: Capabilities {
                        text_search: true,
                        imdb_search: false,
                        paging: false,
                    },
                },
                delay,
                outcome,
            })
        }
    }

    #[async_trait]
    impl SearchProvider for Fake {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn search(
            &self,
            _: &ProviderCtx,
            _: &SearchQuery,
        ) -> std::result::Result<Vec<SearchResult>, ProviderError> {
            tokio::time::sleep(self.delay).await;
            self.outcome.clone()
        }
    }

    fn result(
        provider: &str,
        title: &str,
        hash: Option<&str>,
        seeders: Option<u32>,
    ) -> SearchResult {
        SearchResult {
            info_hash: hash.map(|h| h.parse().unwrap()),
            seeders,
            ..SearchResult::new(provider.into(), title)
        }
    }

    fn aggregator(providers: Vec<Arc<dyn SearchProvider>>) -> Aggregator {
        Aggregator::with_shared_ctx(
            providers,
            &ProviderCtx::new().unwrap(),
            AggregatorConfig {
                provider_timeout: Duration::from_millis(200),
                max_concurrency: 4,
            },
        )
    }

    #[tokio::test]
    async fn merges_same_infohash_across_providers() {
        let agg = aggregator(vec![
            Fake::ok(
                "a",
                vec![result("a", "Ubuntu 24.04", Some(HASH_A), Some(10))],
            ),
            Fake::ok(
                "b",
                vec![SearchResult {
                    size_bytes: Some(6_000_000_000),
                    ..result(
                        "b",
                        "ubuntu-24.04-desktop",
                        Some(&HASH_A.to_uppercase()),
                        Some(40),
                    )
                }],
            ),
            Fake::ok("c", vec![result("c", "Ubuntu 22.04", None, Some(5))]),
        ]);

        let outcome = agg.search_all(SearchQuery::text("ubuntu 24.04")).await;

        assert_eq!(outcome.results.len(), 2);
        let top = &outcome.results[0];
        assert_eq!(top.seeders, Some(40));
        assert_eq!(top.primary.size_bytes, Some(6_000_000_000), "filled from b");
        assert_eq!(top.sources.len(), 2);
        assert!(top.id.starts_with("r_"));
        assert_eq!(outcome.providers.len(), 3);
    }

    #[tokio::test]
    async fn reports_timeouts_and_errors_without_failing_the_search() {
        let agg = aggregator(vec![
            Fake::ok("fast", vec![result("fast", "ubuntu", None, None)]),
            Fake::with("slow", Duration::from_secs(5), Ok(vec![])),
            Fake::with(
                "broken",
                Duration::ZERO,
                Err(ProviderError::Http { status: 500 }),
            ),
        ]);

        let outcome = agg.search_all(SearchQuery::text("ubuntu")).await;

        assert_eq!(outcome.results.len(), 1);
        let status_of = |id: &str| {
            outcome
                .providers
                .iter()
                .find(|p| p.provider.as_str() == id)
                .map(|p| p.status.clone())
                .unwrap()
        };
        assert_eq!(status_of("fast"), ProviderStatus::Ok { count: 1 });
        assert!(matches!(
            status_of("slow"),
            ProviderStatus::Failed {
                error_kind: ErrorKind::Timeout,
                ..
            }
        ));
        assert!(matches!(
            status_of("broken"),
            ProviderStatus::Failed {
                error_kind: ErrorKind::Http,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn streams_events_in_order_and_ends_with_done() {
        let agg = aggregator(vec![Fake::ok("a", vec![result("a", "ubuntu", None, None)])]);
        let mut rx = agg.search(SearchQuery::text("ubuntu"), CancellationToken::new());
        let mut kinds = Vec::new();
        while let Some(event) = rx.recv().await {
            kinds.push(match event {
                SearchEvent::ProviderStarted { .. } => "started",
                SearchEvent::Results { .. } => "results",
                SearchEvent::ProviderFinished { .. } => "finished",
                SearchEvent::ProviderFailed { .. } => "failed",
                SearchEvent::Done { total, .. } => {
                    assert_eq!(total, 1);
                    "done"
                }
            });
        }
        assert_eq!(kinds, ["started", "results", "finished", "done"]);
    }

    #[tokio::test]
    async fn skips_providers_that_do_not_match_the_query() {
        let agg = aggregator(vec![Fake::ok("a", vec![result("a", "x", None, None)])]);
        let query = SearchQuery {
            categories: vec![Category::Music],
            ..SearchQuery::text("x")
        };
        let outcome = agg.search_all(query).await;
        assert!(outcome.results.is_empty());
        assert!(outcome.providers.is_empty());

        let empty = agg.search_all(SearchQuery::text("   ")).await;
        assert!(empty.providers.is_empty());
    }

    #[tokio::test]
    async fn cancellation_stops_the_stream() {
        let agg = aggregator(vec![Fake::with("slow", Duration::from_secs(5), Ok(vec![]))]);
        let cancel = CancellationToken::new();
        let mut rx = agg.search(SearchQuery::text("x"), cancel.clone());
        assert!(matches!(
            rx.recv().await,
            Some(SearchEvent::ProviderStarted { .. })
        ));
        cancel.cancel();
        assert_eq!(rx.recv().await, None, "no Done after cancellation");
    }

    #[tokio::test]
    async fn resolve_routes_to_the_result_provider() {
        let agg = aggregator(vec![Fake::ok("a", vec![])]);
        let hit = result("a", "Ubuntu", Some(HASH_A), None);
        let target = agg.resolve(&hit).await.unwrap();
        assert!(matches!(target, DownloadTarget::Magnet(m) if m.contains(HASH_A)));

        let orphan = result("missing", "x", Some(HASH_A), None);
        assert!(matches!(
            agg.resolve(&orphan).await,
            Err(Error::UnknownProvider(id)) if id == "missing"
        ));
    }
}
