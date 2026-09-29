// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider records in the database: which providers exist, whether they're
//! enabled, and their per-provider configuration.

use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use url::Url;

use crate::error::{Error, Result};
use crate::provider::{ProviderInfo, ProviderKind};
use crate::store::{Store, now_ms};

/// How a provider's traffic is routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Route {
    /// Follow the global network settings.
    #[default]
    Default,
    /// Never use a proxy or Tor for this provider.
    Direct,
    /// Use this provider's own proxy.
    Proxy,
    /// Always go through Tor.
    Tor,
}

/// Per-provider network policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct NetworkPolicy {
    pub route: Route,
    /// Used when `route` is `proxy`.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub proxy: Option<Url>,
}

/// A row of the `providers` table.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRecord {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    pub enabled: bool,
    pub definition_id: Option<String>,
    /// Kind-specific, non-secret configuration (e.g. a Torznab URL, or the
    /// non-secret values of definition settings).
    pub config: serde_json::Value,
    pub network_policy: NetworkPolicy,
    pub created_at: i64,
    /// Set while the provider is auto-disabled by the health tracker.
    pub disabled_until: Option<i64>,
}

impl ProviderRecord {
    pub fn new(id: impl Into<String>, kind: ProviderKind, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            name: name.into(),
            enabled: true,
            definition_id: None,
            config: serde_json::json!({}),
            network_policy: NetworkPolicy::default(),
            created_at: now_ms(),
            disabled_until: None,
        }
    }

    /// Whether searches should include this provider right now.
    pub fn is_active(&self, now: i64) -> bool {
        self.enabled && self.disabled_until.is_none_or(|until| until <= now)
    }
}

const COLUMNS: &str = "id, kind, name, enabled, definition_id, config_json, network_policy_json,
                       created_at, disabled_until";

fn from_row(row: &SqliteRow) -> Result<ProviderRecord> {
    let kind: String = row.try_get("kind")?;
    let config: String = row.try_get("config_json")?;
    let policy: String = row.try_get("network_policy_json")?;
    Ok(ProviderRecord {
        id: row.try_get("id")?,
        kind: serde_json::from_value(serde_json::Value::String(kind))?,
        name: row.try_get("name")?,
        enabled: row.try_get("enabled")?,
        definition_id: row.try_get("definition_id")?,
        config: serde_json::from_str(&config)?,
        network_policy: serde_json::from_str(&policy).unwrap_or_default(),
        created_at: row.try_get("created_at")?,
        disabled_until: row.try_get("disabled_until")?,
    })
}

fn kind_str(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Native => "native",
        ProviderKind::Definition => "definition",
        ProviderKind::Torznab => "torznab",
    }
}

pub async fn list(store: &Store) -> Result<Vec<ProviderRecord>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM providers ORDER BY name COLLATE NOCASE, id"
    )))
    .fetch_all(store.pool())
    .await?;
    rows.iter().map(from_row).collect()
}

pub async fn get(store: &Store, id: &str) -> Result<ProviderRecord> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM providers WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(store.pool())
    .await?
    .ok_or_else(|| Error::NotFound(format!("provider `{id}`")))?;
    from_row(&row)
}

pub async fn insert(store: &Store, record: &ProviderRecord) -> Result<()> {
    let result = sqlx::query(
        "INSERT INTO providers (id, kind, name, enabled, definition_id, config_json,
                                network_policy_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&record.id)
    .bind(kind_str(record.kind))
    .bind(&record.name)
    .bind(record.enabled)
    .bind(&record.definition_id)
    .bind(record.config.to_string())
    .bind(serde_json::to_string(&record.network_policy)?)
    .bind(record.created_at)
    .execute(store.pool())
    .await;
    match result {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(Error::Invalid(format!(
            "a provider with id `{}` already exists",
            record.id
        ))),
        other => other.map(|_| ()).map_err(Into::into),
    }
}

/// Saves the user-editable fields of `record`.
pub async fn update(store: &Store, record: &ProviderRecord) -> Result<()> {
    let result = sqlx::query(
        "UPDATE providers SET name = ?, enabled = ?, definition_id = ?, config_json = ?,
                              network_policy_json = ?
         WHERE id = ?",
    )
    .bind(&record.name)
    .bind(record.enabled)
    .bind(&record.definition_id)
    .bind(record.config.to_string())
    .bind(serde_json::to_string(&record.network_policy)?)
    .bind(&record.id)
    .execute(store.pool())
    .await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("provider `{}`", record.id)));
    }
    Ok(())
}

/// Re-enables an auto-disabled provider immediately.
pub async fn clear_auto_disable(store: &Store, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE providers SET disabled_until = NULL, consecutive_failures = 0 WHERE id = ?",
    )
    .bind(id)
    .execute(store.pool())
    .await?;
    Ok(())
}

pub async fn delete(store: &Store, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM providers WHERE id = ?")
        .bind(id)
        .execute(store.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("provider `{id}`")));
    }
    sqlx::query("DELETE FROM result_cache WHERE cache_key LIKE ?")
        .bind(format!("search:{id}:%"))
        .execute(store.pool())
        .await?;
    Ok(())
}

/// Makes sure a built-in provider has a row, keeping the user's enabled
/// flag if it already exists.
pub async fn ensure_builtin(store: &Store, info: &ProviderInfo) -> Result<()> {
    sqlx::query(
        "INSERT INTO providers (id, kind, name, enabled, config_json, network_policy_json, created_at)
         VALUES (?, ?, ?, 1, '{}', '{}', ?)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name",
    )
    .bind(info.id.as_str())
    .bind(kind_str(info.kind))
    .bind(&info.name)
    .bind(now_ms())
    .execute(store.pool())
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::InternetArchive;
    use crate::provider::SearchProvider;

    #[tokio::test]
    async fn crud_round_trip() {
        let store = Store::open_in_memory().await.unwrap();
        let mut record = ProviderRecord::new("jackett-1", ProviderKind::Torznab, "My Jackett");
        record.config = serde_json::json!({ "url": "http://127.0.0.1:9117/api" });
        insert(&store, &record).await.unwrap();
        assert!(matches!(
            insert(&store, &record).await,
            Err(Error::Invalid(_))
        ));

        record.enabled = false;
        record.network_policy.route = Route::Tor;
        update(&store, &record).await.unwrap();
        let loaded = get(&store, "jackett-1").await.unwrap();
        assert_eq!(loaded, record);

        delete(&store, "jackett-1").await.unwrap();
        assert!(matches!(
            get(&store, "jackett-1").await,
            Err(Error::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn ensure_builtin_keeps_enabled_flag() {
        let store = Store::open_in_memory().await.unwrap();
        let ia = InternetArchive::new();
        ensure_builtin(&store, ia.info()).await.unwrap();

        let mut record = get(&store, "internet-archive").await.unwrap();
        assert!(record.enabled);
        record.enabled = false;
        update(&store, &record).await.unwrap();

        ensure_builtin(&store, ia.info()).await.unwrap();
        assert!(!get(&store, "internet-archive").await.unwrap().enabled);
        assert_eq!(list(&store).await.unwrap().len(), 1);
    }

    #[test]
    fn active_respects_auto_disable() {
        let mut r = ProviderRecord::new("x", ProviderKind::Native, "X");
        assert!(r.is_active(100));
        r.disabled_until = Some(200);
        assert!(!r.is_active(100));
        assert!(r.is_active(200));
        r.enabled = false;
        assert!(!r.is_active(300));
    }
}
