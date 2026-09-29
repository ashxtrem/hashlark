// SPDX-License-Identifier: GPL-3.0-or-later

//! SQLite-backed persistence.

use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::error::Result;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Handle to the Hashlark database. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens (creating if needed) the database at `path` and applies any
    /// pending migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        Self::from_pool(pool).await
    }

    /// Opens a private in-memory database. Intended for tests.
    pub async fn open_in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // Each in-memory connection is its own database, so keep exactly one.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        Self::from_pool(pool).await
    }

    async fn from_pool(pool: SqlitePool) -> Result<Self> {
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Version of the newest migration this build knows about.
    pub fn latest_schema_version() -> i64 {
        MIGRATOR.migrations.last().map_or(0, |m| m.version)
    }

    /// Latest applied migration version, or `None` for an empty database.
    pub async fn schema_version(&self) -> Result<Option<i64>> {
        let version =
            sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(version)
    }

    /// Reads a JSON setting.
    pub async fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let row = sqlx::query("SELECT value_json FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => {
                let json: String = row.try_get("value_json")?;
                Ok(Some(serde_json::from_str(&json)?))
            }
            None => Ok(None),
        }
    }

    /// Writes a JSON setting, replacing any existing value.
    pub async fn set_setting<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        sqlx::query(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json,
                                             updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(json)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Current time as Unix epoch milliseconds, the timestamp format used in
/// every table.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_to_fresh_database() {
        let store = Store::open_in_memory().await.unwrap();
        assert_eq!(
            store.schema_version().await.unwrap(),
            Some(Store::latest_schema_version())
        );
    }

    #[tokio::test]
    async fn settings_round_trip_and_overwrite() {
        let store = Store::open_in_memory().await.unwrap();
        assert_eq!(store.get_setting::<u32>("timeout_s").await.unwrap(), None);

        store.set_setting("timeout_s", &10u32).await.unwrap();
        store.set_setting("timeout_s", &12u32).await.unwrap();
        assert_eq!(
            store.get_setting::<u32>("timeout_s").await.unwrap(),
            Some(12)
        );
    }

    #[tokio::test]
    async fn open_creates_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hashlark.db");
        let store = Store::open(&path).await.unwrap();
        assert!(path.exists());
        assert_eq!(
            store.schema_version().await.unwrap(),
            Some(Store::latest_schema_version())
        );
    }
}
