// SPDX-License-Identifier: GPL-3.0-or-later

//! The FFI layer without Kotlin: the same calls the Android wrapper makes, with
//! a fake secret store and a collecting search listener.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

#[derive(Default)]
struct Secrets(Mutex<BTreeMap<String, String>>);

impl SecretStore for Secrets {
    fn get(&self, key: String) -> Result<Option<String>, HashlarkError> {
        Ok(self.0.lock().unwrap().get(&key).cloned())
    }
    fn set(&self, key: String, value: String) -> Result<(), HashlarkError> {
        self.0.lock().unwrap().insert(key, value);
        Ok(())
    }
    fn delete(&self, key: String) -> Result<(), HashlarkError> {
        self.0.lock().unwrap().remove(&key);
        Ok(())
    }
}

#[derive(Default)]
struct Collector {
    events: Mutex<Vec<Value>>,
    ended: std::sync::atomic::AtomicBool,
}

impl SearchListener for Collector {
    fn on_event(&self, event_json: String) {
        self.events
            .lock()
            .unwrap()
            .push(serde_json::from_str(&event_json).expect("valid event JSON"));
    }

    fn on_end(&self) {
        self.ended.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Collector {
    fn events(&self) -> Vec<Value> {
        self.events.lock().unwrap().clone()
    }

    fn ended(&self) -> bool {
        self.ended.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Waits (events arrive from a background thread) until the stream has ended.
    async fn wait_for_end(&self) -> Vec<Value> {
        for _ in 0..100 {
            if self.ended() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.events()
    }
}

async fn engine() -> (Arc<HashlarkEngine>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let engine = open_engine(
        dir.path().display().to_string(),
        Arc::new(Secrets::default()),
    )
    .await
    .unwrap();
    (engine, dir)
}

fn code(err: &HashlarkError) -> &str {
    let HashlarkError::Failed { code, .. } = err;
    code
}

fn torznab(count: usize) -> String {
    let items: String = (1..=count)
        .map(|i| {
            format!(
                "<item><title>Result {i}</title><link>http://127.0.0.1/{i}.torrent</link>\
                 <torznab:attr name=\"seeders\" value=\"{}\"/>\
                 <torznab:attr name=\"infohash\" value=\"{i:040x}\"/></item>",
                100 - i
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\"?><rss xmlns:torznab=\"http://torznab.com/schemas/2015/feed\">\
         <channel>{items}</channel></rss>"
    )
}

async fn add_mock_provider(engine: &Arc<HashlarkEngine>, name: &str, uri: String) -> String {
    let added: Value = serde_json::from_str(
        &engine
            .clone()
            .add_torznab(name.into(), uri, None)
            .await
            .unwrap(),
    )
    .unwrap();
    added["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn the_engine_opens_and_lists_the_first_party_providers() {
    let (engine, _dir) = engine().await;
    let providers: Value =
        serde_json::from_str(&engine.clone().providers().await.unwrap()).unwrap();
    let ids: Vec<&str> = providers
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"internet-archive"), "{ids:?}");
    assert!(ids.contains(&"linuxtracker"), "{ids:?}");
}

#[tokio::test]
async fn errors_carry_the_http_api_codes() {
    let (engine, _dir) = engine().await;

    let bad_query = engine
        .clone()
        .search("{\"text\":\"  \"}".into(), Arc::new(Collector::default()))
        .await
        .unwrap_err();
    assert_eq!(code(&bad_query), "invalid_request");

    let missing = engine.clone().result("nope".into()).await.unwrap_err();
    assert_eq!(code(&missing), "not_found");

    let not_json = engine.clone().set_settings("{".into()).await.unwrap_err();
    assert_eq!(code(&not_json), "invalid_request");

    let mut settings: Value = serde_json::from_str(&engine.settings().unwrap()).unwrap();
    settings["search"]["provider_timeout_secs"] = 0.into();
    let invalid = engine
        .clone()
        .set_settings(settings.to_string())
        .await
        .unwrap_err();
    assert_eq!(code(&invalid), "invalid_request");
}

#[tokio::test]
async fn invalid_definitions_report_every_problem() {
    let (engine, _dir) = engine().await;
    let err = engine
        .clone()
        .add_definition("id: x".into())
        .await
        .unwrap_err();
    let HashlarkError::Failed { code, details, .. } = err;
    assert_eq!(code, "invalid_definition");
    assert!(!details.is_empty());

    let check: Value = serde_json::from_str(&check_definition("id: x".into()).unwrap()).unwrap();
    assert_eq!(check["ok"], false);
}

#[tokio::test]
async fn settings_round_trip_through_json() {
    let (engine, _dir) = engine().await;
    let mut settings: Value = serde_json::from_str(&engine.settings().unwrap()).unwrap();
    settings["ui"]["theme"] = "dark".into();
    settings["search"]["max_concurrency"] = 4.into();
    let saved: Value = serde_json::from_str(
        &engine
            .clone()
            .set_settings(settings.to_string())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["ui"]["theme"], "dark");
    let reread: Value = serde_json::from_str(&engine.settings().unwrap()).unwrap();
    assert_eq!(reread["search"]["max_concurrency"], 4);
}

#[tokio::test]
async fn a_search_streams_events_to_the_listener_and_resolves_a_magnet() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string(torznab(3)))
        .mount(&server)
        .await;
    let (engine, _dir) = engine().await;
    let id = add_mock_provider(&engine, "Mock", server.uri()).await;

    let collector = Arc::new(Collector::default());
    let query = serde_json::json!({ "text": "x", "providers": [id] }).to_string();
    let _handle = engine
        .clone()
        .search(query, collector.clone())
        .await
        .unwrap();

    let events = collector.wait_for_end().await;
    assert!(collector.ended());
    let names: Vec<&str> = events
        .iter()
        .map(|e| e["event"].as_str().unwrap())
        .collect();
    assert_eq!(names.first(), Some(&"provider_started"), "{names:?}");
    assert_eq!(names.last(), Some(&"done"), "{names:?}");
    let results = events.iter().find(|e| e["event"] == "results").unwrap();
    let items = results["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);

    let first = items[0]["id"].as_str().unwrap().to_owned();
    let target: Value = serde_json::from_str(
        &engine
            .clone()
            .resolve(first, Some("magnet".into()))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(target["type"], "magnet");
    assert!(
        target["url"]
            .as_str()
            .unwrap()
            .starts_with("magnet:?xt=urn:btih:")
    );
}

#[tokio::test]
async fn cancelling_a_search_ends_its_event_stream() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(torznab(1))
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let (engine, _dir) = engine().await;
    let id = add_mock_provider(&engine, "Slow", server.uri()).await;

    let collector = Arc::new(Collector::default());
    let query = serde_json::json!({ "text": "x", "providers": [id] }).to_string();
    let handle = engine
        .clone()
        .search(query, collector.clone())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    handle.cancel();

    let events = collector.wait_for_end().await;
    assert!(
        collector.ended(),
        "the stream should end soon after a cancel: {events:?}"
    );
    // A cancelled search sends no `done`.
    assert!(events.iter().all(|e| e["event"] != "done"), "{events:?}");
}

#[test]
fn small_helpers_behave() {
    assert_eq!(sanitize_file_name("My: Torrent/1".into()), "My_ Torrent_1");
    assert_eq!(sanitize_file_name("...".into()), "download");
    assert!(user_agent().contains("Hashlark"));
    assert_eq!(core_version(), hashlark_core::VERSION);
}

// ----- LAN sharing ---------------------------------------------------------------

#[derive(Default)]
struct LanLog {
    received: Mutex<Vec<Value>>,
    paired: Mutex<Vec<Value>>,
}

impl LanListener for LanLog {
    fn on_received(&self, received_json: String) {
        self.received
            .lock()
            .unwrap()
            .push(serde_json::from_str(&received_json).expect("valid Received JSON"));
    }
    fn on_devices_changed(&self) {}
    fn on_paired(&self, device_json: String) {
        self.paired
            .lock()
            .unwrap()
            .push(serde_json::from_str(&device_json).expect("valid Device JSON"));
    }
    fn on_pairing_changed(&self, code_json: Option<String>) {
        if let Some(json) = code_json {
            let code: Value = serde_json::from_str(&json).expect("valid PairingCode JSON");
            assert_eq!(code["pin"].as_str().unwrap().len(), 6);
        }
    }
}

async fn lan(name: &str) -> (Arc<HashlarkLan>, Arc<LanLog>) {
    let log = Arc::new(LanLog::default());
    let lan = lan::open_test_lan(name, Arc::new(Secrets::default()), log.clone())
        .await
        .unwrap();
    (lan, log)
}

fn code_of(result: Result<impl std::fmt::Debug, HashlarkError>) -> String {
    match result.unwrap_err() {
        HashlarkError::Failed { code, .. } => code,
    }
}

#[tokio::test]
async fn lan_pairs_and_sends_through_the_ffi() {
    const MAGNET: &str = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c";
    let (phone, _) = lan("Pixel").await;
    let (desk, desk_log) = lan("Desk").await;
    let hello: Value = serde_json::from_str(&desk.identity().unwrap()).unwrap();
    let desk_id = hello["id"].as_str().unwrap().to_owned();
    let addr = format!("127.0.0.1:{}", hello["port"]);

    // Not paired yet: sending is refused locally.
    let found: Value =
        serde_json::from_str(&phone.clone().add_address(addr).await.unwrap()).unwrap();
    assert_eq!(found["paired"], false);
    assert_eq!(found["platform"], "android");
    let err = phone
        .clone()
        .send_magnet(desk_id.clone(), MAGNET.into(), "x".into())
        .await;
    assert_eq!(code_of(err), "not_paired");

    // Pairing is closed until the desktop opens it.
    let err = phone.clone().pair(desk_id.clone(), "123456".into()).await;
    assert_eq!(code_of(err), "pairing_closed");

    let code: Value = serde_json::from_str(&desk.open_pairing().unwrap()).unwrap();
    assert!(desk.pairing().unwrap().is_some());
    let pin = code["pin"].as_str().unwrap().to_owned();
    let wrong = if pin == "000000" { "111111" } else { "000000" };
    let err = phone.clone().pair(desk_id.clone(), wrong.into()).await;
    assert_eq!(code_of(err), "wrong_pin");

    let paired: Value =
        serde_json::from_str(&phone.clone().pair(desk_id.clone(), pin).await.unwrap()).unwrap();
    assert_eq!(paired["paired"], true);
    assert_eq!(paired["name"], "Desk");
    let devices: Value = serde_json::from_str(&phone.devices().unwrap()).unwrap();
    assert_eq!(devices.as_array().unwrap().len(), 1);

    phone
        .clone()
        .send_magnet(desk_id, MAGNET.into(), "Big Buck Bunny".into())
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let received = loop {
        if let Some(r) = desk_log.received.lock().unwrap().first().cloned() {
            break r;
        }
        assert!(tokio::time::Instant::now() < deadline, "nothing received");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(received["url"], MAGNET);
    assert_eq!(received["title"], "Big Buck Bunny");
    assert_eq!(received["from"]["name"], "Pixel");
    assert_eq!(desk_log.paired.lock().unwrap().len(), 1);

    phone.shutdown().await.unwrap();
    desk.shutdown().await.unwrap();
}

#[tokio::test]
async fn the_torrent_client_starts_only_when_asked_and_lists_what_is_added() {
    let (engine, dir) = engine().await;

    // Before `start_torrents` saving settings leaves the client alone, even
    // with it enabled in the settings.
    let mut settings: Value = serde_json::from_str(&engine.settings().unwrap()).unwrap();
    assert_eq!(settings["torrents"]["enabled"], true);
    for key in ["dht", "lsd", "upnp", "utp"] {
        settings["torrents"][key] = false.into();
    }
    engine
        .clone()
        .set_settings(settings.to_string())
        .await
        .unwrap();
    let err = engine.clone().torrents().await.unwrap_err();
    assert_eq!(code(&err), "invalid_request");
    engine.clone().set_network_allowed(true).await.unwrap();
    engine.clone().set_torrent_power(false).await.unwrap();

    let downloads = dir.path().join("downloads");
    engine
        .clone()
        .start_torrents(downloads.display().to_string())
        .await
        .unwrap();
    // Starting again does nothing.
    engine
        .clone()
        .start_torrents(downloads.display().to_string())
        .await
        .unwrap();
    let stats: Value =
        serde_json::from_str(&engine.clone().torrent_stats().await.unwrap()).unwrap();
    assert_eq!(stats["running"], true);

    let hash = "1dc689206d6f6ef6021d558d95350ed09004b40c";
    let magnet = format!("magnet:?xt=urn:btih:{hash}&dn=Test");
    let added: Value = serde_json::from_str(
        &engine
            .clone()
            .add_torrent(serde_json::json!({ "source": { "magnet": magnet } }).to_string())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(added["torrent"]["id"], hash);
    assert_eq!(added["already"], false);

    let list: Value = serde_json::from_str(&engine.clone().torrents().await.unwrap()).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], hash);
    let details: Value =
        serde_json::from_str(&engine.clone().torrent(hash.into()).await.unwrap()).unwrap();
    assert_eq!(details["id"], hash);

    let paused = engine
        .clone()
        .torrent_bulk(r#"{"action":"pause"}"#.into())
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&paused).unwrap()["ok"][0],
        hash
    );
    let patched: Value = serde_json::from_str(
        &engine
            .clone()
            .update_torrent(hash.into(), r#"{"action":"resume"}"#.into())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(patched["run_mode"], "auto");

    let bad = engine.clone().add_torrent("{".into()).await.unwrap_err();
    assert_eq!(code(&bad), "invalid_request");

    engine
        .clone()
        .remove_torrent(hash.into(), false)
        .await
        .unwrap();
    let list: Value = serde_json::from_str(&engine.clone().torrents().await.unwrap()).unwrap();
    assert!(list.as_array().unwrap().is_empty());
}

#[test]
fn the_torrent_file_cap_is_exported() {
    assert_eq!(max_torrent_bytes(), 10 * 1024 * 1024);
}
