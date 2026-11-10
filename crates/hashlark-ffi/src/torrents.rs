// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in torrent client for the Android app: a thin layer over
//! `hashlark-torrent` with JSON payloads, like the rest of this crate
//! (ADR 0015, plan `docs/torrent-client-plan.md` §8).
//!
//! The JSON shapes are the serde model of `hashlark_torrent::model`:
//! `TorrentSummary`, `TorrentDetails`, `AddRequest` (`{"source":{"magnet":".."}}`,
//! `{"source":{"torrent":"<base64>"}}`, `{"source":{"result_id":".."}}`),
//! `TorrentPatch`, `BulkRequest` (`{"action":"pause"}`,
//! `{"action":{"remove":{"delete_files":true}}}`), `PeerInfo` and
//! `SessionStats`.
//!
//! Nothing here runs until the app calls [`HashlarkEngine::start_torrents`],
//! so the engine behaves exactly as before for callers that never do.

use std::sync::Arc;

use hashlark_torrent::model::{AddRequest, BulkRequest, PowerState, TorrentPatch};
use hashlark_torrent::{PathPolicy, ServiceOptions, TorrentService};

use crate::{HashlarkEngine, HashlarkError, from_json, run, run_json};

/// Upper bound in bytes for a `.torrent` file the core accepts. The app
/// checks it before reading a `.torrent` intent into memory.
#[uniffi::export]
pub fn max_torrent_bytes() -> u64 {
    hashlark_core::engine::MAX_TORRENT_BYTES
}

impl HashlarkEngine {
    /// The client, or `invalid_request` before [`start_torrents`](Self::start_torrents).
    fn torrent_service(&self) -> Result<TorrentService, HashlarkError> {
        self.torrents.get().cloned().ok_or_else(|| {
            HashlarkError::new("invalid_request", "the torrent client is not started")
        })
    }

    /// Applies saved settings to the client; a no-op until it is started.
    pub(crate) async fn apply_torrent_settings(
        &self,
        saved: &hashlark_core::Settings,
    ) -> Result<(), HashlarkError> {
        let Some(service) = self.torrents.get().cloned() else {
            return Ok(());
        };
        let saved = saved.clone();
        run(async move { service.apply_settings(&saved).await }).await
    }
}

#[uniffi::export]
impl HashlarkEngine {
    /// Creates the client and starts it when `torrents.enabled`. `default_dir`
    /// is where downloads go unless `torrents.download_dir` is set (the
    /// resolved Android folder). Calling it again does nothing.
    pub async fn start_torrents(self: Arc<Self>, default_dir: String) -> Result<(), HashlarkError> {
        let service = self
            .torrents
            .get_or_init(|| {
                let mut options = ServiceOptions::new(self.data_dir.clone(), default_dir);
                options.path_policy = PathPolicy::Unrestricted;
                TorrentService::new(self.engine.clone(), options)
            })
            .clone();
        run(async move { service.start().await }).await
    }

    /// Every torrent: a JSON array of `TorrentSummary`.
    pub async fn torrents(self: Arc<Self>) -> Result<String, HashlarkError> {
        let service = self.torrent_service()?;
        run_json(async move { service.list().await }).await
    }

    /// One torrent with its files and limits: a `TorrentDetails`.
    pub async fn torrent(self: Arc<Self>, id: String) -> Result<String, HashlarkError> {
        let service = self.torrent_service()?;
        run_json(async move { service.details(&id).await }).await
    }

    /// Adds a torrent. `request_json` is an `AddRequest`; returns an `AddResponse`.
    pub async fn add_torrent(
        self: Arc<Self>,
        request_json: String,
    ) -> Result<String, HashlarkError> {
        let request: AddRequest = from_json(&request_json)?;
        let service = self.torrent_service()?;
        run_json(async move { service.add(request).await }).await
    }

    /// Changes a torrent. `patch_json` is a `TorrentPatch`; returns the `TorrentDetails`.
    pub async fn update_torrent(
        self: Arc<Self>,
        id: String,
        patch_json: String,
    ) -> Result<String, HashlarkError> {
        let patch: TorrentPatch = from_json(&patch_json)?;
        let service = self.torrent_service()?;
        run_json(async move { service.update(&id, patch).await }).await
    }

    pub async fn remove_torrent(
        self: Arc<Self>,
        id: String,
        delete_files: bool,
    ) -> Result<(), HashlarkError> {
        let service = self.torrent_service()?;
        run(async move { service.remove(&id, delete_files).await }).await
    }

    /// Connected peers: a JSON array of `PeerInfo`.
    pub async fn torrent_peers(self: Arc<Self>, id: String) -> Result<String, HashlarkError> {
        let service = self.torrent_service()?;
        run_json(async move { service.peers(&id).await }).await
    }

    /// Session-wide numbers: a `SessionStats`.
    pub async fn torrent_stats(self: Arc<Self>) -> Result<String, HashlarkError> {
        let service = self.torrent_service()?;
        run_json(async move { service.stats().await }).await
    }

    /// Acts on many torrents. `request_json` is a `BulkRequest`; returns a `BulkResult`.
    pub async fn torrent_bulk(
        self: Arc<Self>,
        request_json: String,
    ) -> Result<String, HashlarkError> {
        let request: BulkRequest = from_json(&request_json)?;
        let service = self.torrent_service()?;
        run_json(async move { service.bulk(request).await }).await
    }

    /// The Wi-Fi gate: false holds torrents back until the network is allowed
    /// (`unmetered || !unmetered_only`). Ignored before `start_torrents`.
    pub async fn set_network_allowed(self: Arc<Self>, allowed: bool) -> Result<(), HashlarkError> {
        let Some(service) = self.torrents.get().cloned() else {
            return Ok(());
        };
        run(async move {
            service.set_network_allowed(allowed).await;
            Ok(())
        })
        .await
    }

    /// Battery state from the app (power saver, low level, not charging).
    /// Ignored before `start_torrents`.
    pub async fn set_torrent_power(self: Arc<Self>, on_battery: bool) -> Result<(), HashlarkError> {
        let Some(service) = self.torrents.get().cloned() else {
            return Ok(());
        };
        run(async move {
            service.set_power(PowerState { on_battery }).await;
            Ok(())
        })
        .await
    }
}
