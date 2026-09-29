// SPDX-License-Identifier: GPL-3.0-or-later

//! Favourites and the tracker list.

use serde::Serialize;
use sqlx::Row;

use super::Engine;
use crate::error::{Error, Result};
use crate::magnet::DEFAULT_TRACKERS;
use crate::model::MergedResult;
use crate::store::now_ms;

/// A saved result.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Favorite {
    pub result: MergedResult,
    /// Unix ms when it was saved.
    pub saved_at: i64,
}

/// A page fetched for the definition editor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FetchedPage {
    /// Final URL after redirects.
    pub url: String,
    pub status: u16,
    pub html: String,
}

/// Largest tracker list accepted from a URL.
const MAX_TRACKERS: usize = 200;

impl Engine {
    /// Saves a result from a recent search.
    pub async fn add_favorite(&self, result_id: &str) -> Result<Favorite> {
        let result = self.result(result_id).await?;
        let saved_at = now_ms();
        sqlx::query(
            "INSERT INTO favorites (result_id, snapshot_json, ts) VALUES (?, ?, ?)
             ON CONFLICT (result_id) DO UPDATE SET snapshot_json = excluded.snapshot_json",
        )
        .bind(&result.id)
        .bind(serde_json::to_string(&result)?)
        .bind(saved_at)
        .execute(self.store().pool())
        .await?;
        Ok(Favorite { result, saved_at })
    }

    pub async fn favorites(&self) -> Result<Vec<Favorite>> {
        let rows = sqlx::query("SELECT snapshot_json, ts FROM favorites ORDER BY ts DESC")
            .fetch_all(self.store().pool())
            .await?;
        rows.iter()
            .map(|row| {
                Ok(Favorite {
                    result: serde_json::from_str(&row.try_get::<String, _>("snapshot_json")?)?,
                    saved_at: row.try_get("ts")?,
                })
            })
            .collect()
    }

    pub async fn remove_favorite(&self, result_id: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM favorites WHERE result_id = ?")
            .bind(result_id)
            .execute(self.store().pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(Error::NotFound(format!("favourite `{result_id}`")));
        }
        Ok(())
    }

    /// A saved favourite, used when a result is no longer in memory.
    pub(super) async fn favorite(&self, result_id: &str) -> Result<Option<MergedResult>> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT snapshot_json FROM favorites WHERE result_id = ?")
                .bind(result_id)
                .fetch_optional(self.store().pool())
                .await?;
        json.map(|j| serde_json::from_str(&j).map_err(Into::into))
            .transpose()
    }

    /// Fetches a web page through the network settings (for the definition
    /// editor's element picker).
    pub async fn fetch_page(&self, url: &str) -> Result<FetchedPage> {
        let url = url::Url::parse(url.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| Error::Invalid("enter an http(s) URL".into()))?;
        let request = self.http()?.get(url.clone());
        let (client, request) = request.build_split();
        let request = request.map_err(|e| Error::Invalid(e.to_string()))?;
        crate::net::throttle(request.url()).await;
        let response = client.execute(request).await.map_err(|e| Error::Provider {
            provider: url.host_str().unwrap_or("site").to_owned(),
            source: crate::error::ProviderError::Network(e.to_string()),
        })?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let html = crate::provider::read_text(response)
            .await
            .map_err(|source| Error::Provider {
                provider: url.host_str().unwrap_or("site").to_owned(),
                source,
            })?;
        Ok(FetchedPage {
            url: final_url,
            status,
            html,
        })
    }

    // ----- trackers --------------------------------------------------------

    /// Trackers added to magnets: the built-in list plus the ones fetched
    /// from the user's tracker list URL.
    pub async fn trackers(&self) -> Result<Vec<String>> {
        let mut trackers: Vec<String> = DEFAULT_TRACKERS.iter().map(|t| (*t).to_owned()).collect();
        let fetched: Vec<String> =
            sqlx::query_scalar("SELECT url FROM trackers ORDER BY added_at, url")
                .fetch_all(self.store().pool())
                .await?;
        for tracker in fetched {
            if !trackers.contains(&tracker) {
                trackers.push(tracker);
            }
        }
        Ok(trackers)
    }

    /// Downloads the tracker list from the configured URL and stores it.
    /// Returns how many trackers it contained.
    pub async fn refresh_trackers(&self) -> Result<usize> {
        let Some(url) = self.settings().magnets.trackers_url else {
            sqlx::query("DELETE FROM trackers")
                .execute(self.store().pool())
                .await?;
            return Ok(0);
        };
        let response = crate::provider::send(self.http()?.get(url.clone()))
            .await
            .map_err(|source| Error::Provider {
                provider: url.host_str().unwrap_or("tracker list").to_owned(),
                source,
            })?;
        let text = crate::provider::read_text(response)
            .await
            .map_err(|source| Error::Provider {
                provider: "tracker list".into(),
                source,
            })?;
        let trackers = parse_tracker_list(&text);
        let now = now_ms();
        let mut tx = self.store().pool().begin().await?;
        sqlx::query("DELETE FROM trackers")
            .execute(&mut *tx)
            .await?;
        for tracker in &trackers {
            sqlx::query("INSERT OR IGNORE INTO trackers (url, source, added_at) VALUES (?, ?, ?)")
                .bind(tracker)
                .bind(url.as_str())
                .bind(now)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.rebuild().await?;
        Ok(trackers.len())
    }
}

/// Tracker URLs from a plain-text list (one per line, blank lines and `#`
/// comments ignored).
pub fn parse_tracker_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim) {
        let valid = ["udp://", "http://", "https://", "wss://"]
            .iter()
            .any(|scheme| line.starts_with(scheme))
            && url::Url::parse(line).is_ok();
        if valid && !out.iter().any(|t| t == line) {
            out.push(line.to_owned());
            if out.len() == MAX_TRACKERS {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tracker_lists() {
        let list = "udp://a.example:1337/announce\n\n# comment\nhttps://b.example/announce\nnot a url\nudp://a.example:1337/announce\nftp://x\n";
        assert_eq!(
            parse_tracker_list(list),
            [
                "udp://a.example:1337/announce",
                "https://b.example/announce"
            ]
        );
    }
}
