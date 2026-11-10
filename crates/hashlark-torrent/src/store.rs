// SPDX-License-Identifier: GPL-3.0-or-later

//! Queries on the `torrents` table (core migration `0004_torrents.sql`):
//! what librqbit doesn't track.

use std::path::PathBuf;

use hashlark_core::{Result, Store};
use sqlx::Row as _;
use sqlx::sqlite::SqliteRow;

use crate::model::{Origin, RunMode, SeedLimits};

/// One row of the `torrents` table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub info_hash: String,
    pub name: String,
    pub save_dir: PathBuf,
    pub source: Origin,
    pub result_id: Option<String>,
    pub added_at: i64,
    pub completed_at: Option<i64>,
    pub seeding_since: Option<i64>,
    pub run_mode: RunMode,
    pub queue_pos: i64,
    pub peer_limit: Option<u32>,
    pub seed: SeedLimits,
    pub down_kib: Option<u32>,
    pub up_kib: Option<u32>,
    pub downloaded_total: u64,
    pub uploaded_total: u64,
    pub done: bool,
}

impl Row {
    /// True when the torrent has its own speed or connection limit, which
    /// librqbit only takes when the torrent is added.
    pub fn has_add_options(&self) -> bool {
        self.peer_limit.is_some() || self.down_kib.is_some() || self.up_kib.is_some()
    }
}

fn opt_u32(v: Option<i64>) -> Option<u32> {
    v.and_then(|v| u32::try_from(v).ok())
}

fn to_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn from_row(row: &SqliteRow) -> sqlx::Result<Row> {
    let save_dir: String = row.try_get("save_dir")?;
    let source: String = row.try_get("source")?;
    let run_mode: String = row.try_get("run_mode")?;
    let seed_ratio: Option<f64> = row.try_get("seed_ratio")?;
    Ok(Row {
        info_hash: row.try_get("info_hash")?,
        name: row.try_get("name")?,
        save_dir: PathBuf::from(save_dir),
        source: Origin::parse(&source),
        result_id: row.try_get("result_id")?,
        added_at: row.try_get("added_at")?,
        completed_at: row.try_get("completed_at")?,
        seeding_since: row.try_get("seeding_since")?,
        run_mode: RunMode::parse(&run_mode),
        queue_pos: row.try_get("queue_pos")?,
        peer_limit: opt_u32(row.try_get("peer_limit")?),
        seed: SeedLimits {
            ratio: seed_ratio.map(|r| r as f32),
            minutes: opt_u32(row.try_get("seed_minutes")?),
        },
        down_kib: opt_u32(row.try_get("down_kib")?),
        up_kib: opt_u32(row.try_get("up_kib")?),
        downloaded_total: to_u64(row.try_get("downloaded_total")?),
        uploaded_total: to_u64(row.try_get("uploaded_total")?),
        done: row.try_get::<i64, _>("done")? != 0,
    })
}

/// All rows in queue order.
pub(crate) async fn list(store: &Store) -> Result<Vec<Row>> {
    let rows = sqlx::query("SELECT * FROM torrents ORDER BY queue_pos, added_at")
        .fetch_all(store.pool())
        .await?;
    Ok(rows.iter().map(from_row).collect::<sqlx::Result<_>>()?)
}

pub(crate) async fn insert(store: &Store, row: &Row) -> Result<()> {
    sqlx::query(
        "INSERT INTO torrents (info_hash, name, save_dir, source, result_id, added_at,
             completed_at, seeding_since, run_mode, queue_pos, peer_limit, seed_ratio,
             seed_minutes, down_kib, up_kib, downloaded_total, uploaded_total, done)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.info_hash)
    .bind(&row.name)
    .bind(row.save_dir.to_string_lossy().into_owned())
    .bind(row.source.as_str())
    .bind(&row.result_id)
    .bind(row.added_at)
    .bind(row.completed_at)
    .bind(row.seeding_since)
    .bind(row.run_mode.as_str())
    .bind(row.queue_pos)
    .bind(row.peer_limit.map(i64::from))
    .bind(row.seed.ratio.map(f64::from))
    .bind(row.seed.minutes.map(i64::from))
    .bind(row.down_kib.map(i64::from))
    .bind(row.up_kib.map(i64::from))
    .bind(to_i64(row.downloaded_total))
    .bind(to_i64(row.uploaded_total))
    .bind(i64::from(row.done))
    .execute(store.pool())
    .await?;
    Ok(())
}

/// Writes every mutable column of `row`.
pub(crate) async fn update(store: &Store, row: &Row) -> Result<()> {
    sqlx::query(
        "UPDATE torrents SET name = ?, save_dir = ?, completed_at = ?, seeding_since = ?,
             run_mode = ?, queue_pos = ?, peer_limit = ?, seed_ratio = ?, seed_minutes = ?,
             down_kib = ?, up_kib = ?, downloaded_total = ?, uploaded_total = ?, done = ?
         WHERE info_hash = ?",
    )
    .bind(&row.name)
    .bind(row.save_dir.to_string_lossy().into_owned())
    .bind(row.completed_at)
    .bind(row.seeding_since)
    .bind(row.run_mode.as_str())
    .bind(row.queue_pos)
    .bind(row.peer_limit.map(i64::from))
    .bind(row.seed.ratio.map(f64::from))
    .bind(row.seed.minutes.map(i64::from))
    .bind(row.down_kib.map(i64::from))
    .bind(row.up_kib.map(i64::from))
    .bind(to_i64(row.downloaded_total))
    .bind(to_i64(row.uploaded_total))
    .bind(i64::from(row.done))
    .bind(&row.info_hash)
    .execute(store.pool())
    .await?;
    Ok(())
}

/// Writes the cumulative totals of many torrents in one transaction.
pub(crate) async fn set_totals(store: &Store, totals: &[(String, u64, u64)]) -> Result<()> {
    if totals.is_empty() {
        return Ok(());
    }
    let mut tx = store.pool().begin().await?;
    for (hash, down, up) in totals {
        sqlx::query(
            "UPDATE torrents SET downloaded_total = ?, uploaded_total = ? WHERE info_hash = ?",
        )
        .bind(to_i64(*down))
        .bind(to_i64(*up))
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Rewrites `queue_pos` as 0, 1, 2, ... in the given order.
pub(crate) async fn set_queue(store: &Store, order: &[String]) -> Result<()> {
    let mut tx = store.pool().begin().await?;
    for (pos, hash) in order.iter().enumerate() {
        sqlx::query("UPDATE torrents SET queue_pos = ? WHERE info_hash = ?")
            .bind(i64::try_from(pos).unwrap_or(i64::MAX))
            .bind(hash)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn delete(store: &Store, hash: &str) -> Result<()> {
    sqlx::query("DELETE FROM torrents WHERE info_hash = ?")
        .bind(hash)
        .execute(store.pool())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(hash: &str, pos: i64) -> Row {
        Row {
            info_hash: hash.into(),
            name: format!("t{pos}"),
            save_dir: PathBuf::from("/downloads"),
            source: Origin::Magnet,
            result_id: None,
            added_at: 1,
            completed_at: None,
            seeding_since: None,
            run_mode: RunMode::Auto,
            queue_pos: pos,
            peer_limit: None,
            seed: SeedLimits::default(),
            down_kib: None,
            up_kib: None,
            downloaded_total: 0,
            uploaded_total: 0,
            done: false,
        }
    }

    #[tokio::test]
    async fn round_trip_and_queue() {
        let store = Store::open_in_memory().await.unwrap();
        insert(&store, &row("aa", 0)).await.unwrap();
        insert(&store, &row("bb", 1)).await.unwrap();

        let mut b = row("bb", 1);
        b.run_mode = RunMode::Forced;
        b.peer_limit = Some(50);
        b.seed = SeedLimits {
            ratio: Some(2.5),
            minutes: Some(30),
        };
        b.down_kib = Some(100);
        b.done = true;
        b.completed_at = Some(5);
        update(&store, &b).await.unwrap();
        set_totals(&store, &[("bb".into(), 10, 20)]).await.unwrap();
        b.downloaded_total = 10;
        b.uploaded_total = 20;
        assert!(b.has_add_options());

        set_queue(&store, &["bb".into(), "aa".into()])
            .await
            .unwrap();
        b.queue_pos = 0;
        let rows = list(&store).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], b);
        assert_eq!(rows[1].info_hash, "aa");
        assert_eq!(rows[1].queue_pos, 1);

        delete(&store, "aa").await.unwrap();
        assert_eq!(list(&store).await.unwrap().len(), 1);
    }
}
