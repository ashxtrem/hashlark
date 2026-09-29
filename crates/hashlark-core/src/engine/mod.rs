// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine: everything a front end needs, behind one handle. The HTTP
//! server, the desktop app and (later) the Android bindings all drive
//! Hashlark through this type.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::aggregator::{Aggregator, AggregatorConfig, BoundProvider, SearchEvent, SearchOutcome};
use crate::error::{Error, Result};
use crate::magnet::{add_trackers, build_magnet};
use crate::model::{DownloadTarget, MergedResult, SearchQuery, TargetKind};
use crate::paths::AppPaths;
use crate::provider::{DefinitionProvider, ProviderCtx, SearchProvider, builtin_providers};
use crate::ranking;
use crate::registry;
use crate::secrets::SecretStore;
use crate::settings::Settings;
use crate::store::{Store, now_ms};

mod apikeys;
mod library;
mod providers;
mod repos;

pub use apikeys::{ApiKeyInfo, NewApiKey};

pub use library::{Favorite, FetchedPage, parse_tracker_list};
pub use providers::{
    DefinitionCheck, ProviderPatch, ProviderSource, ProviderView, Session, SessionCookie,
    SettingView, TestReport,
};
pub use repos::{RepoView, SyncReport};

/// Merged results kept for `resolve` and result lookups.
const RESULT_CACHE_CAPACITY: usize = 20_000;
/// Search history entries kept.
const HISTORY_LIMIT: i64 = 500;
/// Extra time the aggregator allows beyond the provider timeout, as a
/// backstop; the provider timeout itself is enforced by [`Instrumented`].
const AGGREGATOR_GRACE: Duration = Duration::from_secs(2);

/// Options for [`Engine::open`].
#[derive(Debug)]
pub struct EngineOptions {
    pub secrets: Arc<dyn SecretStore>,
    /// Compiled-in providers. Defaults to [`builtin_providers`].
    pub builtins: Vec<Arc<dyn SearchProvider>>,
    /// First-party definitions installed on start. Defaults to the ones
    /// shipped with Hashlark (legal sources only, ADR 0007).
    pub definitions: Vec<String>,
}

impl EngineOptions {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            secrets,
            builtins: builtin_providers(),
            definitions: providers::first_party_definitions(),
        }
    }
}

/// One saved search.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HistoryEntry {
    pub id: i64,
    pub query: SearchQuery,
    /// Unix ms.
    pub ts: i64,
    pub result_count: i64,
}

/// Handle to a running Hashlark instance. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    paths: AppPaths,
    store: Store,
    secrets: Arc<dyn SecretStore>,
    builtins: Vec<Arc<dyn SearchProvider>>,
    settings: RwLock<Settings>,
    /// Network settings prepared for building clients.
    network: RwLock<crate::net::Network>,
    runtime: RwLock<Arc<Runtime>>,
    results: Mutex<ResultCache>,
    /// Per-provider cookie jars, kept across rebuilds so logins and
    /// challenge clearances survive settings changes.
    jars: Mutex<HashMap<String, Arc<reqwest::cookie::Jar>>>,
    /// Loaded definitions by provider id, with the hash of their YAML, so
    /// caches (static feeds, logins) survive rebuilds.
    definitions: Mutex<HashMap<String, (String, Arc<DefinitionProvider>)>>,
    /// Keeps the hot-reload watcher alive.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// Built-in Tor, started when first needed.
    #[cfg(feature = "tor")]
    tor: tokio::sync::Mutex<Option<Arc<crate::net::tor::EmbeddedTor>>>,
}

/// Instantiated providers, rebuilt whenever providers or settings change.
#[derive(Debug, Default)]
struct Runtime {
    entries: HashMap<String, std::result::Result<BoundProvider, String>>,
}

impl Engine {
    /// Opens the database under `paths` and loads providers and settings.
    pub async fn open(paths: AppPaths, options: EngineOptions) -> Result<Self> {
        paths.ensure_dirs()?;
        let store = Store::open(&paths.db_path()).await?;
        Self::with_store(paths, store, options).await
    }

    /// Like [`open`](Self::open) with an existing store (e.g. in-memory for tests).
    pub async fn with_store(paths: AppPaths, store: Store, options: EngineOptions) -> Result<Self> {
        for provider in &options.builtins {
            registry::ensure_builtin(&store, provider.info()).await?;
        }
        providers::install_first_party(&store, &options.definitions).await?;
        let settings = Settings::load(&store).await?;
        let network = network_for(&settings, None);
        let engine = Self {
            inner: Arc::new(Inner {
                paths,
                store,
                secrets: options.secrets,
                builtins: options.builtins,
                settings: RwLock::new(settings),
                network: RwLock::new(network),
                runtime: RwLock::new(Arc::default()),
                results: Mutex::new(ResultCache::default()),
                jars: Mutex::default(),
                definitions: Mutex::default(),
                watcher: Mutex::default(),
                #[cfg(feature = "tor")]
                tor: tokio::sync::Mutex::default(),
            }),
        };
        engine.refresh_network().await;
        engine.rebuild().await?;
        Ok(engine)
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    pub fn paths(&self) -> &AppPaths {
        &self.inner.paths
    }

    pub fn secrets(&self) -> &Arc<dyn SecretStore> {
        &self.inner.secrets
    }

    // ----- settings ---------------------------------------------------------

    pub fn settings(&self) -> Settings {
        self.inner.settings.read().expect("lock").clone()
    }

    /// Validates, saves and applies new settings.
    pub async fn set_settings(&self, settings: Settings) -> Result<Settings> {
        settings.validate()?;
        // Check the network settings can be applied before saving them.
        crate::net::Network::from_settings(&settings.network, None)?;
        let trackers_changed =
            settings.magnets.trackers_url != self.settings().magnets.trackers_url;
        settings.save(&self.inner.store).await?;
        crate::net::set_rate_limit(settings.network.per_host_rate);
        *self.inner.settings.write().expect("lock") = settings.clone();
        self.refresh_network().await;
        self.rebuild().await?;
        if trackers_changed {
            let engine = self.clone();
            tokio::spawn(async move {
                if let Err(e) = engine.refresh_trackers().await {
                    tracing::warn!(error = %e, "tracker list refresh failed");
                }
            });
        }
        Ok(settings)
    }

    /// Starts built-in Tor if the settings or a provider now need it, and
    /// applies the network settings.
    pub(crate) async fn refresh_network(&self) {
        let settings = self.settings();
        let embedded = self.embedded_tor_url(&settings).await;
        *self.inner.network.write().expect("lock") = network_for(&settings, embedded);
    }

    #[cfg(feature = "tor")]
    async fn embedded_tor_url(&self, settings: &Settings) -> Option<Url> {
        use crate::settings::TorMode;
        if settings.network.tor.mode != TorMode::Embedded {
            return None;
        }
        let provider_wants_tor = registry::list(&self.inner.store)
            .await
            .map(|list| {
                list.iter()
                    .any(|r| r.enabled && r.network_policy.route == registry::Route::Tor)
            })
            .unwrap_or(false);
        let mut tor = self.inner.tor.lock().await;
        if tor.is_none() && (settings.network.tor.enabled || provider_wants_tor) {
            match crate::net::tor::EmbeddedTor::start(&self.inner.paths.data_dir().join("tor"))
                .await
            {
                Ok(started) => *tor = Some(started),
                Err(e) => tracing::error!(error = %e, "could not start built-in Tor"),
            }
        }
        tor.as_ref().map(|t| t.socks_url())
    }

    #[cfg(not(feature = "tor"))]
    async fn embedded_tor_url(&self, _settings: &Settings) -> Option<Url> {
        None
    }

    /// State of built-in Tor.
    pub async fn tor_status(&self) -> crate::net::TorStatus {
        #[cfg(feature = "tor")]
        if let Some(tor) = self.inner.tor.lock().await.as_ref() {
            return tor.status();
        }
        crate::net::TorStatus::stopped()
    }

    pub(crate) fn network(&self) -> crate::net::Network {
        self.inner.network.read().expect("lock").clone()
    }

    /// A client for Hashlark's own requests (downloads, repositories,
    /// tracker lists), following the global network settings.
    pub fn http(&self) -> Result<reqwest::Client> {
        let (options, _) = self
            .network()
            .client_options(&registry::NetworkPolicy::default())?;
        crate::net::build_client(&options)
    }

    // ----- search -----------------------------------------------------------

    async fn aggregator(&self) -> Result<Aggregator> {
        let now = now_ms();
        let runtime = self.runtime();
        let active: Vec<BoundProvider> = registry::list(&self.inner.store)
            .await?
            .into_iter()
            .filter(|r| r.is_active(now))
            .filter_map(|r| runtime.entries.get(&r.id)?.as_ref().ok().cloned())
            .collect();
        let search = self.settings().search;
        Ok(Aggregator::new(
            active,
            AggregatorConfig {
                provider_timeout: search.provider_timeout() + AGGREGATOR_GRACE,
                max_concurrency: search.max_concurrency,
            },
        ))
    }

    /// Starts a search. Events stream through the returned receiver; dropping
    /// it (or cancelling) stops the search.
    pub async fn search(
        &self,
        query: SearchQuery,
        cancel: CancellationToken,
    ) -> Result<mpsc::Receiver<SearchEvent>> {
        if query.is_empty() {
            return Err(Error::Invalid("the search query is empty".into()));
        }
        if query.page == 0 {
            return Err(Error::Invalid("page numbers start at 1".into()));
        }
        let mut upstream = self.aggregator().await?.search(query.clone(), cancel);
        let (tx, rx) = mpsc::channel(64);
        let engine = self.clone();
        tokio::spawn(async move {
            while let Some(event) = upstream.recv().await {
                match &event {
                    SearchEvent::Results { items, .. } => engine.remember(items),
                    SearchEvent::Done { total, .. } => {
                        if let Err(e) = engine.record_history(&query, *total).await {
                            tracing::warn!(error = %e, "could not save search history");
                        }
                    }
                    _ => {}
                }
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }

    /// Runs a search to completion; results are sorted by the query's order.
    pub async fn search_all(&self, query: SearchQuery) -> Result<SearchOutcome> {
        let order = query.sort;
        let mut rx = self.search(query, CancellationToken::new()).await?;
        let mut outcome = collect(&mut rx).await;
        ranking::sort(&mut outcome.results, order);
        Ok(outcome)
    }

    fn remember(&self, items: &[MergedResult]) {
        let mut cache = self.inner.results.lock().expect("lock");
        for item in items {
            cache.insert(item.clone());
        }
    }

    /// A result from a recent search, or a saved favourite.
    pub async fn result(&self, id: &str) -> Result<MergedResult> {
        let cached = self.inner.results.lock().expect("lock").get(id).cloned();
        match cached {
            Some(result) => Ok(result),
            None => self
                .favorite(id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("result `{id}` (search again)"))),
        }
    }

    /// Gets a download target for a result from a recent search.
    ///
    /// With `prefer`, a target of that kind is returned when the result
    /// allows it (e.g. a magnet built from the infohash); otherwise the
    /// provider decides.
    pub async fn resolve(
        &self,
        result_id: &str,
        prefer: Option<TargetKind>,
    ) -> Result<DownloadTarget> {
        let result = self.result(result_id).await?;
        let provider_id = result.primary.provider_id.to_string();
        let bound = self.bound(&provider_id)?;
        let p = &result.primary;
        let preferred = match prefer {
            Some(TargetKind::Magnet) => p
                .magnet
                .clone()
                .or_else(|| {
                    p.info_hash
                        .map(|hash| build_magnet(&hash, &p.title, &bound.ctx.trackers))
                })
                .map(DownloadTarget::Magnet),
            Some(TargetKind::TorrentFile) if !p.needs_resolve => {
                p.torrent_url.clone().map(DownloadTarget::TorrentFile)
            }
            _ => None,
        };
        let target = match preferred {
            Some(target) => target,
            None => bound
                .provider
                .resolve(&bound.ctx, &result.primary)
                .await
                .map_err(|source| Error::Provider {
                    provider: provider_id,
                    source,
                })?,
        };
        Ok(match target {
            DownloadTarget::Magnet(magnet) if self.settings().magnets.append_default_trackers => {
                DownloadTarget::Magnet(add_trackers(&magnet, &bound.ctx.trackers))
            }
            other => other,
        })
    }

    // ----- downloads --------------------------------------------------------

    /// Folder `.torrent` files are saved to: the configured one, else the
    /// OS downloads folder, else the data directory.
    pub fn download_dir(&self) -> PathBuf {
        self.settings()
            .downloads
            .torrent_dir
            .or_else(|| {
                directories::UserDirs::new()?
                    .download_dir()
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(|| self.inner.paths.data_dir().to_path_buf())
    }

    /// Downloads a `.torrent` file and saves it under [`download_dir`],
    /// named after `title`. Returns the saved path.
    ///
    /// [`download_dir`]: Self::download_dir
    pub async fn save_torrent(&self, url: &Url, title: &str) -> Result<PathBuf> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::Invalid(
                "only http(s) .torrent URLs can be downloaded".into(),
            ));
        }
        let ctx = ProviderCtx::with_client(self.http()?);
        let to_error = |source| Error::Provider {
            provider: url.host_str().unwrap_or("download").to_owned(),
            source,
        };
        let mut response = crate::provider::send(ctx.http.get(url.clone()))
            .await
            .map_err(to_error)?;
        if response
            .content_length()
            .is_some_and(|len| len > MAX_TORRENT_BYTES)
        {
            return Err(Error::Invalid("the .torrent file is too large".into()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| to_error(crate::error::ProviderError::Network(e.to_string())))?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() as u64 > MAX_TORRENT_BYTES {
                return Err(Error::Invalid("the .torrent file is too large".into()));
            }
        }
        // A .torrent is a bencoded dictionary, which always starts with `d`.
        if bytes.first() != Some(&b'd') {
            return Err(Error::Invalid(
                "the server did not return a .torrent file".into(),
            ));
        }

        let dir = self.download_dir();
        tokio::fs::create_dir_all(&dir).await?;
        let path = unique_path(&dir, &sanitize_file_name(title), "torrent");
        tokio::fs::write(&path, &bytes).await?;
        Ok(path)
    }

    // ----- history ----------------------------------------------------------

    async fn record_history(&self, query: &SearchQuery, total: usize) -> Result<()> {
        if !self.settings().search.save_history {
            return Ok(());
        }
        let store = &self.inner.store;
        sqlx::query("INSERT INTO search_history (query_json, ts, result_count) VALUES (?, ?, ?)")
            .bind(serde_json::to_string(query)?)
            .bind(now_ms())
            .bind(i64::try_from(total).unwrap_or(i64::MAX))
            .execute(store.pool())
            .await?;
        sqlx::query(
            "DELETE FROM search_history WHERE id NOT IN (
                 SELECT id FROM search_history ORDER BY ts DESC, id DESC LIMIT ?)",
        )
        .bind(HISTORY_LIMIT)
        .execute(store.pool())
        .await?;
        Ok(())
    }

    pub async fn history(&self, limit: u32) -> Result<Vec<HistoryEntry>> {
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT id, query_json, ts, result_count FROM search_history
             ORDER BY ts DESC, id DESC LIMIT ?",
        )
        .bind(i64::from(limit.min(500)))
        .fetch_all(self.inner.store.pool())
        .await?;
        rows.iter()
            .map(|row| {
                let json: String = row.try_get("query_json")?;
                Ok(HistoryEntry {
                    id: row.try_get("id")?,
                    query: serde_json::from_str(&json)?,
                    ts: row.try_get("ts")?,
                    result_count: row.try_get("result_count")?,
                })
            })
            .collect()
    }

    pub async fn clear_history(&self) -> Result<()> {
        sqlx::query("DELETE FROM search_history")
            .execute(self.inner.store.pool())
            .await?;
        Ok(())
    }
}

/// Drains a search stream into an outcome (unsorted).
pub async fn collect(rx: &mut mpsc::Receiver<SearchEvent>) -> SearchOutcome {
    use crate::aggregator::{ProviderOutcome, ProviderStatus};

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
    SearchOutcome {
        results: latest.into_values().collect(),
        providers,
        duration_ms,
    }
}

/// Network settings, or direct connections if they can't be applied (the
/// user can fix them in Settings).
fn network_for(settings: &Settings, embedded_tor: Option<Url>) -> crate::net::Network {
    crate::net::set_rate_limit(settings.network.per_host_rate);
    crate::net::Network::from_settings(&settings.network, embedded_tor).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "network settings could not be applied; using direct connections");
        crate::net::Network::direct()
    })
}

/// Upper bound for a downloaded `.torrent` file.
const MAX_TORRENT_BYTES: u64 = 10 * 1024 * 1024;

/// Makes `title` safe as a file name on every OS.
fn sanitize_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '[' | ']') {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        "download".to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// `dir/stem.ext`, or `dir/stem (2).ext` etc. if that exists.
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .expect("some free file name")
}

/// Bounded map of recent merged results, evicting the oldest.
#[derive(Debug, Default)]
struct ResultCache {
    items: HashMap<String, MergedResult>,
    order: VecDeque<String>,
}

impl ResultCache {
    fn insert(&mut self, item: MergedResult) {
        if self.items.insert(item.id.clone(), item.clone()).is_none() {
            self.order.push_back(item.id);
            while self.order.len() > RESULT_CACHE_CAPACITY {
                if let Some(old) = self.order.pop_front() {
                    self.items.remove(&old);
                }
            }
        }
    }

    fn get(&self, id: &str) -> Option<&MergedResult> {
        self.items.get(id)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::error::ErrorKind;
    use crate::error::ProviderError;
    use crate::health;
    use crate::model::SearchResult;
    use crate::provider::ProviderInfo;
    use crate::provider::{Capabilities, ProviderKind};
    use crate::registry::NetworkPolicy;
    use crate::secrets::MemorySecretStore;

    const HASH: &str = "1dc689206d6f6ef6021d558d95350ed09004b40c";

    #[derive(Debug)]
    struct Fixed(ProviderInfo, Option<ProviderError>);

    #[async_trait]
    impl SearchProvider for Fixed {
        fn info(&self) -> &ProviderInfo {
            &self.0
        }

        async fn search(
            &self,
            _: &ProviderCtx,
            query: &SearchQuery,
        ) -> std::result::Result<Vec<SearchResult>, ProviderError> {
            if let Some(e) = &self.1 {
                return Err(e.clone());
            }
            Ok(vec![SearchResult {
                info_hash: Some(HASH.parse().unwrap()),
                ..SearchResult::new(self.0.id.clone(), format!("{} result", query.text))
            }])
        }
    }

    fn fixed(id: &str, error: Option<ProviderError>) -> Arc<dyn SearchProvider> {
        Arc::new(Fixed(
            ProviderInfo {
                id: id.into(),
                name: id.to_uppercase(),
                kind: ProviderKind::Native,
                description: "test".into(),
                categories: vec![],
                capabilities: Capabilities {
                    text_search: true,
                    imdb_search: false,
                    paging: false,
                },
            },
            error,
        ))
    }

    async fn engine(builtins: Vec<Arc<dyn SearchProvider>>) -> Engine {
        let store = Store::open_in_memory().await.unwrap();
        let dir = std::env::temp_dir().join("hashlark-engine-tests");
        Engine::with_store(
            AppPaths::at(dir),
            store,
            EngineOptions {
                secrets: Arc::new(MemorySecretStore::default()),
                builtins,
                definitions: Vec::new(),
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn search_resolve_and_history() {
        let engine = engine(vec![fixed("a", None)]).await;
        let outcome = engine
            .search_all(SearchQuery::text("ubuntu"))
            .await
            .unwrap();
        assert_eq!(outcome.results.len(), 1);
        let result = &outcome.results[0];

        assert_eq!(engine.result(&result.id).await.unwrap().id, result.id);
        let target = engine.resolve(&result.id, None).await.unwrap();
        assert!(matches!(target, DownloadTarget::Magnet(m) if m.contains(HASH)));
        let magnet = engine
            .resolve(&result.id, Some(TargetKind::Magnet))
            .await
            .unwrap();
        assert!(matches!(magnet, DownloadTarget::Magnet(m) if m.contains(HASH)));
        assert!(matches!(
            engine.resolve("nope", None).await,
            Err(Error::NotFound(_))
        ));

        let history = engine.history(10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].query.text, "ubuntu");
        assert_eq!(history[0].result_count, 1);
        engine.clear_history().await.unwrap();
        assert!(engine.history(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn disabled_providers_are_skipped() {
        let engine = engine(vec![fixed("a", None), fixed("b", None)]).await;
        engine
            .update_provider(
                "b",
                ProviderPatch {
                    enabled: Some(false),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let outcome = engine.search_all(SearchQuery::text("x")).await.unwrap();
        assert_eq!(outcome.providers.len(), 1);
        assert_eq!(outcome.providers[0].provider.as_str(), "a");

        let views = engine.providers().await.unwrap();
        assert_eq!(views.len(), 2);
        assert!(views.iter().all(|v| v.builtin));
        assert!(!views.iter().find(|v| v.id == "b").unwrap().enabled);
    }

    #[tokio::test]
    async fn builtins_cannot_be_removed_and_validation_applies() {
        let engine = engine(vec![fixed("a", None)]).await;
        assert!(matches!(
            engine.remove_provider("a").await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            engine.search_all(SearchQuery::text("  ")).await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            engine.provider("zzz").await,
            Err(Error::NotFound(_))
        ));
        let patch = ProviderPatch {
            network_policy: Some(NetworkPolicy {
                route: registry::Route::Proxy,
                proxy: None,
            }),
            ..Default::default()
        };
        assert!(matches!(
            engine.update_provider("a", patch).await,
            Err(Error::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn test_provider_reports_and_records_failures() {
        let engine = engine(vec![fixed(
            "bad",
            Some(ProviderError::Http { status: 502 }),
        )])
        .await;
        let report = engine.test_provider("bad").await.unwrap();
        assert!(!report.ok);
        assert_eq!(report.error_kind, Some(ErrorKind::Http));
        let view = engine.provider("bad").await.unwrap();
        assert_eq!(view.health.state, health::HealthState::Failing);
    }

    #[tokio::test]
    async fn settings_round_trip_and_validate() {
        let engine = engine(vec![]).await;
        let mut settings = engine.settings();
        settings.search.cache_ttl_secs = 0;
        engine.set_settings(settings.clone()).await.unwrap();
        assert_eq!(engine.settings(), settings);

        settings.search.max_concurrency = 0;
        assert!(matches!(
            engine.set_settings(settings).await,
            Err(Error::Invalid(_))
        ));
        assert_eq!(
            engine.settings().search.max_concurrency,
            16,
            "rejected settings not applied"
        );
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            sanitize_file_name("Ubuntu 24.04: desktop/amd64?"),
            "Ubuntu 24.04_ desktop_amd64_"
        );
        assert_eq!(sanitize_file_name("..."), "download");
        assert_eq!(sanitize_file_name("  "), "download");
        assert_eq!(sanitize_file_name(&"x".repeat(500)).len(), 120);
    }

    #[test]
    fn unique_path_avoids_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "a", "torrent");
        std::fs::write(&first, b"d").unwrap();
        let second = unique_path(dir.path(), "a", "torrent");
        assert_eq!(second.file_name().unwrap(), "a (2).torrent");
    }

    #[tokio::test]
    async fn save_torrent_validates_and_writes() {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(path("/ok.torrent"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(b"d4:infod4:name1:xee".to_vec()),
            )
            .mount(&server)
            .await;
        Mock::given(path("/html"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
            .mount(&server)
            .await;

        let engine = engine(vec![]).await;
        let dir = tempfile::tempdir().unwrap();
        let mut settings = engine.settings();
        settings.downloads.torrent_dir = Some(dir.path().to_path_buf());
        engine.set_settings(settings).await.unwrap();

        let url: Url = format!("{}/ok.torrent", server.uri()).parse().unwrap();
        let saved = engine.save_torrent(&url, "My: Torrent").await.unwrap();
        assert_eq!(saved, dir.path().join("My_ Torrent.torrent"));
        assert_eq!(std::fs::read(&saved).unwrap(), b"d4:infod4:name1:xee");

        let html: Url = format!("{}/html", server.uri()).parse().unwrap();
        assert!(matches!(
            engine.save_torrent(&html, "x").await,
            Err(Error::Invalid(_))
        ));
        let file: Url = "file:///etc/passwd".parse().unwrap();
        assert!(matches!(
            engine.save_torrent(&file, "x").await,
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn result_cache_evicts_oldest() {
        let mut cache = ResultCache::default();
        for i in 0..(RESULT_CACHE_CAPACITY + 5) {
            cache.insert(MergedResult {
                id: format!("r{i}"),
                primary: SearchResult::new("p".into(), "t"),
                sources: vec![],
                seeders: None,
                leechers: None,
                score: 0.0,
            });
        }
        assert!(cache.get("r0").is_none());
        assert!(
            cache
                .get(&format!("r{}", RESULT_CACHE_CAPACITY + 4))
                .is_some()
        );
        assert_eq!(cache.items.len(), RESULT_CACHE_CAPACITY);
    }

    // ----- definitions ------------------------------------------------------

    async fn engine_with(definitions: Vec<String>, secrets: Arc<MemorySecretStore>) -> Engine {
        Engine::with_store(
            AppPaths::at(std::env::temp_dir().join("hashlark-engine-tests")),
            Store::open_in_memory().await.unwrap(),
            EngineOptions {
                secrets,
                builtins: vec![],
                definitions,
            },
        )
        .await
        .unwrap()
    }

    const LOCAL_DEF: &str = r#"
schema: 1
id: local-site
name: Local Site
links: [http://127.0.0.1:9/]
settings:
  - { name: username }
  - { name: password, type: password, required: true }
  - { name: sort, type: select, options: { new: Newest, top: Top } }
search:
  path: /s
  response: html
  rows: tr
  fields:
    title: { selector: a }
    magnet: { selector: a, attr: href }
"#;

    #[tokio::test]
    async fn first_party_definitions_are_installed_and_protected() {
        let engine = engine_with(
            providers::first_party_definitions(),
            Arc::new(MemorySecretStore::default()),
        )
        .await;
        let views = engine.providers().await.unwrap();
        let ids: Vec<&str> = views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["academic-torrents", "foss-torrents", "linuxtracker"]);
        assert!(
            views
                .iter()
                .all(|v| v.builtin && v.enabled && v.error.is_none())
        );
        assert!(matches!(
            &views[0].source,
            ProviderSource::Definition { repo_id: Some(r), .. } if r == providers::FIRST_PARTY_REPO
        ));
        assert!(matches!(
            engine.remove_provider("linuxtracker").await,
            Err(Error::Invalid(_))
        ));
        // Importing a definition with a first-party id is refused.
        let clash = LOCAL_DEF.replace("id: local-site", "id: linuxtracker");
        assert!(matches!(
            engine.add_definition(&clash).await,
            Err(Error::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn imported_definitions_with_settings() {
        let secrets = Arc::new(MemorySecretStore::default());
        let engine = engine_with(Vec::new(), secrets.clone()).await;

        assert!(matches!(
            engine.add_definition("schema: 1").await,
            Err(Error::Definition(_))
        ));
        let view = engine.add_definition(LOCAL_DEF).await.unwrap();
        assert_eq!(view.kind, ProviderKind::Definition);
        assert!(!view.builtin);
        assert_eq!(view.settings.len(), 3);

        let mut changes = std::collections::BTreeMap::new();
        changes.insert("username".to_owned(), Some("ada".to_owned()));
        changes.insert("password".to_owned(), Some("s3cret".to_owned()));
        changes.insert("sort".to_owned(), Some("top".to_owned()));
        let view = engine
            .update_provider(
                "local-site",
                ProviderPatch {
                    settings: Some(changes),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let get = |name: &str| {
            view.settings
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .clone()
        };
        assert_eq!(get("username").value.as_deref(), Some("ada"));
        assert_eq!(get("password").value, None, "passwords are never returned");
        assert!(get("password").is_set);
        use crate::secrets::SecretStore;
        assert_eq!(
            secrets
                .get("provider/local-site/password")
                .unwrap()
                .as_deref(),
            Some("s3cret")
        );
        let record = registry::get(engine.store(), "local-site").await.unwrap();
        assert!(
            !record.config.to_string().contains("s3cret"),
            "no secrets in SQLite"
        );

        let bad_option =
            std::collections::BTreeMap::from([("sort".to_owned(), Some("nope".to_owned()))]);
        assert!(matches!(
            engine
                .update_provider(
                    "local-site",
                    ProviderPatch {
                        settings: Some(bad_option),
                        ..Default::default()
                    }
                )
                .await,
            Err(Error::Invalid(_))
        ));

        // Re-importing the same id updates it.
        let renamed = LOCAL_DEF.replace("name: Local Site", "name: Local Site 2");
        assert_eq!(
            engine.add_definition(&renamed).await.unwrap().name,
            "Local Site 2"
        );

        engine.remove_provider("local-site").await.unwrap();
        assert!(engine.definition("local-site").await.is_err());
        assert_eq!(secrets.get("provider/local-site/password").unwrap(), None);
    }

    #[test]
    fn check_definition_reports_errors() {
        let ok = Engine::check_definition(LOCAL_DEF);
        assert!(ok.ok);
        assert_eq!(ok.id.as_deref(), Some("local-site"));
        let bad = Engine::check_definition(&LOCAL_DEF.replace("rows: tr", "rows: \"[[\""));
        assert!(!bad.ok);
        assert_eq!(bad.id.as_deref(), Some("local-site"));
        assert!(bad.errors[0].contains("CSS selector"));
    }

    // ----- torznab and sessions ---------------------------------------------

    #[tokio::test]
    async fn torznab_providers_search_with_their_api_key() {
        use wiremock::matchers::{path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(path("/api"))
            .and(query_param("apikey", "k1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../fixtures/torznab/search.xml"
                ))),
            )
            .mount(&server)
            .await;

        let secrets = Arc::new(MemorySecretStore::default());
        let engine = engine_with(Vec::new(), secrets.clone()).await;
        assert!(matches!(
            engine.add_torznab("J", "ftp://x", None).await,
            Err(Error::Invalid(_))
        ));
        let view = engine
            .add_torznab(
                "My Jackett",
                &format!("{}/api", server.uri()),
                Some("k1".into()),
            )
            .await
            .unwrap();
        assert_eq!(view.id, "torznab-my-jackett");
        assert!(matches!(&view.source, ProviderSource::Torznab { url } if url.ends_with("/api")));
        assert!(
            view.settings
                .iter()
                .any(|s| s.name == "api_key" && s.is_set && s.value.is_none())
        );
        let second = engine
            .add_torznab("My Jackett", "http://127.0.0.1:1/api", None)
            .await
            .unwrap();
        assert_eq!(second.id, "torznab-my-jackett-2");

        let outcome = engine.search_all(SearchQuery::text("bunny")).await.unwrap();
        let from_first: Vec<_> = outcome
            .providers
            .iter()
            .filter(|p| p.provider.as_str() == "torznab-my-jackett")
            .collect();
        assert!(matches!(
            from_first[0].status,
            crate::aggregator::ProviderStatus::Ok { count: 2 }
        ));

        engine.remove_provider("torznab-my-jackett").await.unwrap();
        use crate::secrets::SecretStore;
        assert_eq!(
            secrets.get("provider/torznab-my-jackett/api_key").unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn sessions_send_cookies_and_user_agent() {
        use wiremock::matchers::{header, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(path("/s"))
            .and(header("cookie", "cf_clearance=ok"))
            .and(header("user-agent", "TestBrowser/1.0"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"<table><tr><td><a href="magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c">Found</a></td></tr></table>"#,
            ))
            .mount(&server)
            .await;
        Mock::given(path("/s"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("server", "cloudflare")
                    .set_body_string("Just a moment..."),
            )
            .mount(&server)
            .await;

        let engine = engine_with(Vec::new(), Arc::new(MemorySecretStore::default())).await;
        let yaml = LOCAL_DEF
            .replace("http://127.0.0.1:9/", &format!("{}/", server.uri()))
            .replace("required: true", "required: false");
        engine.add_definition(&yaml).await.unwrap();

        let report = engine.test_provider("local-site").await.unwrap();
        assert_eq!(report.error_kind, Some(ErrorKind::ChallengeRequired));

        let session = Session {
            url: server.uri().parse().unwrap(),
            cookies: vec![SessionCookie {
                name: "cf_clearance".into(),
                value: "ok".into(),
            }],
            user_agent: Some("TestBrowser/1.0".into()),
            expires_at: None,
        };
        engine.set_session("local-site", session).await.unwrap();
        let report = engine.test_provider("local-site").await.unwrap();
        assert!(report.ok, "{report:?}");
        assert_eq!(report.result_count, 1);
    }
}
