// SPDX-License-Identifier: GPL-3.0-or-later

//! Definitions saved in the database (the `definitions` table).

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::error::{Error, Result};
use crate::store::{Store, now_ms};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StoredDefinition {
    pub id: String,
    /// The repository it came from; `None` if imported by hand.
    pub repo_id: Option<String>,
    pub version: i64,
    pub sha256: String,
    pub yaml: String,
    /// Unix ms.
    pub updated_at: i64,
}

fn from_row(row: &SqliteRow) -> Result<StoredDefinition> {
    Ok(StoredDefinition {
        id: row.try_get("id")?,
        repo_id: row.try_get("repo_id")?,
        version: row.try_get("version")?,
        sha256: row.try_get("sha256")?,
        yaml: row.try_get("yaml")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(&Sha256::digest(bytes))
}

/// Inserts or replaces a definition.
pub async fn upsert(
    store: &Store,
    id: &str,
    repo_id: Option<&str>,
    version: i64,
    yaml: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO definitions (id, repo_id, version, sha256, yaml, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET repo_id = excluded.repo_id, version = excluded.version,
             sha256 = excluded.sha256, yaml = excluded.yaml, updated_at = excluded.updated_at",
    )
    .bind(id)
    .bind(repo_id)
    .bind(version)
    .bind(sha256_hex(yaml.as_bytes()))
    .bind(yaml)
    .bind(now_ms())
    .execute(store.pool())
    .await?;
    Ok(())
}

pub async fn get(store: &Store, id: &str) -> Result<StoredDefinition> {
    let row = sqlx::query("SELECT * FROM definitions WHERE id = ?")
        .bind(id)
        .fetch_optional(store.pool())
        .await?
        .ok_or_else(|| Error::NotFound(format!("definition `{id}`")))?;
    from_row(&row)
}

pub async fn list_for_repo(store: &Store, repo_id: &str) -> Result<Vec<StoredDefinition>> {
    let rows = sqlx::query("SELECT * FROM definitions WHERE repo_id = ? ORDER BY id")
        .bind(repo_id)
        .fetch_all(store.pool())
        .await?;
    rows.iter().map(from_row).collect()
}

pub async fn delete(store: &Store, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM definitions WHERE id = ?")
        .bind(id)
        .execute(store.pool())
        .await?;
    Ok(())
}
