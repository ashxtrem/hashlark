// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use hashlark_core::engine::EngineOptions;
use hashlark_core::provider::{Capabilities, ProviderInfo, ProviderKind};
use hashlark_core::secrets::MemorySecretStore;
use hashlark_core::{
    AppPaths, Engine, ProviderCtx, ProviderError, SearchProvider, SearchQuery, SearchResult, Store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;

const TOKEN: &str = "test-token";
const HASH: &str = "1dc689206d6f6ef6021d558d95350ed09004b40c";

#[derive(Debug)]
struct Fake(ProviderInfo);

#[async_trait]
impl SearchProvider for Fake {
    fn info(&self) -> &ProviderInfo {
        &self.0
    }

    async fn search(
        &self,
        _: &ProviderCtx,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        Ok(vec![SearchResult {
            info_hash: Some(HASH.parse().unwrap()),
            seeders: Some(42),
            ..SearchResult::new(self.0.id.clone(), format!("{} ISO", query.text))
        }])
    }
}

async fn app() -> Router {
    let fake: Arc<dyn SearchProvider> = Arc::new(Fake(ProviderInfo {
        id: "fake".into(),
        name: "Fake".into(),
        kind: ProviderKind::Native,
        description: "test provider".into(),
        categories: vec![],
        capabilities: Capabilities {
            text_search: true,
            imdb_search: false,
            paging: false,
        },
    }));
    let engine = Engine::with_store(
        AppPaths::at(std::env::temp_dir().join("hashlark-server-tests")),
        Store::open_in_memory().await.unwrap(),
        EngineOptions {
            secrets: Arc::new(MemorySecretStore::default()),
            builtins: vec![fake],
            definitions: Vec::new(),
        },
    )
    .await
    .unwrap();
    router(engine, ServerConfig::desktop(TOKEN.into())).layer(
        axum::extract::connect_info::MockConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            9,
        ))),
    )
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri)
        .header(header::HOST, "127.0.0.1:8787")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap()
}

fn send_json(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "127.0.0.1:8787")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn call_json(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let (status, body) = call(app, request).await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn health_needs_no_auth() {
    let app = app().await;
    let request = Request::get("/api/v1/health")
        .header(header::HOST, "localhost:1")
        .body(Body::empty())
        .unwrap();
    let (status, json) = call_json(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["schema_version"], Store::latest_schema_version());
}

#[tokio::test]
async fn rejects_missing_or_wrong_token() {
    let app = app().await;
    let no_token = Request::get("/api/v1/providers")
        .header(header::HOST, "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let (status, json) = call_json(&app, no_token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["error"]["code"], "unauthorized");

    let wrong = Request::get("/api/v1/providers")
        .header(header::HOST, "127.0.0.1")
        .header(header::AUTHORIZATION, "Bearer nope")
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&app, wrong).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_foreign_host_header() {
    let app = app().await;
    let mut request = get("/api/v1/providers");
    request
        .headers_mut()
        .insert(header::HOST, "attacker.example:8787".parse().unwrap());
    let (status, json) = call_json(&app, request).await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    assert_eq!(json["error"]["code"], "bad_host");
}

#[tokio::test]
async fn cors_allows_only_the_webview_origin() {
    let app = app().await;
    let preflight = |origin: &str| {
        Request::builder()
            .method("OPTIONS")
            .uri("/api/v1/providers")
            .header(header::HOST, "127.0.0.1")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .body(Body::empty())
            .unwrap()
    };
    let ok = app
        .clone()
        .oneshot(preflight("http://tauri.localhost"))
        .await
        .unwrap();
    assert_eq!(
        ok.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "http://tauri.localhost"
    );
    let evil = app
        .clone()
        .oneshot(preflight("https://evil.example"))
        .await
        .unwrap();
    assert!(
        evil.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[tokio::test]
async fn sse_search_streams_events() {
    let app = app().await;
    let (status, body) = call(&app, get("/api/v1/search?q=ubuntu&sort=seeders")).await;
    assert_eq!(status, StatusCode::OK);
    let events: Vec<&str> = body
        .lines()
        .filter_map(|l| l.strip_prefix("event: "))
        .collect();
    assert_eq!(
        events,
        ["provider_started", "results", "provider_finished", "done"]
    );
    assert!(body.contains("ubuntu ISO"));
}

#[tokio::test]
async fn search_then_resolve() {
    let app = app().await;
    let (status, outcome) = call_json(
        &app,
        send_json("POST", "/api/v1/search", json!({ "text": "ubuntu" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{outcome}");
    let id = outcome["results"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(outcome["results"][0]["seeders"], 42);

    let (status, result) = call_json(&app, get(&format!("/api/v1/results/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["primary"]["title"], "ubuntu ISO");

    let (status, target) = call_json(
        &app,
        send_json("POST", "/api/v1/resolve", json!({ "result_id": id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(target["type"], "magnet");
    assert!(target["url"].as_str().unwrap().contains(HASH));

    let (status, history) = call_json(&app, get("/api/v1/history")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history[0]["query"]["text"], "ubuntu");
}

#[tokio::test]
async fn validation_errors_are_400() {
    let app = app().await;
    let (status, json) = call_json(&app, get("/api/v1/search?q=x&cat=films")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "invalid_request");

    let (status, _) = call_json(&app, get("/api/v1/search?q=")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call_json(
        &app,
        send_json(
            "PUT",
            "/api/v1/settings",
            json!({ "search": { "max_concurrency": 0 } }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let delete = Request::delete("/api/v1/providers/fake")
        .header(header::HOST, "127.0.0.1")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&app, delete).await.0, StatusCode::BAD_REQUEST);

    let (status, json) = call_json(&app, get("/api/v1/results/missing")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["code"], "not_found");
}

#[tokio::test]
async fn providers_and_settings_round_trip() {
    let app = app().await;
    let (status, providers) = call_json(&app, get("/api/v1/providers")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(providers[0]["id"], "fake");
    assert_eq!(providers[0]["health"]["state"], "unknown");

    let (status, updated) = call_json(
        &app,
        send_json(
            "PATCH",
            "/api/v1/providers/fake",
            json!({ "enabled": false }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["enabled"], false);

    let (status, report) = call_json(
        &app,
        send_json("POST", "/api/v1/providers/fake/test", json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["ok"], true);

    let (_, mut settings) = call_json(&app, get("/api/v1/settings")).await;
    settings["search"]["provider_timeout_secs"] = json!(20);
    let (status, saved) = call_json(&app, send_json("PUT", "/api/v1/settings", settings)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["search"]["provider_timeout_secs"], 20);
}

#[tokio::test]
async fn openapi_lists_routes() {
    let app = app().await;
    let request = Request::get("/api/v1/openapi.json")
        .header(header::HOST, "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let (status, spec) = call_json(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    for path in [
        "/search",
        "/resolve",
        "/providers/{id}",
        "/settings",
        "/health",
    ] {
        assert!(spec["paths"].get(path).is_some(), "missing {path}");
    }
    assert!(spec["components"]["securitySchemes"]["bearer"].is_object());
}

const DEF: &str = r#"
schema: 1
id: api-test
name: API Test
links: [http://127.0.0.1:9/]
search:
  path: /s
  response: html
  rows: tr
  fields:
    title: { selector: a }
    magnet: { selector: a, attr: href }
"#;

#[tokio::test]
async fn definitions_can_be_checked_added_and_read() {
    let app = app().await;

    let (status, check) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/definitions/check",
            json!({ "yaml": "schema: 1
id: x" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(check["ok"], false);

    let (status, err) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/providers",
            json!({ "kind": "definition", "yaml": DEF.replace("rows: tr", "rows: \"[[\"") }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "invalid_definition");
    assert!(
        err["error"]["details"][0]
            .as_str()
            .unwrap()
            .contains("CSS selector")
    );

    let (status, view) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/providers",
            json!({ "kind": "definition", "yaml": DEF }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    assert_eq!(view["id"], "api-test");
    assert_eq!(view["source"]["type"], "definition");
    assert_eq!(view["builtin"], false);

    let (status, stored) = call_json(&app, get("/api/v1/definitions/api-test")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(stored["yaml"].as_str().unwrap().contains("API Test"));

    let delete = Request::delete("/api/v1/providers/api-test")
        .header(header::HOST, "127.0.0.1")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&app, delete).await.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn torznab_endpoint_for_sonarr_and_radarr() {
    let app = app().await;
    let plain = |uri: &str| {
        Request::get(uri)
            .header(header::HOST, "127.0.0.1:8787")
            .body(Body::empty())
            .unwrap()
    };

    let (status, caps) = call(&app, plain("/torznab/api?t=caps")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(caps.contains("<tv-search available=\"yes\""));

    let (status, body) = call(&app, plain("/torznab/api?t=search&q=x&apikey=wrong")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("code=\"100\""));

    let (status, body) = call(
        &app,
        plain(&format!(
            "/torznab/api?t=tvsearch&q=ubuntu&season=1&ep=2&cat=5000&apikey={TOKEN}"
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let doc = roxmltree::Document::parse(&body).unwrap();
    let titles: Vec<&str> = doc
        .descendants()
        .filter(|n| n.has_tag_name("title"))
        .filter_map(|n| n.text())
        .collect();
    assert!(titles.contains(&"ubuntu S01E02 ISO"), "{titles:?}");
    assert!(body.contains("magnet:?xt=urn:btih:1dc689206d6f6ef6021d558d95350ed09004b40c"));
    assert!(body.contains("name=\"seeders\" value=\"42\""));

    let (status, empty) = call(
        &app,
        plain(&format!("/torznab/api?t=search&apikey={TOKEN}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !empty.contains("<item>"),
        "RSS sync without a query gets an empty feed"
    );
}

#[tokio::test]
async fn sessions_and_torznab_providers_via_api() {
    let app = app().await;
    let (status, view) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/providers/fake/session",
            json!({ "url": "https://site.example/", "cookies": [{ "name": "cf_clearance", "value": "x" }], "user_agent": "UA" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{view}");

    let (status, view) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/providers",
            json!({ "kind": "torznab", "name": "Prowlarr", "url": "http://127.0.0.1:9696/1/api", "api_key": "k" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    assert_eq!(view["id"], "torznab-prowlarr");
    assert_eq!(view["source"]["type"], "torznab");
}
