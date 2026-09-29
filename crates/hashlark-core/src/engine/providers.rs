// SPDX-License-Identifier: GPL-3.0-or-later

//! Managing providers: loading them, their settings, importing definitions,
//! and hot reload.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{Engine, Runtime};
use crate::aggregator::BoundProvider;
use crate::definition;
use crate::definition::spec::{SettingDef, SettingKind, SiteType};
use crate::definition::store as defstore;
use crate::error::{Error, ErrorKind, ProviderError, Result};
use crate::health::{self, HealthSummary};
use crate::instrument::Instrumented;
use crate::model::{Category, SearchQuery, SearchResult};
use crate::net::{self, ClientOptions};
use crate::provider::torznab::API_KEY;
use crate::provider::{
    Capabilities, DefinitionProvider, MirrorMemory, ProviderCtx, ProviderKind, SearchProvider,
    TorznabProvider,
};
use crate::registry::{self, NetworkPolicy, ProviderRecord, Route};
use crate::secrets::provider_secret_key;
use crate::settings::Settings;
use crate::store::{Store, now_ms};

/// Repository id of the definitions shipped with Hashlark.
pub const FIRST_PARTY_REPO: &str = "builtin";

/// The first-party definitions compiled into this build.
pub fn first_party_definitions() -> Vec<String> {
    [
        include_str!("../../../../definitions/builtin/linuxtracker.yml"),
        include_str!("../../../../definitions/builtin/academic-torrents.yml"),
        include_str!("../../../../definitions/builtin/foss-torrents.yml"),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// Where a provider comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderSource {
    /// Compiled into Hashlark.
    Native,
    Definition {
        version: i64,
        /// Repository it came from; `null` if imported by hand.
        repo_id: Option<String>,
        site_type: Option<SiteType>,
        links: Vec<String>,
    },
    Torznab {
        url: String,
    },
}

/// A user-configurable setting of a provider.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SettingView {
    pub name: String,
    pub label: String,
    pub kind: SettingKind,
    pub required: bool,
    pub options: BTreeMap<String, String>,
    /// Current value; always `null` for passwords.
    pub value: Option<String>,
    /// Whether a value is stored (useful for passwords).
    pub is_set: bool,
}

/// A provider as shown to users.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub description: String,
    pub enabled: bool,
    /// Shipped with Hashlark: can be disabled but not removed.
    pub builtin: bool,
    pub categories: Vec<Category>,
    pub capabilities: Option<Capabilities>,
    pub health: HealthSummary,
    /// Why the provider couldn't be loaded (e.g. an invalid definition).
    pub error: Option<String>,
    pub network_policy: NetworkPolicy,
    pub source: ProviderSource,
    pub settings: Vec<SettingView>,
}

/// Changes to a provider. Absent fields are left as they are.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProviderPatch {
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub network_policy: Option<NetworkPolicy>,
    /// Setting values to change; `null` clears one.
    pub settings: Option<BTreeMap<String, Option<String>>>,
}

/// Outcome of a manual provider test.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TestReport {
    pub ok: bool,
    pub result_count: usize,
    pub latency_ms: u64,
    pub error_kind: Option<ErrorKind>,
    pub message: Option<String>,
}

/// Result of checking a definition without saving it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DefinitionCheck {
    pub ok: bool,
    pub errors: Vec<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<u32>,
}

/// How long a browser-check session is trusted when the site doesn't say.
const SESSION_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// One cookie from a browser session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionCookie {
    pub name: String,
    pub value: String,
}

/// Cookies from a browser check the user completed on the site.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Session {
    /// Page the cookies belong to.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub url: url::Url,
    pub cookies: Vec<SessionCookie>,
    /// The browser's user agent; challenge cookies only work with it.
    pub user_agent: Option<String>,
    /// Unix ms; defaults to 24 hours from now.
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSession {
    url: url::Url,
    cookies: Vec<SessionCookie>,
}

/// Remembers the last working mirror in memory and in `provider_mirrors`.
#[derive(Debug)]
struct DbMirrorMemory {
    store: Store,
    provider_id: String,
    preferred: Mutex<Option<String>>,
}

impl MirrorMemory for DbMirrorMemory {
    fn preferred(&self) -> Option<String> {
        self.preferred.lock().expect("lock").clone()
    }

    fn remember(&self, url: &str) {
        {
            let mut preferred = self.preferred.lock().expect("lock");
            if preferred.as_deref() == Some(url) {
                return;
            }
            *preferred = Some(url.to_owned());
        }
        let (store, id, url) = (self.store.clone(), self.provider_id.clone(), url.to_owned());
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let result = sqlx::query(
                    "INSERT INTO provider_mirrors (provider_id, url, position, last_ok_at)
                     VALUES (?, ?, 0, ?)
                     ON CONFLICT (provider_id, url) DO UPDATE SET last_ok_at = excluded.last_ok_at",
                )
                .bind(&id)
                .bind(&url)
                .bind(now_ms())
                .execute(store.pool())
                .await;
                if let Err(e) = result {
                    tracing::debug!(provider = %id, error = %e, "could not save mirror");
                }
            });
        }
    }
}

/// Installs or updates the first-party definitions and their providers.
pub(super) async fn install_first_party(store: &Store, yamls: &[String]) -> Result<()> {
    for yaml in yamls {
        let def = match definition::load(yaml) {
            Ok(compiled) => compiled.spec,
            Err(e) => {
                tracing::error!(error = %e, "a first-party definition is invalid");
                continue;
            }
        };
        let current = defstore::get(store, &def.id).await.ok();
        match &current {
            // Imported by the user with the same id: leave theirs alone.
            Some(stored) if stored.repo_id.as_deref() != Some(FIRST_PARTY_REPO) => continue,
            Some(stored) if stored.version >= i64::from(def.version) => {}
            _ => {
                defstore::upsert(
                    store,
                    &def.id,
                    Some(FIRST_PARTY_REPO),
                    def.version.into(),
                    yaml,
                )
                .await?;
            }
        }
        sqlx::query(
            "INSERT INTO providers (id, kind, name, enabled, definition_id, config_json,
                                    network_policy_json, created_at)
             VALUES (?, 'definition', ?, 1, ?, '{}', '{}', ?)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&def.id)
        .bind(&def.name)
        .bind(&def.id)
        .bind(now_ms())
        .execute(store.pool())
        .await?;
    }
    Ok(())
}

impl Engine {
    /// Re-instantiates every provider from its record and current settings.
    pub(super) async fn rebuild(&self) -> Result<()> {
        let settings = self.settings();
        let records = registry::list(&self.inner.store).await?;
        let mut entries = HashMap::new();
        for record in &records {
            let bound = self.instantiate(record, &settings).await;
            if let Err(e) = &bound {
                tracing::warn!(provider = %record.id, error = %e, "provider could not be loaded");
            }
            entries.insert(record.id.clone(), bound);
        }
        *self.inner.runtime.write().expect("lock") = Arc::new(Runtime { entries });
        Ok(())
    }

    async fn load_definition(
        &self,
        id: &str,
    ) -> std::result::Result<Arc<DefinitionProvider>, String> {
        let stored = defstore::get(&self.inner.store, id)
            .await
            .map_err(|e| e.to_string())?;
        if let Some((sha, provider)) = self.inner.definitions.lock().expect("lock").get(id)
            && *sha == stored.sha256
        {
            return Ok(Arc::clone(provider));
        }
        let provider =
            Arc::new(DefinitionProvider::from_yaml(&stored.yaml).map_err(|e| e.to_string())?);
        self.inner
            .definitions
            .lock()
            .expect("lock")
            .insert(id.to_owned(), (stored.sha256, Arc::clone(&provider)));
        Ok(provider)
    }

    fn setting_defs(&self, provider: &dyn SearchProvider) -> Vec<SettingDef> {
        provider.settings()
    }

    /// The provider's settings with secrets filled in from the secret store.
    async fn provider_config(
        &self,
        record: &ProviderRecord,
        defs: &[SettingDef],
    ) -> Result<BTreeMap<String, String>> {
        let stored = record.config.get("settings").cloned().unwrap_or_default();
        let mut config = BTreeMap::new();
        for def in defs {
            let value = if def.kind == SettingKind::Password {
                self.secret(&provider_secret_key(&record.id, &def.name))
                    .await?
            } else {
                stored
                    .get(&def.name)
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            };
            if let Some(v) = value.or_else(|| def.default.clone()) {
                config.insert(def.name.clone(), v);
            }
        }
        Ok(config)
    }

    async fn secret(&self, key: &str) -> Result<Option<String>> {
        let secrets = Arc::clone(&self.inner.secrets);
        let key = key.to_owned();
        tokio::task::spawn_blocking(move || secrets.get(&key))
            .await
            .map_err(|e| Error::Secrets(e.to_string()))?
    }

    async fn set_secret(&self, key: &str, value: Option<String>) -> Result<()> {
        let secrets = Arc::clone(&self.inner.secrets);
        let key = key.to_owned();
        tokio::task::spawn_blocking(move || match value {
            Some(v) => secrets.set(&key, &v),
            None => secrets.delete(&key),
        })
        .await
        .map_err(|e| Error::Secrets(e.to_string()))?
    }

    fn cookie_jar(&self, provider_id: &str) -> Arc<reqwest::cookie::Jar> {
        Arc::clone(
            self.inner
                .jars
                .lock()
                .expect("lock")
                .entry(provider_id.to_owned())
                .or_default(),
        )
    }

    async fn instantiate(
        &self,
        record: &ProviderRecord,
        settings: &Settings,
    ) -> std::result::Result<BoundProvider, String> {
        let provider: Arc<dyn SearchProvider> = match record.kind {
            ProviderKind::Native => self
                .inner
                .builtins
                .iter()
                .find(|p| p.info().id.as_str() == record.id)
                .cloned()
                .ok_or_else(|| "this built-in provider is not part of this version".to_owned())?,
            ProviderKind::Definition => {
                let def_id = record.definition_id.as_deref().unwrap_or(&record.id);
                self.load_definition(def_id).await?
            }
            ProviderKind::Torznab => {
                let url = record
                    .config
                    .get("url")
                    .and_then(|v| v.as_str())
                    .and_then(|u| url::Url::parse(u).ok())
                    .ok_or_else(|| "the Torznab URL is missing or invalid".to_owned())?;
                Arc::new(TorznabProvider::new(&record.id, &record.name, url))
            }
        };
        let defs = self.setting_defs(provider.as_ref());
        let config = self
            .provider_config(record, &defs)
            .await
            .map_err(|e| e.to_string())?;
        let preferred: Option<String> = sqlx::query_scalar(
            "SELECT url FROM provider_mirrors WHERE provider_id = ? AND last_ok_at IS NOT NULL
             ORDER BY last_ok_at DESC LIMIT 1",
        )
        .bind(&record.id)
        .fetch_optional(self.inner.store.pool())
        .await
        .map_err(|e| e.to_string())?;

        let (mut options, onion) = self
            .network()
            .client_options(&record.network_policy)
            .map_err(|e| e.to_string())?;
        let jar = self.cookie_jar(&record.id);
        if let Some(session) = self.session(&record.id).await.map_err(|e| e.to_string())? {
            for cookie in &session.cookies {
                jar.add_cookie_str(
                    &format!("{}={}; Path=/", cookie.name, cookie.value),
                    &session.url,
                );
            }
            options.user_agent = session.user_agent;
        }
        options.cookies = Some(jar);
        let http = net::build_client(&options).map_err(|e| e.to_string())?;
        let http_no_redirect = net::build_client(&ClientOptions {
            follow_redirects: false,
            ..options
        })
        .map_err(|e| e.to_string())?;
        let trackers = self.trackers().await.map_err(|e| e.to_string())?;
        let ctx = ProviderCtx {
            trackers: trackers.into(),
            http_no_redirect,
            onion,
            config: Arc::new(config),
            mirrors: Some(Arc::new(DbMirrorMemory {
                store: self.inner.store.clone(),
                provider_id: record.id.clone(),
                preferred: Mutex::new(preferred),
            })),
            ..ProviderCtx::with_client(http)
        };
        let instrumented = Instrumented::new(
            provider,
            self.inner.store.clone(),
            settings.search.cache_ttl(),
            settings.search.provider_timeout(),
        );
        Ok(BoundProvider::new(Arc::new(instrumented), ctx))
    }

    pub(super) fn runtime(&self) -> Arc<Runtime> {
        Arc::clone(&self.inner.runtime.read().expect("lock"))
    }

    pub(super) fn bound(&self, id: &str) -> Result<BoundProvider> {
        match self.runtime().entries.get(id) {
            Some(Ok(bound)) => Ok(bound.clone()),
            Some(Err(e)) => Err(Error::Invalid(format!(
                "provider `{id}` is not usable: {e}"
            ))),
            None => Err(Error::NotFound(format!("provider `{id}`"))),
        }
    }

    async fn source(&self, record: &ProviderRecord) -> ProviderSource {
        match record.kind {
            ProviderKind::Native => ProviderSource::Native,
            ProviderKind::Torznab => ProviderSource::Torznab {
                url: record
                    .config
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned(),
            },
            ProviderKind::Definition => {
                let id = record.definition_id.as_deref().unwrap_or(&record.id);
                let stored = defstore::get(&self.inner.store, id).await.ok();
                let loaded = self
                    .inner
                    .definitions
                    .lock()
                    .expect("lock")
                    .get(id)
                    .map(|(_, p)| Arc::clone(p));
                ProviderSource::Definition {
                    version: stored.as_ref().map_or(0, |s| s.version),
                    repo_id: stored.and_then(|s| s.repo_id),
                    site_type: loaded.as_ref().map(|p| p.definition().spec.site_type),
                    links: loaded
                        .map(|p| p.definition().spec.links.clone())
                        .unwrap_or_default(),
                }
            }
        }
    }

    async fn is_builtin(&self, record: &ProviderRecord) -> bool {
        match record.kind {
            ProviderKind::Native => self
                .inner
                .builtins
                .iter()
                .any(|p| p.info().id.as_str() == record.id),
            ProviderKind::Definition => defstore::get(
                &self.inner.store,
                record.definition_id.as_deref().unwrap_or(&record.id),
            )
            .await
            .is_ok_and(|d| d.repo_id.as_deref() == Some(FIRST_PARTY_REPO)),
            ProviderKind::Torznab => false,
        }
    }

    async fn view(&self, record: &ProviderRecord, runtime: &Runtime) -> Result<ProviderView> {
        let bound = runtime.entries.get(&record.id);
        let info = bound
            .and_then(|b| b.as_ref().ok())
            .map(|b| b.provider.info());
        let defs = bound
            .and_then(|b| b.as_ref().ok())
            .map(|b| self.setting_defs(b.provider.as_ref()))
            .unwrap_or_default();
        let stored = record.config.get("settings").cloned().unwrap_or_default();
        let mut settings = Vec::new();
        for def in &defs {
            let (value, is_set) = if def.kind == SettingKind::Password {
                let set = self
                    .secret(&provider_secret_key(&record.id, &def.name))
                    .await?
                    .is_some();
                (None, set)
            } else {
                let v = stored
                    .get(&def.name)
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let set = v.is_some();
                (v.or_else(|| def.default.clone()), set)
            };
            settings.push(SettingView {
                name: def.name.clone(),
                label: def.label.clone().unwrap_or_else(|| def.name.clone()),
                kind: def.kind,
                required: def.required,
                options: def.options.clone(),
                value,
                is_set,
            });
        }
        Ok(ProviderView {
            id: record.id.clone(),
            name: record.name.clone(),
            kind: record.kind,
            description: info.map(|i| i.description.clone()).unwrap_or_default(),
            enabled: record.enabled,
            builtin: self.is_builtin(record).await,
            categories: info.map(|i| i.categories.clone()).unwrap_or_default(),
            capabilities: info.map(|i| i.capabilities),
            health: health::summary(&self.inner.store, &record.id).await?,
            error: bound.and_then(|b| b.as_ref().err().cloned()),
            network_policy: record.network_policy.clone(),
            source: self.source(record).await,
            settings,
        })
    }

    pub async fn providers(&self) -> Result<Vec<ProviderView>> {
        let runtime = self.runtime();
        let mut views = Vec::new();
        for record in registry::list(&self.inner.store).await? {
            views.push(self.view(&record, &runtime).await?);
        }
        Ok(views)
    }

    pub async fn provider(&self, id: &str) -> Result<ProviderView> {
        let record = registry::get(&self.inner.store, id).await?;
        self.view(&record, &self.runtime()).await
    }

    pub async fn update_provider(&self, id: &str, patch: ProviderPatch) -> Result<ProviderView> {
        let mut record = registry::get(&self.inner.store, id).await?;
        if let Some(enabled) = patch.enabled {
            record.enabled = enabled;
            if enabled {
                // Turning a provider on by hand also lifts an auto-disable.
                registry::clear_auto_disable(&self.inner.store, id).await?;
            }
        }
        if let Some(name) = patch.name {
            let name = name.trim();
            if name.is_empty() {
                return Err(Error::Invalid("name must not be empty".into()));
            }
            record.name = name.to_owned();
        }
        if let Some(policy) = patch.network_policy {
            if policy.route == Route::Proxy && policy.proxy.is_none() {
                return Err(Error::Invalid(
                    "a proxy URL is required when the route is `proxy`".into(),
                ));
            }
            record.network_policy = policy;
        }
        if let Some(changes) = patch.settings {
            let defs = match self.runtime().entries.get(id) {
                Some(Ok(bound)) => self.setting_defs(bound.provider.as_ref()),
                _ => Vec::new(),
            };
            let mut stored = record
                .config
                .get("settings")
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            for (name, value) in changes {
                let def = defs.iter().find(|d| d.name == name).ok_or_else(|| {
                    Error::Invalid(format!("this provider has no setting `{name}`"))
                })?;
                let value = value.filter(|v| !v.is_empty());
                if let (SettingKind::Select, Some(v)) = (def.kind, &value)
                    && !def.options.contains_key(v)
                {
                    return Err(Error::Invalid(format!(
                        "`{v}` is not an option for `{name}`"
                    )));
                }
                if def.kind == SettingKind::Password {
                    self.set_secret(&provider_secret_key(id, &name), value)
                        .await?;
                } else {
                    match value {
                        Some(v) => stored.insert(name, serde_json::Value::String(v)),
                        None => stored.remove(&name),
                    };
                }
            }
            if !record.config.is_object() {
                record.config = serde_json::json!({});
            }
            record.config["settings"] = serde_json::Value::Object(stored);
        }
        registry::update(&self.inner.store, &record).await?;
        self.refresh_network().await;
        // New settings may change the login: start a fresh session.
        self.inner.jars.lock().expect("lock").remove(id);
        self.inner.definitions.lock().expect("lock").remove(id);
        self.rebuild().await?;
        self.provider(id).await
    }

    pub async fn remove_provider(&self, id: &str) -> Result<()> {
        let record = registry::get(&self.inner.store, id).await?;
        if self.is_builtin(&record).await {
            return Err(Error::Invalid(
                "built-in providers can be disabled but not removed".into(),
            ));
        }
        self.remove_provider_unchecked(id).await
    }

    /// Removes a provider, its secrets and (if imported by hand) its
    /// definition.
    pub(super) async fn remove_provider_unchecked(&self, id: &str) -> Result<()> {
        let record = registry::get(&self.inner.store, id).await?;
        let defs = match self.runtime().entries.get(id) {
            Some(Ok(bound)) => self.setting_defs(bound.provider.as_ref()),
            _ => Vec::new(),
        };
        for def in defs.iter().filter(|d| d.kind == SettingKind::Password) {
            self.set_secret(&provider_secret_key(id, &def.name), None)
                .await?;
        }
        registry::delete(&self.inner.store, id).await?;
        if record.kind == ProviderKind::Definition {
            let def_id = record.definition_id.as_deref().unwrap_or(id);
            if let Ok(stored) = defstore::get(&self.inner.store, def_id).await
                && stored.repo_id.is_none()
            {
                defstore::delete(&self.inner.store, def_id).await?;
            }
        }
        self.inner.jars.lock().expect("lock").remove(id);
        self.inner.definitions.lock().expect("lock").remove(id);
        self.rebuild().await
    }

    /// Runs the provider's self-test now and records the outcome.
    pub async fn test_provider(&self, id: &str) -> Result<TestReport> {
        let bound = self.bound(id)?;
        let started = std::time::Instant::now();
        let outcome = bound.provider.test(&bound.ctx).await;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(match outcome {
            Ok(count) => TestReport {
                ok: true,
                result_count: count,
                latency_ms,
                error_kind: None,
                message: None,
            },
            Err(e) => TestReport {
                ok: false,
                result_count: 0,
                latency_ms,
                error_kind: Some(e.kind()),
                message: Some(e.to_string()),
            },
        })
    }

    // ----- torznab --------------------------------------------------------

    /// Adds a Torznab endpoint (Jackett, Prowlarr, Bitmagnet).
    pub async fn add_torznab(
        &self,
        name: &str,
        url: &str,
        api_key: Option<String>,
    ) -> Result<ProviderView> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("name must not be empty".into()));
        }
        let url = url::Url::parse(url.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| Error::Invalid("the Torznab URL must be an http(s) URL".into()))?;
        let slug = name
            .to_lowercase()
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let base = format!(
            "torznab-{}",
            if slug.is_empty() { "endpoint" } else { &slug }
        );
        let mut id = base.clone();
        let mut n = 2;
        while registry::get(&self.inner.store, &id).await.is_ok() {
            id = format!("{base}-{n}");
            n += 1;
        }
        let mut record = ProviderRecord::new(&id, ProviderKind::Torznab, name);
        record.config = serde_json::json!({ "url": url.as_str() });
        registry::insert(&self.inner.store, &record).await?;
        if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
            self.set_secret(
                &provider_secret_key(&id, API_KEY),
                Some(key.trim().to_owned()),
            )
            .await?;
        }
        self.rebuild().await?;
        self.provider(&id).await
    }

    // ----- browser-check sessions -----------------------------------------

    /// Stores cookies (and the browser's user agent) from a browser check
    /// the user completed, so the provider's requests pass it too.
    pub async fn set_session(&self, provider_id: &str, session: Session) -> Result<ProviderView> {
        registry::get(&self.inner.store, provider_id).await?;
        if session.cookies.is_empty() {
            return Err(Error::Invalid(
                "no cookies were received from the site".into(),
            ));
        }
        let expires_at = session
            .expires_at
            .unwrap_or_else(|| now_ms() + SESSION_TTL_MS);
        sqlx::query(
            "INSERT INTO provider_sessions (provider_id, cookies_json, user_agent, expires_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (provider_id) DO UPDATE SET cookies_json = excluded.cookies_json,
                 user_agent = excluded.user_agent, expires_at = excluded.expires_at",
        )
        .bind(provider_id)
        .bind(serde_json::to_string(&StoredSession {
            url: session.url.clone(),
            cookies: session.cookies,
        })?)
        .bind(&session.user_agent)
        .bind(expires_at)
        .execute(self.inner.store.pool())
        .await?;
        registry::clear_auto_disable(&self.inner.store, provider_id).await?;
        // Results cached while the site was blocking are stale.
        sqlx::query("DELETE FROM result_cache WHERE cache_key LIKE ?")
            .bind(format!("search:{provider_id}:%"))
            .execute(self.inner.store.pool())
            .await?;
        self.rebuild().await?;
        self.provider(provider_id).await
    }

    async fn session(&self, provider_id: &str) -> Result<Option<Session>> {
        use sqlx::Row;
        let row = sqlx::query(
            "SELECT cookies_json, user_agent, expires_at FROM provider_sessions
             WHERE provider_id = ? AND (expires_at IS NULL OR expires_at > ?)",
        )
        .bind(provider_id)
        .bind(now_ms())
        .fetch_optional(self.inner.store.pool())
        .await?;
        let Some(row) = row else { return Ok(None) };
        let stored: StoredSession =
            serde_json::from_str(&row.try_get::<String, _>("cookies_json")?)?;
        Ok(Some(Session {
            url: stored.url,
            cookies: stored.cookies,
            user_agent: row.try_get("user_agent")?,
            expires_at: row.try_get("expires_at")?,
        }))
    }

    // ----- definitions --------------------------------------------------

    /// Checks a definition without saving it.
    pub fn check_definition(yaml: &str) -> DefinitionCheck {
        match definition::load(yaml) {
            Ok(def) => DefinitionCheck {
                ok: true,
                errors: Vec::new(),
                id: Some(def.spec.id.clone()),
                name: Some(def.spec.name.clone()),
                version: Some(def.spec.version),
            },
            Err(e) => {
                let spec = definition::parse_yaml(yaml).ok();
                DefinitionCheck {
                    ok: false,
                    errors: e.errors,
                    id: spec.as_ref().map(|s| s.id.clone()),
                    name: spec.as_ref().map(|s| s.name.clone()),
                    version: spec.map(|s| s.version),
                }
            }
        }
    }

    /// Adds a provider from a definition, or updates one imported earlier
    /// with the same id.
    pub async fn add_definition(&self, yaml: &str) -> Result<ProviderView> {
        self.import_definition(yaml, None).await
    }

    async fn import_definition(&self, yaml: &str, repo_id: Option<&str>) -> Result<ProviderView> {
        let def = definition::load(yaml)?.spec;
        let id = def.id.clone();
        if let Ok(existing) = registry::get(&self.inner.store, &id).await {
            let stored = defstore::get(&self.inner.store, &id).await.ok();
            let same_source = existing.kind == ProviderKind::Definition
                && stored
                    .as_ref()
                    .is_some_and(|s| s.repo_id.as_deref() == repo_id);
            if !same_source {
                return Err(Error::Invalid(format!(
                    "a provider with id `{id}` already exists; change the definition's id"
                )));
            }
        }
        defstore::upsert(&self.inner.store, &id, repo_id, def.version.into(), yaml).await?;
        if registry::get(&self.inner.store, &id).await.is_err() {
            let mut record = ProviderRecord::new(&id, ProviderKind::Definition, &def.name);
            record.definition_id = Some(id.clone());
            registry::insert(&self.inner.store, &record).await?;
        } else {
            sqlx::query("UPDATE providers SET name = ? WHERE id = ?")
                .bind(&def.name)
                .bind(&id)
                .execute(self.inner.store.pool())
                .await?;
            // Cached results may no longer match the new definition.
            sqlx::query("DELETE FROM result_cache WHERE cache_key LIKE ?")
                .bind(format!("search:{id}:%"))
                .execute(self.inner.store.pool())
                .await?;
        }
        self.rebuild().await?;
        self.provider(&id).await
    }

    /// The saved YAML of a definition.
    pub async fn definition(&self, id: &str) -> Result<defstore::StoredDefinition> {
        defstore::get(&self.inner.store, id).await
    }

    /// Runs a definition once without saving it (used by the definition
    /// editor and import preview). Uses no stored settings or cookies.
    pub async fn preview_definition(
        &self,
        yaml: &str,
        query: &SearchQuery,
        config: BTreeMap<String, String>,
    ) -> Result<Vec<SearchResult>> {
        let provider = DefinitionProvider::new(definition::load(yaml)?);
        let http = net::build_client(&ClientOptions {
            cookies: Some(Arc::default()),
            ..ClientOptions::default()
        })?;
        let ctx = ProviderCtx {
            config: Arc::new(config),
            ..ProviderCtx::with_client(http)
        };
        let timeout = self.settings().search.provider_timeout() * 2;
        tokio::time::timeout(timeout, provider.search(&ctx, query))
            .await
            .unwrap_or(Err(ProviderError::Timeout))
            .map_err(|source| Error::Provider {
                provider: provider.info().id.to_string(),
                source,
            })
    }

    /// Imports every `*.yml` in `dir` and re-imports files as they change,
    /// for developing definitions. Imported definitions behave like manual
    /// imports.
    pub async fn watch_definitions(&self, dir: PathBuf) -> Result<()> {
        use notify::{RecursiveMode, Watcher};

        for path in definition_files(&dir)? {
            self.reload_definition_file(&path).await;
        }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event
                    && (event.kind.is_modify() || event.kind.is_create())
                {
                    for path in event.paths {
                        let _ = tx.send(path);
                    }
                }
            })
            .map_err(|e| Error::Config(format!("cannot watch {}: {e}", dir.display())))?;
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|e| Error::Config(format!("cannot watch {}: {e}", dir.display())))?;
        *self.inner.watcher.lock().expect("lock") = Some(watcher);

        let engine = self.clone();
        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                // Editors write in bursts: wait for quiet, then reload once.
                let mut changed = vec![first];
                while let Ok(Some(more)) =
                    tokio::time::timeout(Duration::from_millis(300), rx.recv()).await
                {
                    changed.push(more);
                }
                changed.sort();
                changed.dedup();
                for path in changed.iter().filter(|p| is_definition_file(p)) {
                    engine.reload_definition_file(path).await;
                }
            }
        });
        tracing::info!(dir = %dir.display(), "watching definitions for changes");
        Ok(())
    }

    async fn reload_definition_file(&self, path: &Path) {
        let yaml = match tokio::fs::read_to_string(path).await {
            Ok(yaml) => yaml,
            Err(e) => {
                tracing::warn!(file = %path.display(), error = %e, "could not read definition");
                return;
            }
        };
        match self.import_definition(&yaml, None).await {
            Ok(view) => {
                tracing::info!(file = %path.display(), provider = %view.id, "definition loaded")
            }
            Err(e) => tracing::warn!(file = %path.display(), error = %e, "definition rejected"),
        }
    }
}

fn is_definition_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yml" | "yaml")
    )
}

fn definition_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_definition_file(p))
        .collect();
    files.sort();
    Ok(files)
}
