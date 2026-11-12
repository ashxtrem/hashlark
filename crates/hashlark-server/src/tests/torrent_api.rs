// SPDX-License-Identifier: GPL-3.0-or-later

//! `/api/v1/torrents*`, offline: magnets without trackers, a client with
//! DHT, LSD and UPnP off, and a temp data folder.

use hashlark_torrent::PathPolicy;

use super::*;

const MAGNET: &str = "magnet:?xt=urn:btih:1dc689206d6f6ef6021d558d95350ed09004b40c&dn=test";

/// The API with a running torrent client that may only use
/// `<dir>/downloads`.
async fn torrent_app() -> (Router, tempfile::TempDir) {
    let engine = engine().await;
    let mut settings = engine.settings();
    settings.torrents.dht = false;
    settings.torrents.lsd = false;
    settings.torrents.upnp = false;
    engine.set_settings(settings).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    let torrents = TorrentService::new(
        engine.clone(),
        ServiceOptions {
            path_policy: PathPolicy::Roots(vec![downloads.clone()]),
            listen_port_override: Some(0),
            listen_ip: std::net::Ipv4Addr::LOCALHOST.into(),
            ..ServiceOptions::new(dir.path().join("data"), downloads)
        },
    );
    torrents.start().await.unwrap();
    assert!(torrents.is_running());
    let app = mock_peer(router(
        engine,
        torrents,
        ServerConfig::desktop(TOKEN.into()),
    ));
    (app, dir)
}

fn delete(uri: &str) -> Request<Body> {
    Request::delete(uri)
        .header(header::HOST, "127.0.0.1:8787")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn torrent_routes_need_auth() {
    let app = app().await;
    let one = format!("/api/v1/torrents/{HASH}");
    let peers = format!("{one}/peers");
    let file = format!("{one}/file");
    for (method, uri) in [
        ("GET", "/api/v1/torrents"),
        ("POST", "/api/v1/torrents"),
        ("GET", "/api/v1/torrents/events"),
        ("GET", "/api/v1/torrents/stats"),
        ("POST", "/api/v1/torrents/bulk"),
        ("POST", "/api/v1/torrents/create"),
        ("GET", one.as_str()),
        ("PATCH", one.as_str()),
        ("DELETE", one.as_str()),
        ("GET", peers.as_str()),
        ("GET", file.as_str()),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "127.0.0.1")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(
            call(&app, request).await.0,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
    }
}

#[tokio::test]
async fn adding_a_torrent_while_the_client_is_off_is_invalid() {
    let app = app().await;
    let (status, json) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/torrents",
            json!({ "source": { "magnet": MAGNET } }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "invalid_request");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("client is off")
    );
}

#[tokio::test]
async fn torrent_add_list_pause_delete() {
    let (app, _dir) = torrent_app().await;
    let (status, list) = call_json(&app, get("/api/v1/torrents")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list, json!([]));

    let (status, added) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/torrents",
            json!({ "source": { "magnet": MAGNET } }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["already"], false);
    assert_eq!(added["torrent"]["id"], HASH);

    let (_, list) = call_json(&app, get("/api/v1/torrents")).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], HASH);

    let (status, details) = call_json(
        &app,
        send_json(
            "PATCH",
            &format!("/api/v1/torrents/{HASH}"),
            json!({ "action": "pause" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{details}");
    assert_eq!(details["run_mode"], "paused");

    let (status, stats) = call_json(&app, get("/api/v1/torrents/stats")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stats["running"], true);
    assert_eq!(stats["counts"]["all"], 1);

    let (status, bulk) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/torrents/bulk",
            json!({ "action": "resume" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bulk}");
    assert_eq!(bulk["ok"], json!([HASH]));

    // Its metadata isn't known (nothing to fetch it from): recheck fails
    // per torrent, the request itself succeeds.
    let (status, bulk) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/torrents/bulk",
            json!({ "ids": [HASH], "action": "recheck" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bulk}");
    assert_eq!(bulk["ok"], json!([]));
    assert_eq!(bulk["failed"][0]["id"], HASH);
    let message = bulk["failed"][0]["message"].as_str().unwrap();
    assert!(message.contains("metadata"), "{bulk}");

    let (status, _) = call(&app, delete(&format!("/api/v1/torrents/{HASH}"))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = call_json(&app, get("/api/v1/torrents")).await;
    assert_eq!(list, json!([]));
    let (status, json) = call_json(&app, get(&format!("/api/v1/torrents/{HASH}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["code"], "not_found");
}

#[tokio::test]
async fn torrent_save_dir_outside_the_roots_is_invalid() {
    let (app, dir) = torrent_app().await;
    let outside = dir.path().join("elsewhere");
    let (status, json) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/torrents",
            json!({ "source": { "magnet": MAGNET }, "save_dir": outside }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert_eq!(json["error"]["code"], "invalid_request");
    let (_, list) = call_json(&app, get("/api/v1/torrents")).await;
    assert_eq!(list, json!([]));

    // The settings can't point the client outside the roots either.
    let (_, mut settings) = call_json(&app, get("/api/v1/settings")).await;
    settings["torrents"]["download_dir"] = json!(outside);
    let (status, json) = call_json(&app, send_json("PUT", "/api/v1/settings", settings)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn a_big_torrent_body_reaches_validation_not_413() {
    let (app, _dir) = torrent_app().await;
    // Over axum's 2 MB default, under the route's 14 MiB.
    let junk = "A".repeat(3 * 1024 * 1024);
    let (status, json) = call_json(
        &app,
        send_json(
            "POST",
            "/api/v1/torrents",
            json!({ "source": { "torrent": junk } }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn torrent_events_send_a_snapshot() {
    let (app, _dir) = torrent_app().await;
    let response = app
        .clone()
        .oneshot(get("/api/v1/torrents/events"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut body = response.into_body();
    let mut seen = String::new();
    let read = async {
        while !seen.contains("event: snapshot") {
            let frame = body.frame().await.unwrap().unwrap();
            if let Some(data) = frame.data_ref() {
                seen.push_str(&String::from_utf8_lossy(data));
            }
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), read)
        .await
        .expect("a snapshot within 10 s");
    assert!(seen.contains("\"torrents\""), "{seen}");
}

#[tokio::test]
async fn torrent_file_of_an_unknown_torrent_is_404() {
    let (app, _dir) = torrent_app().await;
    let (status, _) = call(&app, get(&format!("/api/v1/torrents/{HASH}/file"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
