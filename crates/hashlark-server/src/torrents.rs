// SPDX-License-Identifier: GPL-3.0-or-later

//! `/api/v1/torrents*` handlers: the built-in torrent client
//! (plan `docs/torrent-client-plan.md` §5). The engine is
//! [`hashlark_torrent::TorrentService`]; its errors map through
//! [`ApiError`]. A client that is switched off answers `400 invalid`.

use std::convert::Infallible;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_core::Stream;
use hashlark_torrent::model::{
    AddRequest, AddResponse, BulkRequest, BulkResult, CreateRequest, CreatedTorrent, PeerInfo,
    SessionStats, TorrentDetails, TorrentEvent, TorrentPatch, TorrentSummary,
};
use serde::Deserialize;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::IntoParams;

use crate::AppState;
use crate::error::{ApiResult, ErrorBody};

/// Largest body of `POST /torrents`: the base64 of a 10 MiB `.torrent` is
/// about 13.4 MiB, and axum's 2 MB default would answer 413 first.
pub const ADD_BODY_LIMIT: usize = 14 * 1024 * 1024;

/// Query string of `DELETE /torrents/{id}`.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RemoveParams {
    /// Also delete the downloaded files (default false).
    #[serde(default)]
    pub delete_files: bool,
}

/// Every torrent, in queue order.
#[utoipa::path(
    get, path = "/torrents", tag = "torrents",
    responses((status = 200, body = Vec<TorrentSummary>), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn list_torrents(State(state): State<AppState>) -> ApiResult<Json<Vec<TorrentSummary>>> {
    Ok(Json(state.torrents.list().await?))
}

/// Adds a torrent from a magnet link, base64 `.torrent` bytes or a search
/// result id. A known info hash returns the existing torrent with
/// `already: true`. `400 invalid` when the built-in client is off or
/// `save_dir` is outside the allowed folders.
#[utoipa::path(
    post, path = "/torrents", tag = "torrents",
    request_body = AddRequest,
    responses((status = 200, body = AddResponse), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn add_torrent(
    State(state): State<AppState>,
    Json(req): Json<AddRequest>,
) -> ApiResult<Json<AddResponse>> {
    Ok(Json(state.torrents.add(req).await?))
}

/// Streams live updates as Server-Sent Events.
///
/// Event names: `snapshot` (every second: `{ session, torrents }`),
/// `completed` and `error`. Each event's data is the JSON of a
/// `TorrentEvent`.
#[utoipa::path(
    get, path = "/torrents/events", tag = "torrents",
    responses(
        (status = 200, description = "SSE stream of TorrentEvent", content_type = "text/event-stream", body = TorrentEvent),
    ),
    security(("bearer" = []))
)]
pub async fn torrent_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // A receiver that fell behind skips ahead: the next snapshot catches up.
    let stream = BroadcastStream::new(state.torrents.subscribe())
        .filter_map(|event| event.ok())
        .map(|event| {
            Ok(Event::default()
                .event(event_name(&event))
                .json_data(&event)
                .unwrap_or_else(|_| Event::default().comment("serialization error")))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn event_name(event: &TorrentEvent) -> &'static str {
    match event {
        TorrentEvent::Snapshot { .. } => "snapshot",
        TorrentEvent::Completed { .. } => "completed",
        TorrentEvent::Error { .. } => "error",
    }
}

/// Session-wide numbers: speeds, listen port, counts per state.
#[utoipa::path(
    get, path = "/torrents/stats", tag = "torrents",
    responses((status = 200, body = SessionStats), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn torrent_stats(State(state): State<AppState>) -> ApiResult<Json<SessionStats>> {
    Ok(Json(state.torrents.stats().await?))
}

/// Pauses, resumes, stops, starts or removes many torrents (every torrent
/// when `ids` is absent), or switches turtle mode.
#[utoipa::path(
    post, path = "/torrents/bulk", tag = "torrents",
    request_body = BulkRequest,
    responses((status = 200, body = BulkResult), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn bulk_torrents(
    State(state): State<AppState>,
    Json(req): Json<BulkRequest>,
) -> ApiResult<Json<BulkResult>> {
    Ok(Json(state.torrents.bulk(req).await?))
}

/// Creates a torrent from a file or folder and starts seeding it.
#[utoipa::path(
    post, path = "/torrents/create", tag = "torrents",
    request_body = CreateRequest,
    responses((status = 200, body = CreatedTorrent), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn create_torrent(
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> ApiResult<Json<CreatedTorrent>> {
    Ok(Json(state.torrents.create(req).await?))
}

/// One torrent with its files, trackers and limits.
#[utoipa::path(
    get, path = "/torrents/{id}", tag = "torrents",
    params(("id" = String, Path, description = "Info hash, 40 hex characters")),
    responses(
        (status = 200, body = TorrentDetails),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn get_torrent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<TorrentDetails>> {
    Ok(Json(state.torrents.details(&id).await?))
}

/// Changes a torrent: run action, selected files, limits, seeding limits
/// or queue position.
#[utoipa::path(
    patch, path = "/torrents/{id}", tag = "torrents",
    params(("id" = String, Path, description = "Info hash, 40 hex characters")),
    request_body = TorrentPatch,
    responses(
        (status = 200, body = TorrentDetails),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn update_torrent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<TorrentPatch>,
) -> ApiResult<Json<TorrentDetails>> {
    Ok(Json(state.torrents.update(&id, patch).await?))
}

/// Removes a torrent, and its files with `delete_files=true`.
#[utoipa::path(
    delete, path = "/torrents/{id}", tag = "torrents",
    params(
        ("id" = String, Path, description = "Info hash, 40 hex characters"),
        RemoveParams,
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn remove_torrent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<RemoveParams>,
) -> ApiResult<axum::http::StatusCode> {
    state.torrents.remove(&id, params.delete_files).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Connected peers of a torrent.
#[utoipa::path(
    get, path = "/torrents/{id}/peers", tag = "torrents",
    params(("id" = String, Path, description = "Info hash, 40 hex characters")),
    responses(
        (status = 200, body = Vec<PeerInfo>),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn torrent_peers(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<PeerInfo>>> {
    Ok(Json(state.torrents.peers(&id).await?))
}

/// The `.torrent` file of a torrent that has its metadata.
#[utoipa::path(
    get, path = "/torrents/{id}/file", tag = "torrents",
    params(("id" = String, Path, description = "Info hash, 40 hex characters")),
    responses(
        (status = 200, description = "The .torrent file", content_type = "application/x-bittorrent", body = Vec<u8>),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn torrent_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let bytes = state.torrents.torrent_file(&id).await?;
    let disposition = format!("attachment; filename=\"{}.torrent\"", safe_name(&id));
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-bittorrent"),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

/// Keeps only the characters of an info hash, so the header stays valid.
fn safe_name(id: &str) -> String {
    id.chars().filter(char::is_ascii_alphanumeric).collect()
}
