// SPDX-License-Identifier: GPL-3.0-or-later

//! UniFFI bindings of the Hashlark core for the Android app (ADR 0003).
//!
//! The boundary is deliberately narrow: one [`HashlarkEngine`] object whose
//! methods take and return the same JSON documents as the HTTP API
//! (`docs/adr/0004-http-api-contract.md`), plus two callback interfaces for
//! the things Kotlin has to provide or receive: [`SecretStore`] (Android
//! Keystore) and [`SearchListener`] (streamed search events). Keeping the
//! payloads as JSON means the Kotlin models mirror the OpenAPI schema and
//! every API change lands in one place, the core.

use std::future::Future;
use std::sync::{Arc, OnceLock};

use hashlark_core::engine::{ProviderPatch, Session};
use hashlark_core::model::TargetKind;
use hashlark_core::{
    AppPaths, CancellationToken, Engine, EngineOptions, Error, SearchQuery, Settings,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

uniffi::setup_scaffolding!();

/// Version of the core, for the About screen and the update check.
#[uniffi::export]
pub fn core_version() -> String {
    hashlark_core::VERSION.to_owned()
}

/// User agent the core sends. The browser-check `WebView` uses the same one,
/// because challenge cookies only work with the agent that earned them.
#[uniffi::export]
pub fn user_agent() -> String {
    hashlark_core::net::USER_AGENT.to_owned()
}

/// Makes `title` safe as a file name.
#[uniffi::export]
pub fn sanitize_file_name(title: String) -> String {
    hashlark_core::engine::sanitize_file_name(&title)
}

/// Checks a provider definition without saving it. Returns a `DefinitionCheck`.
#[uniffi::export]
pub fn check_definition(yaml: String) -> Result<String, HashlarkError> {
    to_json(&Engine::check_definition(&yaml))
}

// ----- errors ------------------------------------------------------------------

/// Every failure that crosses the boundary. `code` is the same stable code
/// the HTTP API uses (`not_found`, `invalid_request`, `invalid_definition`,
/// `provider_error`, `internal`, ...).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HashlarkError {
    #[error("{description}")]
    Failed {
        code: String,
        /// Human-readable text (not called `message`: that clashes with `Throwable.message` in Kotlin).
        description: String,
        /// For `invalid_definition`: every problem found.
        details: Vec<String>,
        /// For `provider_error`: what went wrong with the provider.
        provider_error: Option<String>,
    },
}

impl HashlarkError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self::Failed {
            code: code.to_owned(),
            description: message.into(),
            details: Vec::new(),
            provider_error: None,
        }
    }

    fn internal(message: impl std::fmt::Display) -> Self {
        tracing::error!(error = %message, "internal error");
        Self::new("internal", message.to_string())
    }
}

impl From<Error> for HashlarkError {
    fn from(err: Error) -> Self {
        match err {
            Error::NotFound(_) | Error::UnknownProvider(_) => {
                Self::new("not_found", err.to_string())
            }
            Error::Invalid(message) => Self::new("invalid_request", message),
            Error::Definition(def) => Self::Failed {
                code: "invalid_definition".into(),
                description: format!("the definition has {} problem(s)", def.errors.len()),
                details: def.errors,
                provider_error: None,
            },
            Error::Provider { ref source, .. } => Self::Failed {
                code: "provider_error".into(),
                description: err.to_string(),
                details: Vec::new(),
                provider_error: Some(source.kind().as_str().to_owned()),
            },
            other => Self::internal(other),
        }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HashlarkError {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::internal(err)
    }
}

impl From<serde_json::Error> for HashlarkError {
    fn from(err: serde_json::Error) -> Self {
        Self::new("invalid_request", format!("invalid JSON: {err}"))
    }
}

// ----- runtime -----------------------------------------------------------------

/// One multi-thread runtime for the whole process. Exported futures are
/// polled by UniFFI on whichever thread; the real work runs here.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("hashlark-rt")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Runs a core future on the shared runtime.
async fn run<T, F>(future: F) -> Result<T, HashlarkError>
where
    T: Send + 'static,
    F: Future<Output = hashlark_core::Result<T>> + Send + 'static,
{
    runtime()
        .spawn(future)
        .await
        .map_err(|e| HashlarkError::internal(format!("task failed: {e}")))?
        .map_err(Into::into)
}

/// Like [`run`], serializing the result.
async fn run_json<T, F>(future: F) -> Result<String, HashlarkError>
where
    T: Serialize + Send + 'static,
    F: Future<Output = hashlark_core::Result<T>> + Send + 'static,
{
    to_json(&run(future).await?)
}

fn to_json<T: Serialize>(value: &T) -> Result<String, HashlarkError> {
    serde_json::to_string(value).map_err(HashlarkError::internal)
}

fn from_json<T: DeserializeOwned>(json: &str) -> Result<T, HashlarkError> {
    Ok(serde_json::from_str(json)?)
}

// ----- callbacks -----------------------------------------------------------------

/// Credential storage provided by the app (Android Keystore + DataStore).
/// Calls may block briefly.
#[uniffi::export(with_foreign)]
pub trait SecretStore: Send + Sync {
    fn get(&self, key: String) -> Result<Option<String>, HashlarkError>;
    fn set(&self, key: String, value: String) -> Result<(), HashlarkError>;
    fn delete(&self, key: String) -> Result<(), HashlarkError>;
}

struct ForeignSecrets(Arc<dyn SecretStore>);

impl std::fmt::Debug for ForeignSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ForeignSecrets")
    }
}

fn secrets_error(e: HashlarkError) -> Error {
    Error::Secrets(e.to_string())
}

impl hashlark_core::secrets::SecretStore for ForeignSecrets {
    fn get(&self, key: &str) -> hashlark_core::Result<Option<String>> {
        self.0.get(key.to_owned()).map_err(secrets_error)
    }

    fn set(&self, key: &str, value: &str) -> hashlark_core::Result<()> {
        self.0
            .set(key.to_owned(), value.to_owned())
            .map_err(secrets_error)
    }

    fn delete(&self, key: &str) -> hashlark_core::Result<()> {
        self.0.delete(key.to_owned()).map_err(secrets_error)
    }
}

/// Receives the events of a running search, as the JSON of a `SearchEvent`
/// (`provider_started`, `results`, `provider_finished`, `provider_failed`,
/// `done`). Called from a background thread; implementations must return
/// quickly.
#[uniffi::export(with_foreign)]
pub trait SearchListener: Send + Sync {
    fn on_event(&self, event_json: String);

    /// The stream is over: after `done`, and also after a cancelled or abandoned
    /// search, which sends no `done`. No more events follow.
    fn on_end(&self);
}

/// A running search. Dropping it does **not** cancel; call [`cancel`](Self::cancel).
#[derive(Debug, uniffi::Object)]
pub struct SearchHandle {
    cancel: CancellationToken,
}

#[uniffi::export]
impl SearchHandle {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

// ----- engine ----------------------------------------------------------------------

/// Opens (creating if needed) the engine with its data under `data_dir`,
/// which on Android is `Context.filesDir`.
#[uniffi::export]
pub async fn open_engine(
    data_dir: String,
    secrets: Arc<dyn SecretStore>,
) -> Result<Arc<HashlarkEngine>, HashlarkError> {
    let engine = run(async move {
        let options = EngineOptions::new(Arc::new(ForeignSecrets(secrets)));
        Engine::open(AppPaths::at(data_dir), options).await
    })
    .await?;
    Ok(Arc::new(HashlarkEngine { engine }))
}

#[derive(Debug, uniffi::Object)]
pub struct HashlarkEngine {
    engine: Engine,
}

#[uniffi::export]
impl HashlarkEngine {
    // ----- search ------------------------------------------------------------

    /// Starts a search. `query_json` is a `SearchQuery`.
    pub async fn search(
        self: Arc<Self>,
        query_json: String,
        listener: Arc<dyn SearchListener>,
    ) -> Result<Arc<SearchHandle>, HashlarkError> {
        let query: SearchQuery = from_json(&query_json)?;
        let cancel = CancellationToken::new();
        let engine = self.engine.clone();
        let token = cancel.clone();
        let mut rx = run(async move { engine.search(query, token).await }).await?;
        // A plain thread: the callback is synchronous Kotlin code and must
        // not stall a runtime worker.
        std::thread::Builder::new()
            .name("hashlark-search".into())
            .spawn(move || {
                while let Some(event) = rx.blocking_recv() {
                    match serde_json::to_string(&event) {
                        Ok(json) => listener.on_event(json),
                        Err(e) => tracing::warn!(error = %e, "could not serialize a search event"),
                    }
                }
                listener.on_end();
            })
            .map_err(HashlarkError::internal)?;
        Ok(Arc::new(SearchHandle { cancel }))
    }

    /// A result from a recent search, or a favourite. Returns a `MergedResult`.
    pub async fn result(self: Arc<Self>, id: String) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.result(&id).await }).await
    }

    /// The magnet or `.torrent` for a result. `prefer` is `magnet`,
    /// `torrent_file` or none. Returns a `DownloadTarget`.
    pub async fn resolve(
        self: Arc<Self>,
        result_id: String,
        prefer: Option<String>,
    ) -> Result<String, HashlarkError> {
        let prefer: Option<TargetKind> = match prefer {
            Some(kind) => Some(from_json(&to_json(&kind)?)?),
            None => None,
        };
        let engine = self.engine.clone();
        run_json(async move { engine.resolve(&result_id, prefer).await }).await
    }

    /// Downloads a `.torrent` file and returns its bytes.
    pub async fn fetch_torrent(self: Arc<Self>, url: String) -> Result<Vec<u8>, HashlarkError> {
        let url =
            Url::parse(&url).map_err(|e| HashlarkError::new("invalid_request", e.to_string()))?;
        let engine = self.engine.clone();
        run(async move { engine.fetch_torrent(&url).await }).await
    }

    // ----- providers -------------------------------------------------------------

    /// Every provider, as a JSON array of `ProviderView`.
    pub async fn providers(self: Arc<Self>) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.providers().await }).await
    }

    /// Applies a `ProviderPatch`; returns the `ProviderView`.
    pub async fn update_provider(
        self: Arc<Self>,
        id: String,
        patch_json: String,
    ) -> Result<String, HashlarkError> {
        let patch: ProviderPatch = from_json(&patch_json)?;
        let engine = self.engine.clone();
        run_json(async move { engine.update_provider(&id, patch).await }).await
    }

    pub async fn remove_provider(self: Arc<Self>, id: String) -> Result<(), HashlarkError> {
        let engine = self.engine.clone();
        run(async move { engine.remove_provider(&id).await }).await
    }

    /// Runs a provider's self-test; returns a `TestReport`.
    pub async fn test_provider(self: Arc<Self>, id: String) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.test_provider(&id).await }).await
    }

    pub async fn add_torznab(
        self: Arc<Self>,
        name: String,
        url: String,
        api_key: Option<String>,
    ) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.add_torznab(&name, &url, api_key).await }).await
    }

    /// Adds or updates a provider from a YAML definition.
    pub async fn add_definition(self: Arc<Self>, yaml: String) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.add_definition(&yaml).await }).await
    }

    /// Stores the cookies of a completed browser check (`Session`).
    pub async fn set_session(
        self: Arc<Self>,
        provider_id: String,
        session_json: String,
    ) -> Result<String, HashlarkError> {
        let session: Session = from_json(&session_json)?;
        let engine = self.engine.clone();
        run_json(async move { engine.set_session(&provider_id, session).await }).await
    }

    // ----- settings ----------------------------------------------------------------

    pub fn settings(&self) -> Result<String, HashlarkError> {
        to_json(&self.engine.settings())
    }

    /// Validates, saves and applies `Settings`; returns the saved settings.
    pub async fn set_settings(
        self: Arc<Self>,
        settings_json: String,
    ) -> Result<String, HashlarkError> {
        let settings: Settings = from_json(&settings_json)?;
        let engine = self.engine.clone();
        run_json(async move { engine.set_settings(settings).await }).await
    }

    /// State of built-in Tor (`TorStatus`).
    pub async fn tor_status(self: Arc<Self>) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { Ok(engine.tor_status().await) }).await
    }

    // ----- history and favourites -------------------------------------------------------

    pub async fn history(self: Arc<Self>, limit: u32) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.history(limit).await }).await
    }

    pub async fn clear_history(self: Arc<Self>) -> Result<(), HashlarkError> {
        let engine = self.engine.clone();
        run(async move { engine.clear_history().await }).await
    }

    pub async fn favorites(self: Arc<Self>) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.favorites().await }).await
    }

    pub async fn add_favorite(self: Arc<Self>, result_id: String) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.add_favorite(&result_id).await }).await
    }

    pub async fn remove_favorite(self: Arc<Self>, result_id: String) -> Result<(), HashlarkError> {
        let engine = self.engine.clone();
        run(async move { engine.remove_favorite(&result_id).await }).await
    }

    // ----- repositories and trackers -------------------------------------------------------

    pub async fn repos(self: Arc<Self>) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.repos().await }).await
    }

    /// Fetches a repository's signed index without adding it, so the user
    /// can check the signing key first (trust on first use). Returns a `RepoPreview`.
    pub async fn preview_repo(self: Arc<Self>, url: String) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.preview_repo(&url).await }).await
    }

    /// Adds a repository; returns `{ "repo": RepoView, "sync": SyncReport }`.
    pub async fn add_repo(self: Arc<Self>, url: String) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move {
            let (repo, sync) = engine.add_repo(&url).await?;
            Ok(serde_json::json!({ "repo": repo, "sync": sync }))
        })
        .await
    }

    pub async fn sync_repo(self: Arc<Self>, id: String) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.sync_repo(&id).await }).await
    }

    /// Syncs every repository and refreshes the tracker list. This is what
    /// the app's WorkManager job calls instead of the core's own timer.
    /// Returns the number of repositories that failed to sync.
    pub async fn sync_all(self: Arc<Self>) -> Result<u32, HashlarkError> {
        let engine = self.engine.clone();
        run(async move {
            let failed = engine
                .sync_all_repos()
                .await
                .iter()
                .filter(|(_, outcome)| outcome.is_err())
                .count();
            if engine.settings().magnets.trackers_url.is_some() {
                engine.refresh_trackers().await?;
            }
            Ok(u32::try_from(failed).unwrap_or(u32::MAX))
        })
        .await
    }

    pub async fn remove_repo(self: Arc<Self>, id: String) -> Result<(), HashlarkError> {
        let engine = self.engine.clone();
        run(async move { engine.remove_repo(&id).await }).await
    }

    pub async fn trackers(self: Arc<Self>) -> Result<String, HashlarkError> {
        let engine = self.engine.clone();
        run_json(async move { engine.trackers().await }).await
    }

    pub async fn refresh_trackers(self: Arc<Self>) -> Result<u32, HashlarkError> {
        let engine = self.engine.clone();
        run(async move {
            let fetched = engine.refresh_trackers().await?;
            Ok(u32::try_from(fetched).unwrap_or(u32::MAX))
        })
        .await
    }
}

#[cfg(test)]
mod tests;
