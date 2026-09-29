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
