// SPDX-License-Identifier: GPL-3.0-or-later

//! Hashlark HTTP API. Used in-process by the desktop app and standalone in
//! headless mode. See ADR 0004.

pub mod api;
pub mod auth;
pub mod error;
pub mod torznab;
pub mod ui;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::routing::get;
use axum::{Json, Router, middleware};
use hashlark_core::Engine;
use serde::Serialize;
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa::OpenApi;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub use auth::{AuthConfig, HostPolicy};
pub use ui::UiSource;

/// Base path of every API route.
pub const API_BASE: &str = "/api/v1";

/// Shared state for request handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub engine: Engine,
    pub auth: Arc<AuthConfig>,
    pub host_policy: Arc<HostPolicy>,
    pub failures: Arc<auth::FailureLimiter>,
}

/// How the server is exposed.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub auth: AuthConfig,
    pub host_policy: HostPolicy,
    /// Browser origins allowed to call the API cross-origin (e.g. the
    /// desktop webview). Same-origin callers need no entry.
    pub cors_origins: Vec<HeaderValue>,
    /// Serve the web UI at `/` (headless mode).
    pub web_ui: Option<UiSource>,
}

impl ServerConfig {
    /// Settings for the desktop app: loopback only, random token, and the
    /// origins Tauri's webview uses on each OS.
    pub fn desktop(token: String) -> Self {
        Self {
            auth: AuthConfig {
                token: Some(token),
                api_keys: false,
            },
            host_policy: HostPolicy::Loopback,
            web_ui: None,
            cors_origins: [
                "tauri://localhost",
                "http://tauri.localhost",
                "https://tauri.localhost",
                // Vite dev server used while developing the UI.
                "http://localhost:5173",
            ]
            .into_iter()
            .map(HeaderValue::from_static)
            .collect(),
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Hashlark API",
        description = "Search many torrent indexers at once. All routes except /health and /openapi.json need `Authorization: Bearer <token>`.",
        license(name = "GPL-3.0-or-later", identifier = "GPL-3.0-or-later")
    ),
    servers((url = "/api/v1")),
    modifiers(&BearerScheme),
    tags(
        (name = "search", description = "Searching and resolving downloads"),
        (name = "providers", description = "Managing indexers"),
        (name = "definitions", description = "YAML provider definitions"),
        (name = "repositories", description = "Signed definition repositories"),
        (name = "favorites"),
        (name = "api keys", description = "Keys for the headless server"),
        (name = "settings"),
        (name = "history"),
        (name = "system"),
    )
)]
struct ApiDoc;

struct BearerScheme;

impl utoipa::Modify for BearerScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi
            .components
            .get_or_insert_with(Default::default)
            .add_security_scheme(
                "bearer",
                SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
            );
    }
}

fn protected_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(api::search_stream, api::search_blocking))
        .routes(routes!(api::get_result))
        .routes(routes!(api::resolve))
        .routes(routes!(api::list_providers, api::add_provider))
        .routes(routes!(api::get_definition))
        .routes(routes!(api::check_definition))
        .routes(routes!(api::preview_definition))
        .routes(routes!(api::starter_definition))
        .routes(routes!(api::convert_definition))
        .routes(routes!(api::fetch_page))
        .routes(routes!(api::tor_status))
        .routes(routes!(
            api::get_provider,
            api::update_provider,
            api::delete_provider
        ))
        .routes(routes!(api::test_provider))
        .routes(routes!(api::set_session))
        .routes(routes!(api::get_settings, api::put_settings))
        .routes(routes!(api::get_history, api::clear_history))
        .routes(routes!(api::list_repos, api::add_repo))
        .routes(routes!(api::sync_repo))
        .routes(routes!(api::remove_repo))
        .routes(routes!(api::list_favorites))
        .routes(routes!(api::add_favorite, api::remove_favorite))
        .routes(routes!(api::list_trackers))
        .routes(routes!(api::refresh_trackers))
        .routes(routes!(api::list_api_keys, api::create_api_key))
        .routes(routes!(api::revoke_api_key))
}

/// The OpenAPI document for the API.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (_, mut doc) = protected_routes().routes(routes!(health)).split_for_parts();
    doc.info.version = hashlark_core::VERSION.to_owned();
    doc
}

/// Builds the full router.
pub fn router(engine: Engine, config: ServerConfig) -> Router {
    let state = AppState {
        engine,
        auth: Arc::new(config.auth),
        host_policy: Arc::new(config.host_policy),
        failures: Arc::default(),
    };
    let (protected, _) = protected_routes().split_for_parts();
    let protected = protected.layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_auth,
    ));
    let spec = openapi();

    let api = Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(move || async move { Json(spec) }))
        .merge(protected);

    let mut app = Router::new()
        .nest(API_BASE, api)
        .route("/torznab/api", get(torznab::api))
        .route("/torznab/download/{id}", get(torznab::download));
    if let Some(ui) = config.web_ui {
        app = app.fallback(get(move |uri: axum::http::Uri| async move {
            ui.serve(&uri).await
        }));
    }
    let mut app = app
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::check_host,
        ))
        .with_state(state);
    if !config.cors_origins.is_empty() {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(config.cors_origins))
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::PATCH,
                    Method::DELETE,
                ])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );
    }
    app.layer(tower_http::trace::TraceLayer::new_for_http())
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct Health {
    status: &'static str,
    version: &'static str,
    schema_version: Option<i64>,
}

/// Liveness check. No authentication.
#[utoipa::path(
    get, path = "/health", tag = "system",
    responses((status = 200, body = Health), (status = 503, body = Health))
)]
async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    match state.engine.store().schema_version().await {
        Ok(schema_version) => (
            StatusCode::OK,
            Json(Health {
                status: "ok",
                version: hashlark_core::VERSION,
                schema_version,
            }),
        ),
        Err(err) => {
            tracing::error!(%err, "health check could not reach the database");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    version: hashlark_core::VERSION,
                    schema_version: None,
                }),
            )
        }
    }
}

/// A server running inside the desktop app.
#[derive(Debug)]
pub struct LocalServer {
    pub addr: SocketAddr,
    pub token: String,
    pub handle: tokio::task::JoinHandle<()>,
}

impl LocalServer {
    /// Base URL of the API, e.g. `http://127.0.0.1:52345/api/v1`.
    pub fn base_url(&self) -> String {
        format!("http://{}{API_BASE}", self.addr)
    }
}

/// Starts the API on a random loopback port with a fresh token, for the
/// desktop app.
pub async fn serve_local(engine: Engine) -> std::io::Result<LocalServer> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?;
    let token = AuthConfig::random_token();
    let app = router(engine, ServerConfig::desktop(token.clone()));
    let handle = tokio::spawn(async move {
        let app = app.into_make_service_with_connect_info::<SocketAddr>();
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "local API server stopped");
        }
    });
    Ok(LocalServer {
        addr,
        token,
        handle,
    })
}

#[cfg(test)]
mod tests;
