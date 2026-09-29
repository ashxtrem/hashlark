// SPDX-License-Identifier: GPL-3.0-or-later

//! `/api/v1` handlers.

use std::convert::Infallible;
use std::str::FromStr;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use hashlark_core::aggregator::SearchOutcome;
use hashlark_core::definition::store::StoredDefinition;
use hashlark_core::engine::HistoryEntry;
use hashlark_core::engine::{
    ApiKeyInfo, DefinitionCheck, Favorite, NewApiKey, RepoView, Session, SyncReport,
};
use hashlark_core::model::TargetKind;
use hashlark_core::{
    CancellationToken, Category, DownloadTarget, MergedResult, ProviderPatch, ProviderView,
    SearchQuery, Settings, SortOrder, TestReport,
};
use hashlark_core::{Engine, SearchResult};
use serde::Deserialize;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::error::{ApiError, ApiResult, ErrorBody};

/// Query string of `GET /search`.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    /// Text to search for.
    pub q: Option<String>,
    /// Comma-separated categories, e.g. `movies,tv`.
    pub cat: Option<String>,
    /// Comma-separated provider ids. Default: all enabled providers.
    pub providers: Option<String>,
    /// IMDb id, for providers that support it.
    pub imdb: Option<String>,
    /// 1-based page. Default 1.
    pub page: Option<u32>,
    /// `relevance` (default), `title`, `seeders`, `peers`, `size` or `date`.
    pub sort: Option<String>,
}

fn csv(value: Option<&str>) -> impl Iterator<Item = &str> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

impl SearchParams {
    fn into_query(self) -> ApiResult<SearchQuery> {
        let categories = csv(self.cat.as_deref())
            .map(Category::from_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::bad_request)?;
        let providers: Vec<_> = csv(self.providers.as_deref()).map(Into::into).collect();
        let sort = match self.sort.as_deref() {
            Some(s) => SortOrder::from_str(s).map_err(ApiError::bad_request)?,
            None => SortOrder::default(),
        };
        Ok(SearchQuery {
            text: self.q.unwrap_or_default(),
            categories,
            providers: (!providers.is_empty()).then_some(providers),
            imdb_id: self.imdb.filter(|s| !s.is_empty()),
            page: self.page.unwrap_or(1),
            sort,
        })
    }
}

/// Streams a search as Server-Sent Events.
///
/// Event names: `provider_started`, `results`, `provider_finished`,
/// `provider_failed`, `done`. Each event's data is the JSON of a
/// `SearchEvent`. `results` items with an `id` seen before replace it.
/// Closing the connection cancels the search.
#[utoipa::path(
    get, path = "/search", tag = "search",
    params(SearchParams),
    responses(
        (status = 200, description = "SSE stream of SearchEvent", content_type = "text/event-stream", body = hashlark_core::SearchEvent),
        (status = 400, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn search_stream(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let query = params.into_query()?;
    let rx = state.engine.search(query, CancellationToken::new()).await?;
    let stream = ReceiverStream::new(rx).map(|event| {
        Ok(Event::default()
            .event(event.name())
            .json_data(&event)
            .unwrap_or_else(|_| Event::default().comment("serialization error")))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Runs a search to completion and returns sorted results.
#[utoipa::path(
    post, path = "/search", tag = "search",
    request_body = SearchQuery,
    responses((status = 200, body = SearchOutcome), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn search_blocking(
    State(state): State<AppState>,
    Json(query): Json<SearchQuery>,
) -> ApiResult<Json<SearchOutcome>> {
    Ok(Json(state.engine.search_all(query).await?))
}

/// A result from a recent search.
#[utoipa::path(
    get, path = "/results/{id}", tag = "search",
    params(("id" = String, Path, description = "Result id from a search")),
    responses((status = 200, body = MergedResult), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn get_result(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<MergedResult>> {
    Ok(Json(state.engine.result(&id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveRequest {
    pub result_id: String,
    /// Ask for a magnet or a `.torrent` when the result allows it.
    #[serde(default)]
    pub prefer: Option<TargetKind>,
}

/// Gets the magnet link or `.torrent` URL for a result.
#[utoipa::path(
    post, path = "/resolve", tag = "search",
    request_body = ResolveRequest,
    responses(
        (status = 200, body = DownloadTarget),
        (status = 404, body = ErrorBody),
        (status = 502, description = "The provider failed", body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn resolve(
    State(state): State<AppState>,
    Json(request): Json<ResolveRequest>,
) -> ApiResult<Json<DownloadTarget>> {
    Ok(Json(
        state
            .engine
            .resolve(&request.result_id, request.prefer)
            .await?,
    ))
}

/// Lists every provider with its state and health.
#[utoipa::path(
    get, path = "/providers", tag = "providers",
    responses((status = 200, body = Vec<ProviderView>)),
    security(("bearer" = []))
)]
pub async fn list_providers(State(state): State<AppState>) -> ApiResult<Json<Vec<ProviderView>>> {
    Ok(Json(state.engine.providers().await?))
}

/// One provider.
#[utoipa::path(
    get, path = "/providers/{id}", tag = "providers",
    params(("id" = String, Path)),
    responses((status = 200, body = ProviderView), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn get_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ProviderView>> {
    Ok(Json(state.engine.provider(&id).await?))
}

/// Changes a provider (enable/disable, rename, network policy).
#[utoipa::path(
    patch, path = "/providers/{id}", tag = "providers",
    params(("id" = String, Path)),
    request_body = ProviderPatch,
    responses(
        (status = 200, body = ProviderView),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn update_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<ProviderPatch>,
) -> ApiResult<Json<ProviderView>> {
    Ok(Json(state.engine.update_provider(&id, patch).await?))
}

/// Removes a user-added provider.
#[utoipa::path(
    delete, path = "/providers/{id}", tag = "providers",
    params(("id" = String, Path)),
    responses(
        (status = 204),
        (status = 400, description = "Built-in providers can't be removed", body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.engine.remove_provider(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Runs a provider's self-test now.
#[utoipa::path(
    post, path = "/providers/{id}/test", tag = "providers",
    params(("id" = String, Path)),
    responses((status = 200, body = TestReport), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn test_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<TestReport>> {
    Ok(Json(state.engine.test_provider(&id).await?))
}

/// Current settings.
#[utoipa::path(
    get, path = "/settings", tag = "settings",
    responses((status = 200, body = Settings)),
    security(("bearer" = []))
)]
pub async fn get_settings(State(state): State<AppState>) -> Json<Settings> {
    Json(state.engine.settings())
}

/// Replaces all settings. Fields left out take their defaults.
#[utoipa::path(
    put, path = "/settings", tag = "settings",
    request_body = Settings,
    responses((status = 200, body = Settings), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn put_settings(
    State(state): State<AppState>,
    Json(settings): Json<Settings>,
) -> ApiResult<Json<Settings>> {
    Ok(Json(state.engine.set_settings(settings).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryParams {
    /// Maximum entries (default 50, max 500).
    pub limit: Option<u32>,
}

/// Recent searches, newest first.
#[utoipa::path(
    get, path = "/history", tag = "history",
    params(HistoryParams),
    responses((status = 200, body = Vec<HistoryEntry>)),
    security(("bearer" = []))
)]
pub async fn get_history(
    State(state): State<AppState>,
    Query(params): Query<HistoryParams>,
) -> ApiResult<Json<Vec<HistoryEntry>>> {
    Ok(Json(
        state.engine.history(params.limit.unwrap_or(50)).await?,
    ))
}

/// Deletes all search history.
#[utoipa::path(
    delete, path = "/history", tag = "history",
    responses((status = 204)),
    security(("bearer" = []))
)]
pub async fn clear_history(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.engine.clear_history().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// How to add a provider.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AddProvider {
    /// From a YAML definition.
    Definition { yaml: String },
    /// A Torznab endpoint (Jackett, Prowlarr, Bitmagnet).
    Torznab {
        name: String,
        url: String,
        #[serde(default)]
        api_key: Option<String>,
    },
}

/// Adds a provider. Re-adding a definition with the same id updates it.
#[utoipa::path(
    post, path = "/providers", tag = "providers",
    request_body = AddProvider,
    responses(
        (status = 201, body = ProviderView),
        (status = 400, description = "Invalid definition; `details` lists the problems", body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn add_provider(
    State(state): State<AppState>,
    Json(request): Json<AddProvider>,
) -> ApiResult<(StatusCode, Json<ProviderView>)> {
    let view = match request {
        AddProvider::Definition { yaml } => state.engine.add_definition(&yaml).await?,
        AddProvider::Torznab { name, url, api_key } => {
            state.engine.add_torznab(&name, &url, api_key).await?
        }
    };
    Ok((StatusCode::CREATED, Json(view)))
}

/// Stores cookies from a browser check the user completed on the site
/// (desktop "Open site to continue").
#[utoipa::path(
    post, path = "/providers/{id}/session", tag = "providers",
    params(("id" = String, Path)),
    request_body = Session,
    responses(
        (status = 200, body = ProviderView),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn set_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(session): Json<Session>,
) -> ApiResult<Json<ProviderView>> {
    Ok(Json(state.engine.set_session(&id, session).await?))
}

/// A saved definition's YAML.
#[utoipa::path(
    get, path = "/definitions/{id}", tag = "definitions",
    params(("id" = String, Path)),
    responses((status = 200, body = StoredDefinition), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn get_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<StoredDefinition>> {
    Ok(Json(state.engine.definition(&id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckDefinitionRequest {
    pub yaml: String,
}

/// Validates a definition without saving it.
#[utoipa::path(
    post, path = "/definitions/check", tag = "definitions",
    request_body = CheckDefinitionRequest,
    responses((status = 200, body = DefinitionCheck)),
    security(("bearer" = []))
)]
pub async fn check_definition(
    Json(request): Json<CheckDefinitionRequest>,
) -> Json<DefinitionCheck> {
    Json(Engine::check_definition(&request.yaml))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PreviewDefinitionRequest {
    pub yaml: String,
    pub query: String,
    #[serde(default)]
    pub categories: Vec<Category>,
    /// Setting values (`cfg.*`) to use for this run only.
    #[serde(default)]
    pub settings: std::collections::BTreeMap<String, String>,
}

/// Runs a definition once, live, without saving it.
#[utoipa::path(
    post, path = "/definitions/preview", tag = "definitions",
    request_body = PreviewDefinitionRequest,
    responses(
        (status = 200, body = Vec<SearchResult>),
        (status = 400, body = ErrorBody),
        (status = 502, description = "The site failed", body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn preview_definition(
    State(state): State<AppState>,
    Json(request): Json<PreviewDefinitionRequest>,
) -> ApiResult<Json<Vec<SearchResult>>> {
    let query = SearchQuery {
        categories: request.categories,
        ..SearchQuery::text(request.query)
    };
    Ok(Json(
        state
            .engine
            .preview_definition(&request.yaml, &query, request.settings)
            .await?,
    ))
}

// ----- repositories ----------------------------------------------------------

/// Definition repositories, including the built-in one.
#[utoipa::path(
    get, path = "/repos", tag = "repositories",
    responses((status = 200, body = Vec<RepoView>)),
    security(("bearer" = []))
)]
pub async fn list_repos(State(state): State<AppState>) -> ApiResult<Json<Vec<RepoView>>> {
    Ok(Json(state.engine.repos().await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddRepoRequest {
    /// The repository folder or its `index.json`.
    pub url: String,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct AddRepoResponse {
    pub repo: RepoView,
    pub sync: SyncReport,
}

/// Adds a signed definition repository. Its providers are added disabled.
#[utoipa::path(
    post, path = "/repos", tag = "repositories",
    request_body = AddRepoRequest,
    responses(
        (status = 201, body = AddRepoResponse),
        (status = 400, description = "Invalid, unsigned or duplicate repository", body = ErrorBody),
        (status = 502, description = "Could not fetch it", body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn add_repo(
    State(state): State<AppState>,
    Json(request): Json<AddRepoRequest>,
) -> ApiResult<(StatusCode, Json<AddRepoResponse>)> {
    let (repo, sync) = state.engine.add_repo(&request.url).await?;
    Ok((StatusCode::CREATED, Json(AddRepoResponse { repo, sync })))
}

/// Fetches a repository's latest definitions now.
#[utoipa::path(
    post, path = "/repos/{id}/sync", tag = "repositories",
    params(("id" = String, Path)),
    responses((status = 200, body = SyncReport), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn sync_repo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<SyncReport>> {
    Ok(Json(state.engine.sync_repo(&id).await?))
}

/// Removes a repository and its providers.
#[utoipa::path(
    delete, path = "/repos/{id}", tag = "repositories",
    params(("id" = String, Path)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn remove_repo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.engine.remove_repo(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- favourites --------------------------------------------------------------

/// Saved results, newest first.
#[utoipa::path(
    get, path = "/favorites", tag = "favorites",
    responses((status = 200, body = Vec<Favorite>)),
    security(("bearer" = []))
)]
pub async fn list_favorites(State(state): State<AppState>) -> ApiResult<Json<Vec<Favorite>>> {
    Ok(Json(state.engine.favorites().await?))
}

/// Saves a result from a recent search.
#[utoipa::path(
    put, path = "/favorites/{id}", tag = "favorites",
    params(("id" = String, Path, description = "Result id")),
    responses((status = 200, body = Favorite), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn add_favorite(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Favorite>> {
    Ok(Json(state.engine.add_favorite(&id).await?))
}

#[utoipa::path(
    delete, path = "/favorites/{id}", tag = "favorites",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn remove_favorite(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.engine.remove_favorite(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- trackers ------------------------------------------------------------------

/// Trackers added to magnets.
#[utoipa::path(
    get, path = "/trackers", tag = "settings",
    responses((status = 200, body = Vec<String>)),
    security(("bearer" = []))
)]
pub async fn list_trackers(State(state): State<AppState>) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(state.engine.trackers().await?))
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct RefreshTrackersResponse {
    /// Trackers fetched from the list URL.
    pub fetched: usize,
}

/// Downloads the tracker list from the configured URL now.
#[utoipa::path(
    post, path = "/trackers/refresh", tag = "settings",
    responses((status = 200, body = RefreshTrackersResponse), (status = 502, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn refresh_trackers(
    State(state): State<AppState>,
) -> ApiResult<Json<RefreshTrackersResponse>> {
    Ok(Json(RefreshTrackersResponse {
        fetched: state.engine.refresh_trackers().await?,
    }))
}

// ----- API keys --------------------------------------------------------------------

/// API keys that can sign in to this server.
#[utoipa::path(
    get, path = "/api-keys", tag = "api keys",
    responses((status = 200, body = Vec<ApiKeyInfo>)),
    security(("bearer" = []))
)]
pub async fn list_api_keys(State(state): State<AppState>) -> ApiResult<Json<Vec<ApiKeyInfo>>> {
    Ok(Json(state.engine.api_keys().await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    /// What the key is for, e.g. "Phone" or "Sonarr".
    pub name: String,
}

/// Creates an API key. The key is returned only in this response.
#[utoipa::path(
    post, path = "/api-keys", tag = "api keys",
    request_body = CreateApiKeyRequest,
    responses((status = 201, body = NewApiKey), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn create_api_key(
    State(state): State<AppState>,
    Json(request): Json<CreateApiKeyRequest>,
) -> ApiResult<(StatusCode, Json<NewApiKey>)> {
    Ok((
        StatusCode::CREATED,
        Json(state.engine.create_api_key(&request.name).await?),
    ))
}

/// Revokes an API key.
#[utoipa::path(
    delete, path = "/api-keys/{id}", tag = "api keys",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.engine.revoke_api_key(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- definition tools --------------------------------------------------------------

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StarterParams {
    /// `html` (default), `json` or `rss`.
    pub kind: Option<String>,
    /// Id for the new definition.
    pub id: Option<String>,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct YamlText {
    pub yaml: String,
}

/// A commented starter definition.
#[utoipa::path(
    get, path = "/definitions/starter", tag = "definitions",
    params(StarterParams),
    responses((status = 200, body = YamlText), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn starter_definition(Query(params): Query<StarterParams>) -> ApiResult<Json<YamlText>> {
    use hashlark_core::definition::templates::{StarterKind, starter};
    let kind = match params.kind.as_deref().unwrap_or("html") {
        "html" => StarterKind::Html,
        "json" => StarterKind::Json,
        "rss" => StarterKind::Rss,
        other => {
            return Err(ApiError::bad_request(format!(
                "unknown starter kind `{other}`"
            )));
        }
    };
    Ok(Json(YamlText {
        yaml: starter(kind, params.id.as_deref().unwrap_or("my-site")),
    }))
}

/// Converts a Jackett (Cardigann) definition. Nothing is saved.
#[utoipa::path(
    post, path = "/definitions/convert", tag = "definitions",
    request_body = CheckDefinitionRequest,
    responses((status = 200, body = hashlark_core::definition::cardigann::Conversion), (status = 400, body = ErrorBody)),
    security(("bearer" = []))
)]
pub async fn convert_definition(
    Json(request): Json<CheckDefinitionRequest>,
) -> ApiResult<Json<hashlark_core::definition::cardigann::Conversion>> {
    hashlark_core::definition::cardigann::convert(&request.yaml)
        .map(Json)
        .map_err(|e| hashlark_core::Error::Definition(e).into())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FetchPageRequest {
    pub url: String,
}

/// Fetches a page for the definition editor's element picker.
#[utoipa::path(
    post, path = "/tools/fetch-page", tag = "definitions",
    request_body = FetchPageRequest,
    responses(
        (status = 200, body = hashlark_core::engine::FetchedPage),
        (status = 400, body = ErrorBody),
        (status = 502, body = ErrorBody),
    ),
    security(("bearer" = []))
)]
pub async fn fetch_page(
    State(state): State<AppState>,
    Json(request): Json<FetchPageRequest>,
) -> ApiResult<Json<hashlark_core::engine::FetchedPage>> {
    Ok(Json(state.engine.fetch_page(&request.url).await?))
}

/// State of built-in Tor (connecting, ready, or not running).
#[utoipa::path(
    get, path = "/network/tor", tag = "settings",
    responses((status = 200, body = hashlark_core::net::TorStatus)),
    security(("bearer" = []))
)]
pub async fn tor_status(State(state): State<AppState>) -> Json<hashlark_core::net::TorStatus> {
    Json(state.engine.tor_status().await)
}
