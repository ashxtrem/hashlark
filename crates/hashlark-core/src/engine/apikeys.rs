// SPDX-License-Identifier: GPL-3.0-or-later

//! API keys for the headless server. Keys are 256-bit random values; only
//! their SHA-256 is stored.

use serde::Serialize;
use sqlx::Row;

use super::Engine;
use crate::definition::store::sha256_hex;
use crate::error::{Error, Result};
use crate::store::now_ms;

/// Prefix that makes keys easy to recognise (e.g. in secret scanners).
const PREFIX: &str = "hlk_";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    /// Unix ms.
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// A newly created key. The key itself is shown only once.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NewApiKey {
    #[serde(flatten)]
    pub info: ApiKeyInfo,
    pub key: String,
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("OS random number generator");
    data_encoding::HEXLOWER.encode(&buf)
}

impl Engine {
    pub async fn create_api_key(&self, name: &str) -> Result<NewApiKey> {
        let name = name.trim();
        if name.is_empty() || name.len() > 64 {
            return Err(Error::Invalid(
                "the key name must be 1–64 characters".into(),
            ));
        }
        let key = format!("{PREFIX}{}", random_hex(32));
        let info = ApiKeyInfo {
            id: random_hex(8),
            name: name.to_owned(),
            created_at: now_ms(),
            last_used_at: None,
        };
        sqlx::query("INSERT INTO api_keys (id, name, key_hash, created_at) VALUES (?, ?, ?, ?)")
            .bind(&info.id)
            .bind(&info.name)
            .bind(sha256_hex(key.as_bytes()))
            .bind(info.created_at)
            .execute(self.store().pool())
            .await?;
        Ok(NewApiKey { info, key })
    }

    pub async fn api_keys(&self) -> Result<Vec<ApiKeyInfo>> {
        let rows = sqlx::query(
            "SELECT id, name, created_at, last_used_at FROM api_keys ORDER BY created_at",
        )
        .fetch_all(self.store().pool())
        .await?;
        rows.iter()
            .map(|r| {
                Ok(ApiKeyInfo {
                    id: r.try_get("id")?,
                    name: r.try_get("name")?,
                    created_at: r.try_get("created_at")?,
                    last_used_at: r.try_get("last_used_at")?,
                })
            })
            .collect()
    }

    pub async fn revoke_api_key(&self, id: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM api_keys WHERE id = ?")
            .bind(id)
            .execute(self.store().pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(Error::NotFound(format!("API key `{id}`")));
        }
        Ok(())
    }

    /// Whether `key` is a valid API key; records its use.
    pub async fn verify_api_key(&self, key: &str) -> Result<bool> {
        if !key.starts_with(PREFIX) {
            return Ok(false);
        }
        let done = sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE key_hash = ?")
            .bind(now_ms())
            .bind(sha256_hex(key.as_bytes()))
            .execute(self.store().pool())
            .await?;
        Ok(done.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::engine::{Engine, EngineOptions};
    use crate::secrets::MemorySecretStore;
    use crate::{AppPaths, Store};

    #[tokio::test]
    async fn create_verify_revoke() {
        let engine = Engine::with_store(
            AppPaths::at(std::env::temp_dir()),
            Store::open_in_memory().await.unwrap(),
            EngineOptions {
                secrets: Arc::new(MemorySecretStore::default()),
                builtins: vec![],
                definitions: vec![],
            },
        )
        .await
        .unwrap();

        let new = engine.create_api_key("phone").await.unwrap();
        assert!(new.key.starts_with("hlk_") && new.key.len() == 68);
        assert!(engine.verify_api_key(&new.key).await.unwrap());
        assert!(!engine.verify_api_key("hlk_nope").await.unwrap());
        assert!(!engine.verify_api_key("not a key").await.unwrap());

        let listed = engine.api_keys().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].last_used_at.is_some());

        let stored: String = sqlx::query_scalar("SELECT key_hash FROM api_keys")
            .fetch_one(engine.store().pool())
            .await
            .unwrap();
        assert_ne!(stored, new.key, "only the hash is stored");

        engine.revoke_api_key(&new.info.id).await.unwrap();
        assert!(!engine.verify_api_key(&new.key).await.unwrap());
        assert!(engine.create_api_key("  ").await.is_err());
    }
}
