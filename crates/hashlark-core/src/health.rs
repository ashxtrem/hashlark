// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-provider health: a rolling log of attempts, and automatic disabling
//! after repeated failures with a growing retry delay.

use serde::Serialize;
use sqlx::Row;

use crate::error::{ErrorKind, Result};
use crate::store::{Store, now_ms};

/// Failures in a row before a provider is auto-disabled.
pub const FAILURES_BEFORE_DISABLE: u32 = 5;
/// Attempts kept per provider.
const HISTORY_LEN: i64 = 50;
/// Attempts used for the summary statistics.
const WINDOW: i64 = 20;
/// Retry delays after the 1st, 2nd and later auto-disables.
const BACKOFF_MS: [i64; 3] = [15 * 60_000, 60 * 60_000, 6 * 60 * 60_000];

/// Overall state shown next to a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    /// Never searched.
    Unknown,
    Ok,
    /// Some recent attempts failed.
    Degraded,
    /// The last attempts failed, but it isn't disabled yet.
    Failing,
    /// Skipped by searches until `disabled_until`.
    AutoDisabled,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HealthSummary {
    pub state: HealthState,
    /// Share of successful attempts in the recent window.
    pub success_rate: Option<f32>,
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
    pub last_error_kind: Option<ErrorKind>,
    /// Unix ms of the last attempt.
    pub last_checked_at: Option<i64>,
    pub consecutive_failures: u32,
    /// Unix ms until which the provider is skipped, if auto-disabled.
    pub disabled_until: Option<i64>,
}

/// Records one attempt and updates the auto-disable state.
pub async fn record(
    store: &Store,
    provider_id: &str,
    latency_ms: u64,
    error: Option<ErrorKind>,
) -> Result<()> {
    let now = now_ms();
    let mut tx = store.pool().begin().await?;
    sqlx::query(
        "INSERT INTO provider_health (provider_id, ts, ok, latency_ms, error_kind)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(provider_id)
    .bind(now)
    .bind(error.is_none())
    .bind(i64::try_from(latency_ms).unwrap_or(i64::MAX))
    .bind(error.map(ErrorKind::as_str))
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM provider_health WHERE provider_id = ? AND id NOT IN (
             SELECT id FROM provider_health WHERE provider_id = ? ORDER BY ts DESC, id DESC LIMIT ?)",
    )
    .bind(provider_id)
    .bind(provider_id)
    .bind(HISTORY_LEN)
    .execute(&mut *tx)
    .await?;

    if error.is_none() {
        sqlx::query(
            "UPDATE providers SET consecutive_failures = 0, disabled_until = NULL,
                                  disable_count = 0
             WHERE id = ?",
        )
        .bind(provider_id)
        .execute(&mut *tx)
        .await?;
    } else {
        let row = sqlx::query(
            "UPDATE providers SET consecutive_failures = consecutive_failures + 1
             WHERE id = ? RETURNING consecutive_failures, disable_count",
        )
        .bind(provider_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = row {
            let failures: i64 = row.try_get("consecutive_failures")?;
            let disable_count: i64 = row.try_get("disable_count")?;
            if failures >= i64::from(FAILURES_BEFORE_DISABLE) {
                let backoff = BACKOFF_MS[usize::try_from(disable_count).unwrap_or(0).min(2)];
                sqlx::query(
                    "UPDATE providers SET disabled_until = ?, disable_count = disable_count + 1,
                                          consecutive_failures = 0
                     WHERE id = ?",
                )
                .bind(now + backoff)
                .bind(provider_id)
                .execute(&mut *tx)
                .await?;
                tracing::info!(
                    provider = provider_id,
                    retry_in_min = backoff / 60_000,
                    "provider auto-disabled after repeated failures"
                );
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Summarises recent attempts for one provider.
pub async fn summary(store: &Store, provider_id: &str) -> Result<HealthSummary> {
    let rows = sqlx::query(
        "SELECT ts, ok, latency_ms, error_kind FROM provider_health
         WHERE provider_id = ? ORDER BY ts DESC, id DESC LIMIT ?",
    )
    .bind(provider_id)
    .bind(WINDOW)
    .fetch_all(store.pool())
    .await?;

    let state_row =
        sqlx::query("SELECT consecutive_failures, disabled_until FROM providers WHERE id = ?")
            .bind(provider_id)
            .fetch_optional(store.pool())
            .await?;
    let (consecutive_failures, disabled_until) = match state_row {
        Some(row) => (
            u32::try_from(row.try_get::<i64, _>("consecutive_failures")?).unwrap_or(0),
            row.try_get::<Option<i64>, _>("disabled_until")?,
        ),
        None => (0, None),
    };
    let disabled_until = disabled_until.filter(|&t| t > now_ms());

    let mut latencies = Vec::new();
    let mut ok_count = 0usize;
    let mut last_error_kind = None;
    for row in &rows {
        let ok: bool = row.try_get("ok")?;
        if ok {
            ok_count += 1;
        } else if last_error_kind.is_none() {
            last_error_kind = row
                .try_get::<Option<String>, _>("error_kind")?
                .and_then(|k| parse_kind(&k));
        }
        if let Some(ms) = row.try_get::<Option<i64>, _>("latency_ms")? {
            latencies.push(u64::try_from(ms).unwrap_or(0));
        }
    }
    latencies.sort_unstable();

    let success_rate = (!rows.is_empty()).then(|| ok_count as f32 / rows.len() as f32);
    let last_ok = rows
        .first()
        .map(|r| r.try_get::<bool, _>("ok"))
        .transpose()?;
    let state = if disabled_until.is_some() {
        HealthState::AutoDisabled
    } else {
        match (last_ok, success_rate) {
            (None, _) => HealthState::Unknown,
            (Some(false), _) => HealthState::Failing,
            (Some(true), Some(rate)) if rate < 0.8 => HealthState::Degraded,
            (Some(true), _) => HealthState::Ok,
        }
    };

    Ok(HealthSummary {
        state,
        success_rate,
        p50_ms: percentile(&latencies, 50),
        p95_ms: percentile(&latencies, 95),
        last_error_kind: if state == HealthState::Ok {
            None
        } else {
            last_error_kind
        },
        last_checked_at: rows.first().map(|r| r.try_get("ts")).transpose()?,
        consecutive_failures,
        disabled_until,
    })
}

fn percentile(sorted: &[u64], p: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = (sorted.len() * p).div_ceil(100).saturating_sub(1);
    sorted.get(idx.min(sorted.len() - 1)).copied()
}

fn parse_kind(s: &str) -> Option<ErrorKind> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store_with_provider() -> Store {
        let store = Store::open_in_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO providers (id, kind, name, created_at) VALUES ('p', 'native', 'P', 0)",
        )
        .execute(store.pool())
        .await
        .unwrap();
        store
    }

    #[tokio::test]
    async fn unknown_then_ok_then_degraded() {
        let store = store_with_provider().await;
        assert_eq!(
            summary(&store, "p").await.unwrap().state,
            HealthState::Unknown
        );

        record(&store, "p", 100, None).await.unwrap();
        let s = summary(&store, "p").await.unwrap();
        assert_eq!(s.state, HealthState::Ok);
        assert_eq!(s.p50_ms, Some(100));

        record(&store, "p", 300, Some(ErrorKind::Timeout))
            .await
            .unwrap();
        let s = summary(&store, "p").await.unwrap();
        assert_eq!(s.state, HealthState::Failing);
        assert_eq!(s.last_error_kind, Some(ErrorKind::Timeout));

        record(&store, "p", 200, None).await.unwrap();
        let s = summary(&store, "p").await.unwrap();
        assert_eq!(s.state, HealthState::Degraded, "2 of 3 ok is below 80%");
        assert_eq!(s.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn auto_disables_after_repeated_failures_and_recovers() {
        let store = store_with_provider().await;
        for _ in 0..FAILURES_BEFORE_DISABLE {
            record(&store, "p", 10, Some(ErrorKind::Network))
                .await
                .unwrap();
        }
        let s = summary(&store, "p").await.unwrap();
        assert_eq!(s.state, HealthState::AutoDisabled);
        let until = s.disabled_until.unwrap();
        assert!(
            until > now_ms() + 14 * 60_000,
            "first backoff is 15 minutes"
        );

        record(&store, "p", 10, None).await.unwrap();
        let s = summary(&store, "p").await.unwrap();
        assert_eq!(
            s.state,
            HealthState::Degraded,
            "back in use, 1 of 6 recent attempts ok"
        );
        assert_eq!(s.disabled_until, None);
    }

    #[tokio::test]
    async fn history_is_capped() {
        let store = store_with_provider().await;
        for _ in 0..(HISTORY_LEN + 10) {
            record(&store, "p", 1, None).await.unwrap();
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_health")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(count, HISTORY_LEN);
    }

    #[test]
    fn percentiles() {
        assert_eq!(percentile(&[], 50), None);
        assert_eq!(percentile(&[5], 95), Some(5));
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 50), Some(50));
        assert_eq!(percentile(&v, 95), Some(95));
    }
}
