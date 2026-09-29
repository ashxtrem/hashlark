// SPDX-License-Identifier: GPL-3.0-or-later

//! Adding, syncing and removing definition repositories.

use std::time::Duration;

use serde::Serialize;
use sqlx::Row;
use url::Url;

use super::Engine;
use super::providers::FIRST_PARTY_REPO;
use crate::definition;
use crate::definition::store::{self as defstore, sha256_hex};
use crate::error::{Error, Result};
use crate::provider::{ProviderKind, read_text, send};
use crate::registry::{self, ProviderRecord};
use crate::repos::{self, RepoIndex};
use crate::store::now_ms;
use crate::util::fnv1a64;

/// How often repositories and the tracker list are refreshed.
const REFRESH_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Delay before the first background refresh after start-up.
const FIRST_REFRESH_AFTER: Duration = Duration::from_secs(60);

/// A definition repository as shown to users.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RepoView {
    pub id: String,
    pub url: String,
    pub name: Option<String>,
    /// Short form of the pinned signing key.
    pub fingerprint: Option<String>,
    /// Unix ms.
    pub last_sync_at: Option<i64>,
    pub last_version: Option<i64>,
    pub definitions: i64,
    /// Shipped with Hashlark; can't be removed.
    pub builtin: bool,
}

/// What a sync changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SyncReport {
    /// New providers (added disabled, for the user to review).
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    pub unchanged: usize,
    /// Definitions that were skipped, and why.
    pub errors: Vec<String>,
}

impl Engine {
    pub async fn repos(&self) -> Result<Vec<RepoView>> {
        let rows = sqlx::query(
            "SELECT r.id, r.url, r.name, r.public_key, r.last_sync_at, r.last_version,
                    (SELECT COUNT(*) FROM definitions d WHERE d.repo_id = r.id) AS n
             FROM definition_repos r ORDER BY r.created_at, r.id",
        )
        .fetch_all(self.store().pool())
        .await?;
        rows.iter()
            .map(|row| {
                let id: String = row.try_get("id")?;
                let key: Option<String> = row.try_get("public_key")?;
                Ok(RepoView {
                    builtin: id == FIRST_PARTY_REPO,
                    id,
                    url: row.try_get("url")?,
                    name: row.try_get("name")?,
                    fingerprint: key.as_deref().map(repos::fingerprint),
                    last_sync_at: row.try_get("last_sync_at")?,
                    last_version: row.try_get("last_version")?,
                    definitions: row.try_get("n")?,
                })
            })
            .collect()
    }

    async fn repo(&self, id: &str) -> Result<RepoView> {
        self.repos()
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| Error::NotFound(format!("repository `{id}`")))
    }

    async fn fetch_index(&self, url: &Url) -> Result<(Vec<u8>, String)> {
        let fetch = |url: Url| async move {
            let response = send(self.http()?.get(url.clone()))
                .await
                .map_err(|source| Error::Provider {
                    provider: url.host_str().unwrap_or("repository").to_owned(),
                    source,
                })?;
            read_text(response).await.map_err(|source| Error::Provider {
                provider: "repository".into(),
                source,
            })
        };
        let index = fetch(url.clone()).await?;
        let sig_url = Url::parse(&format!("{url}.sig")).expect("valid URL");
        let signature = fetch(sig_url).await.map_err(|_| {
            Error::Invalid("the repository has no signature (index.json.sig)".into())
        })?;
        Ok((index.into_bytes(), signature))
    }

    /// Adds a repository and installs its definitions (disabled).
    pub async fn add_repo(&self, url: &str) -> Result<(RepoView, SyncReport)> {
        let url = repos::normalize_repo_url(url)?;
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM definition_repos WHERE url = ?")
                .bind(url.as_str())
                .fetch_optional(self.store().pool())
                .await?;
        if exists.is_some() {
            return Err(Error::Invalid("this repository is already added".into()));
        }
        let (bytes, signature) = self.fetch_index(&url).await?;
        let index = repos::verify_index(&bytes, &signature, None)?;
        let id = format!("repo-{:08x}", fnv1a64(url.as_str()) as u32);
        sqlx::query(
            "INSERT INTO definition_repos (id, url, name, public_key, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(url.as_str())
        .bind(&index.name)
        .bind(&index.public_key)
        .bind(now_ms())
        .execute(self.store().pool())
        .await?;
        let report = self.apply_index(&id, &url, &index).await?;
        Ok((self.repo(&id).await?, report))
    }

    /// Fetches a repository's latest index and applies the changes.
    pub async fn sync_repo(&self, id: &str) -> Result<SyncReport> {
        if id == FIRST_PARTY_REPO {
            return Ok(SyncReport::default());
        }
        let row = sqlx::query("SELECT url, public_key FROM definition_repos WHERE id = ?")
            .bind(id)
            .fetch_optional(self.store().pool())
            .await?
            .ok_or_else(|| Error::NotFound(format!("repository `{id}`")))?;
        let url: Url = row
            .try_get::<String, _>("url")?
            .parse()
            .map_err(|_| Error::Invalid("stored repository URL is invalid".into()))?;
        let pinned: Option<String> = row.try_get("public_key")?;
        let (bytes, signature) = self.fetch_index(&url).await?;
        let index = repos::verify_index(&bytes, &signature, pinned.as_deref())?;
        self.apply_index(id, &url, &index).await
    }

    /// Syncs every added repository; failures are logged, not fatal.
    pub async fn sync_all_repos(&self) -> Vec<(String, Result<SyncReport>)> {
        let mut out = Vec::new();
        let Ok(repos) = self.repos().await else {
            return out;
        };
        for repo in repos.into_iter().filter(|r| !r.builtin) {
            let result = self.sync_repo(&repo.id).await;
            if let Err(e) = &result {
                tracing::warn!(repo = %repo.url, error = %e, "repository sync failed");
            }
            out.push((repo.id, result));
        }
        out
    }

    async fn apply_index(&self, repo_id: &str, url: &Url, index: &RepoIndex) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        let http = self.http()?;
        for entry in &index.definitions {
            let stored = defstore::get(self.store(), &entry.id).await.ok();
            if let Some(s) = &stored
                && s.repo_id.as_deref() != Some(repo_id)
            {
                report.errors.push(format!(
                    "{}: a provider with this id already exists from another source",
                    entry.id
                ));
                continue;
            }
            if stored
                .as_ref()
                .is_some_and(|s| s.version >= i64::from(entry.version))
            {
                report.unchanged += 1;
                continue;
            }
            let yaml = match self.fetch_definition(&http, url, entry).await {
                Ok(yaml) => yaml,
                Err(e) => {
                    report.errors.push(format!("{}: {e}", entry.id));
                    continue;
                }
            };
            let def = match definition::load(&yaml) {
                Ok(def) if def.spec.id == entry.id => def.spec,
                Ok(def) => {
                    report.errors.push(format!(
                        "{}: the file declares id `{}`",
                        entry.id, def.spec.id
                    ));
                    continue;
                }
                Err(e) => {
                    report.errors.push(format!("{}: {e}", entry.id));
                    continue;
                }
            };
            defstore::upsert(
                self.store(),
                &def.id,
                Some(repo_id),
                def.version.into(),
                &yaml,
            )
            .await?;
            if registry::get(self.store(), &def.id).await.is_err() {
                let mut record = ProviderRecord::new(&def.id, ProviderKind::Definition, &def.name);
                record.definition_id = Some(def.id.clone());
                // New providers from a repository wait for the user to enable them.
                record.enabled = false;
                registry::insert(self.store(), &record).await?;
                report.added.push(def.id);
            } else {
                report.updated.push(def.id);
            }
        }

        let listed: Vec<&str> = index.definitions.iter().map(|e| e.id.as_str()).collect();
        for stored in defstore::list_for_repo(self.store(), repo_id).await? {
            if !listed.contains(&stored.id.as_str()) {
                let _ = registry::delete(self.store(), &stored.id).await;
                defstore::delete(self.store(), &stored.id).await?;
                report.removed.push(stored.id);
            }
        }

        sqlx::query(
            "UPDATE definition_repos SET name = ?, last_sync_at = ?, last_version = ? WHERE id = ?",
        )
        .bind(&index.name)
        .bind(now_ms())
        .bind(i64::try_from(index.version).unwrap_or(i64::MAX))
        .bind(repo_id)
        .execute(self.store().pool())
        .await?;
        self.rebuild().await?;
        Ok(report)
    }

    async fn fetch_definition(
        &self,
        http: &reqwest::Client,
        index_url: &Url,
        entry: &repos::RepoEntry,
    ) -> Result<String> {
        let url = repos::entry_url(index_url, entry)?;
        let response = send(http.get(url))
            .await
            .map_err(|source| Error::Provider {
                provider: "repository".into(),
                source,
            })?;
        let yaml = read_text(response)
            .await
            .map_err(|source| Error::Provider {
                provider: "repository".into(),
                source,
            })?;
        if sha256_hex(yaml.as_bytes()) != entry.sha256.to_ascii_lowercase() {
            return Err(Error::Invalid("the file doesn't match its checksum".into()));
        }
        Ok(yaml)
    }

    /// Removes a repository and the providers it installed.
    pub async fn remove_repo(&self, id: &str) -> Result<()> {
        if id == FIRST_PARTY_REPO {
            return Err(Error::Invalid(
                "the built-in repository can't be removed".into(),
            ));
        }
        self.repo(id).await?;
        for stored in defstore::list_for_repo(self.store(), id).await? {
            if registry::get(self.store(), &stored.id).await.is_ok() {
                self.remove_provider_unchecked(&stored.id).await?;
            }
        }
        sqlx::query("DELETE FROM definition_repos WHERE id = ?")
            .bind(id)
            .execute(self.store().pool())
            .await?;
        self.rebuild().await
    }

    /// Syncs repositories and the tracker list once a day in the
    /// background, for as long as the engine lives.
    pub fn start_background_tasks(&self) -> tokio::task::JoinHandle<()> {
        let weak = std::sync::Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            tokio::time::sleep(FIRST_REFRESH_AFTER).await;
            loop {
                let Some(inner) = weak.upgrade() else { return };
                let engine = Engine { inner };
                engine.sync_all_repos().await;
                if engine.settings().magnets.trackers_url.is_some()
                    && let Err(e) = engine.refresh_trackers().await
                {
                    tracing::warn!(error = %e, "tracker list refresh failed");
                }
                drop(engine);
                tokio::time::sleep(REFRESH_EVERY).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::AppPaths;
    use crate::engine::{Engine, EngineOptions};
    use crate::repos::{build_index, generate_keypair, sign};
    use crate::secrets::MemorySecretStore;
    use crate::store::Store;

    async fn engine() -> Engine {
        Engine::with_store(
            AppPaths::at(std::env::temp_dir().join("hashlark-repo-tests")),
            Store::open_in_memory().await.unwrap(),
            EngineOptions {
                secrets: Arc::new(MemorySecretStore::default()),
                builtins: vec![],
                definitions: vec![],
            },
        )
        .await
        .unwrap()
    }

    /// Serves a signed repository with the given definitions.
    async fn serve(
        server: &MockServer,
        signing: &str,
        public: &str,
        version: u64,
        defs: &[(&str, String)],
    ) {
        server.reset().await;
        let files: Vec<(String, String)> = defs
            .iter()
            .map(|(f, y)| ((*f).to_owned(), y.clone()))
            .collect();
        let index = build_index("Test repo", version, public, &files).unwrap();
        let bytes = serde_json::to_vec_pretty(&index).unwrap();
        Mock::given(path("/defs/index.json.sig"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(sign(signing, &bytes).unwrap()),
            )
            .mount(server)
            .await;
        Mock::given(path("/defs/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(server)
            .await;
        for (file, yaml) in defs {
            Mock::given(path(format!("/defs/{file}")))
                .respond_with(ResponseTemplate::new(200).set_body_string(yaml.clone()))
                .mount(server)
                .await;
        }
    }

    fn def(id: &str, version: u32) -> String {
        crate::repos::tests::DEF
            .replace("id: repo-site", &format!("id: {id}"))
            .replace("version: 3", &format!("version: {version}"))
    }

    #[tokio::test]
    async fn add_sync_update_remove() {
        let server = MockServer::start().await;
        let (signing, public) = generate_keypair();
        serve(
            &server,
            &signing,
            &public,
            1,
            &[("a.yml", def("site-a", 1)), ("b.yml", def("site-b", 1))],
        )
        .await;

        let engine = engine().await;
        let (repo, report) = engine
            .add_repo(&format!("{}/defs", server.uri()))
            .await
            .unwrap();
        assert_eq!(report.added, ["site-a", "site-b"]);
        assert_eq!(repo.definitions, 2);
        assert!(repo.fingerprint.is_some());
        let a = engine.provider("site-a").await.unwrap();
        assert!(!a.enabled, "new repository providers start disabled");
        assert!(!a.builtin);

        // v2: site-a updated, site-b dropped, site-c added.
        serve(
            &server,
            &signing,
            &public,
            2,
            &[("a.yml", def("site-a", 2)), ("c.yml", def("site-c", 1))],
        )
        .await;
        let report = engine.sync_repo(&repo.id).await.unwrap();
        assert_eq!(report.updated, ["site-a"]);
        assert_eq!(report.added, ["site-c"]);
        assert_eq!(report.removed, ["site-b"]);
        assert!(engine.provider("site-b").await.is_err());

        // A new signing key is refused.
        let (other_signing, other_public) = generate_keypair();
        serve(
            &server,
            &other_signing,
            &other_public,
            3,
            &[("a.yml", def("site-a", 3))],
        )
        .await;
        let err = engine.sync_repo(&repo.id).await.unwrap_err().to_string();
        assert!(err.contains("signing key changed"), "{err}");

        engine.remove_repo(&repo.id).await.unwrap();
        assert!(engine.provider("site-a").await.is_err());
        assert!(engine.repos().await.unwrap().iter().all(|r| r.builtin));
    }

    #[tokio::test]
    async fn tampered_definitions_are_skipped() {
        let server = MockServer::start().await;
        let (signing, public) = generate_keypair();
        serve(
            &server,
            &signing,
            &public,
            1,
            &[("a.yml", def("site-a", 1))],
        )
        .await;
        // Serve a different file than the one the index was built from.
        Mock::given(path("/defs/a.yml"))
            .respond_with(ResponseTemplate::new(200).set_body_string(def("site-a", 9)))
            .with_priority(1)
            .mount(&server)
            .await;
        let engine = engine().await;
        let (_, report) = engine
            .add_repo(&format!("{}/defs/index.json", server.uri()))
            .await
            .unwrap();
        assert!(report.added.is_empty());
        assert!(report.errors[0].contains("checksum"), "{:?}", report.errors);
    }

    #[tokio::test]
    async fn unsigned_repositories_are_refused() {
        let server = MockServer::start().await;
        Mock::given(path("/defs/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        let engine = engine().await;
        let err = engine
            .add_repo(&format!("{}/defs", server.uri()))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("signature"), "{err}");
    }
}
