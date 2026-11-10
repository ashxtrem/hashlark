// SPDX-License-Identifier: GPL-3.0-or-later

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use hashlark_core::secrets::{MemorySecretStore, SecretStore};
use tokio::sync::mpsc;

use crate::client::Client;
use crate::crypto::{self, Exchange, Role};
use crate::protocol::{Message, MessageKind, SendRequest, b64, send_aad};
use crate::*;

const MAGNET: &str =
    "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=Cosmos+Laundromat";

// ----- crypto -------------------------------------------------------------

fn exchange(pin_a: &str, pin_b: &str) -> (crypto::PairKeys, crypto::PairKeys) {
    let a = Exchange::start(Role::Initiator, pin_a, "aaaa", "bbbb");
    let b = Exchange::start(Role::Responder, pin_b, "aaaa", "bbbb");
    let (msg_a, msg_b) = (a.msg.clone(), b.msg.clone());
    (a.finish(&msg_b).unwrap(), b.finish(&msg_a).unwrap())
}

#[test]
fn same_pin_gives_same_key_and_confirms() {
    let (a, b) = exchange("123456", "123456");
    assert_eq!(a.device_key, b.device_key);
    assert!(b.verify(Role::Initiator, &a.confirmation(Role::Initiator)));
    assert!(a.verify(Role::Responder, &b.confirmation(Role::Responder)));
    // A confirmation can't be reflected back as the other role's.
    assert!(!a.verify(Role::Responder, &a.confirmation(Role::Initiator)));
}

#[test]
fn wrong_pin_gives_different_keys() {
    let (a, b) = exchange("123456", "123457");
    assert_ne!(a.device_key, b.device_key);
    assert!(!b.verify(Role::Initiator, &a.confirmation(Role::Initiator)));
}

#[test]
fn seal_and_open() {
    let key = crypto::random::<32>();
    let (nonce, data) = crypto::seal(&key, b"aad", b"hello");
    assert_eq!(crypto::open(&key, b"aad", &nonce, &data).unwrap(), b"hello");
    assert!(crypto::open(&key, b"other", &nonce, &data).is_none());
    let mut tampered = data.clone();
    tampered[0] ^= 1;
    assert!(crypto::open(&key, b"aad", &nonce, &tampered).is_none());
    assert!(crypto::open(&crypto::random::<32>(), b"aad", &nonce, &data).is_none());
}

#[test]
fn pins_are_six_digits() {
    for _ in 0..1000 {
        let pin = crypto::new_pin();
        assert_eq!(pin.len(), 6);
        assert!(pin.bytes().all(|b| b.is_ascii_digit()));
    }
}

#[test]
fn links_are_validated() {
    assert!(validate_link(MAGNET).is_ok());
    assert!(validate_link("https://example.com/a.torrent").is_err());
    assert!(validate_link("magnet:?xt=urn:btih:00\nx").is_err());
    let long = format!("magnet:?{}", "a".repeat(MAX_LINK_LEN));
    assert!(validate_link(&long).is_err());
}

#[test]
fn names_and_titles_are_cleaned() {
    assert_eq!(clean_name("  Desk\u{7}top  "), "Desktop");
    assert_eq!(clean_name("   "), "Hashlark device");
    assert_eq!(clean_name(&"x".repeat(100)).len(), 64);
    assert_eq!(clean_title("a\nb"), "a b");
}

// ----- two devices on loopback --------------------------------------------

#[derive(Debug)]
enum Event {
    Received(Received),
    Devices,
    Paired(Device),
    Pairing(Option<PairingCode>),
}

struct Recorder(mpsc::UnboundedSender<Event>);

impl LanEvents for Recorder {
    fn received(&self, item: Received) {
        let _ = self.0.send(Event::Received(item));
    }
    fn devices_changed(&self) {
        let _ = self.0.send(Event::Devices);
    }
    fn paired(&self, device: Device) {
        let _ = self.0.send(Event::Paired(device));
    }
    fn pairing_changed(&self, code: Option<PairingCode>) {
        let _ = self.0.send(Event::Pairing(code));
    }
}

struct Node {
    lan: LanService,
    secrets: Arc<MemorySecretStore>,
    events: mpsc::UnboundedReceiver<Event>,
}

impl Node {
    async fn start(name: &str, platform: Platform) -> Self {
        Self::start_with(name, platform, Arc::default()).await
    }

    async fn start_with(name: &str, platform: Platform, secrets: Arc<MemorySecretStore>) -> Self {
        let (tx, events) = mpsc::unbounded_channel();
        let config = LanConfig {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            discovery: false,
            ..LanConfig::new(name, platform)
        };
        let store: Arc<dyn SecretStore> = secrets.clone();
        let lan = LanService::start(config, store, Arc::new(Recorder(tx)))
            .await
            .unwrap();
        Self {
            lan,
            secrets,
            events,
        }
    }

    fn id(&self) -> String {
        self.lan.identity().id
    }

    fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.lan.port())
    }

    /// Waits for the first event `pick` accepts, skipping others.
    async fn wait<T>(&mut self, mut pick: impl FnMut(Event) -> Option<T>) -> T {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let event = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .expect("timed out waiting for an event")
                .expect("event channel closed");
            if let Some(found) = pick(event) {
                return found;
            }
        }
    }

    async fn received(&mut self) -> Received {
        self.wait(|e| match e {
            Event::Received(r) => Some(r),
            _ => None,
        })
        .await
    }
}

/// Phone `a` pairs with desktop `b`.
async fn paired() -> (Node, Node) {
    let phone = Node::start("Pixel", Platform::Android).await;
    let desktop = Node::start("Desk", Platform::Desktop).await;
    let code = desktop.lan.open_pairing();
    let found = phone.lan.add_address(&desktop.addr()).await.unwrap();
    assert_eq!(found.id, desktop.id());
    assert!(!found.paired);
    phone.lan.pair(&desktop.id(), &code.pin).await.unwrap();
    (phone, desktop)
}

#[tokio::test]
async fn pair_then_send_both_ways() {
    let (mut phone, mut desktop) = paired().await;

    let on_desktop = desktop
        .wait(|e| match e {
            Event::Paired(d) => Some(d),
            _ => None,
        })
        .await;
    assert_eq!(on_desktop.id, phone.id());
    assert_eq!(on_desktop.name, "Pixel");
    assert_eq!(on_desktop.platform, Platform::Android);
    assert!(desktop.lan.pairing().is_none(), "pairing closes once used");

    let seen_by_phone = phone.lan.devices();
    assert_eq!(seen_by_phone.len(), 1);
    assert!(seen_by_phone[0].paired);
    // Added by address, not announced over mDNS.
    assert!(!seen_by_phone[0].online);
    assert_eq!(seen_by_phone[0].name, "Desk");

    // Phone → desktop.
    phone
        .lan
        .send_magnet(&desktop.id(), MAGNET, "Cosmos Laundromat")
        .await
        .unwrap();
    let got = desktop.received().await;
    assert_eq!(got.url, MAGNET);
    assert_eq!(got.title, "Cosmos Laundromat");
    assert_eq!(got.from.id, phone.id());

    // Desktop → phone, using the address learned while pairing.
    desktop
        .lan
        .send_magnet(&phone.id(), MAGNET, "Back")
        .await
        .unwrap();
    assert_eq!(phone.received().await.title, "Back");
}

#[tokio::test]
async fn wrong_pins_fail_and_the_pin_rotates() {
    let phone = Node::start("Pixel", Platform::Android).await;
    let mut desktop = Node::start("Desk", Platform::Desktop).await;
    let first = desktop.lan.open_pairing();
    phone.lan.add_address(&desktop.addr()).await.unwrap();
    let wrong = if first.pin == "000000" {
        "111111"
    } else {
        "000000"
    };

    for _ in 0..MAX_FAILURES {
        let err = phone.lan.pair(&desktop.id(), wrong).await.unwrap_err();
        assert!(matches!(err, Error::WrongPin), "{err:?}");
    }
    let rotated = desktop
        .wait(|e| match e {
            Event::Pairing(Some(code)) if code.pin != first.pin => Some(code),
            _ => None,
        })
        .await;
    assert_eq!(desktop.lan.pairing(), Some(rotated.clone()));

    // The old PIN no longer works; the new one does.
    if first.pin != rotated.pin {
        let err = phone.lan.pair(&desktop.id(), &first.pin).await.unwrap_err();
        assert!(matches!(err, Error::WrongPin), "{err:?}");
    }
    phone.lan.pair(&desktop.id(), &rotated.pin).await.unwrap();
}

#[tokio::test]
async fn pairing_needs_an_open_window() {
    let phone = Node::start("Pixel", Platform::Android).await;
    let desktop = Node::start("Desk", Platform::Desktop).await;
    phone.lan.add_address(&desktop.addr()).await.unwrap();
    let err = phone.lan.pair(&desktop.id(), "123456").await.unwrap_err();
    assert!(matches!(err, Error::PairingClosed), "{err:?}");

    desktop.lan.open_pairing();
    desktop.lan.close_pairing();
    let err = phone.lan.pair(&desktop.id(), "123456").await.unwrap_err();
    assert!(matches!(err, Error::PairingClosed), "{err:?}");
}

#[tokio::test]
async fn sending_needs_a_pairing() {
    let phone = Node::start("Pixel", Platform::Android).await;
    let desktop = Node::start("Desk", Platform::Desktop).await;
    phone.lan.add_address(&desktop.addr()).await.unwrap();
    let err = phone
        .lan
        .send_magnet(&desktop.id(), MAGNET, "x")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotPaired), "{err:?}");
}

#[tokio::test]
async fn a_device_forgotten_by_the_receiver_is_rejected() {
    let (phone, desktop) = paired().await;
    desktop.lan.forget(&phone.id()).await.unwrap();
    assert!(desktop.lan.devices().iter().all(|d| d.id != phone.id()));
    let err = phone
        .lan
        .send_magnet(&desktop.id(), MAGNET, "x")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Rejected), "{err:?}");
}

#[tokio::test]
async fn only_magnet_links_are_sent() {
    let (phone, desktop) = paired().await;
    let err = phone
        .lan
        .send_magnet(&desktop.id(), "https://example.com/x.torrent", "x")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
}

/// A send built by hand from the phone's stored key.
fn handmade(phone: &Node, desktop: &Node, sent_at: i64) -> SendRequest {
    let key = phone
        .lan
        .shared()
        .store
        .get(&desktop.id())
        .unwrap()
        .key()
        .unwrap();
    let message = Message {
        kind: MessageKind::Magnet,
        url: MAGNET.into(),
        title: "x".into(),
        sent_at,
        port: phone.lan.port(),
    };
    let (nonce, data) = crypto::seal(
        &key,
        &send_aad(&phone.id(), &desktop.id()),
        &serde_json::to_vec(&message).unwrap(),
    );
    SendRequest {
        from: phone.id(),
        to: desktop.id(),
        nonce: b64(&nonce),
        data: b64(&data),
    }
}

fn desktop_addr(desktop: &Node) -> SocketAddr {
    desktop.addr().parse().unwrap()
}

#[tokio::test]
async fn replays_are_refused() {
    let (phone, mut desktop) = paired().await;
    let request = handmade(&phone, &desktop, now_secs());
    let client = Client::new();
    client.send(desktop_addr(&desktop), &request).await.unwrap();
    desktop.received().await;
    let err = client
        .send(desktop_addr(&desktop), &request)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
}

#[tokio::test]
async fn old_messages_are_refused() {
    let (phone, desktop) = paired().await;
    let request = handmade(&phone, &desktop, now_secs() - 3600);
    let err = Client::new()
        .send(desktop_addr(&desktop), &request)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
}

#[tokio::test]
async fn tampered_and_misaddressed_messages_are_refused() {
    let (phone, desktop) = paired().await;
    let client = Client::new();

    let mut tampered = handmade(&phone, &desktop, now_secs());
    let mut bytes = crate::protocol::unb64(&tampered.data).unwrap();
    bytes[0] ^= 1;
    tampered.data = b64(&bytes);
    let err = client
        .send(desktop_addr(&desktop), &tampered)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Rejected), "{err:?}");

    let mut misaddressed = handmade(&phone, &desktop, now_secs());
    misaddressed.to = phone.id();
    let err = client
        .send(desktop_addr(&desktop), &misaddressed)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
}

#[tokio::test]
async fn pairings_survive_a_restart() {
    let (phone, mut desktop) = paired().await;
    let phone_id = phone.id();
    let secrets = Arc::clone(&phone.secrets);
    phone.lan.shutdown().await;
    drop(phone);

    let phone = Node::start_with("Pixel", Platform::Android, secrets).await;
    assert_eq!(phone.id(), phone_id, "the device id is stable");
    let devices = phone.lan.devices();
    assert_eq!(devices.len(), 1);
    assert!(devices[0].paired);
    assert!(!devices[0].online, "not seen since the restart");

    // The address remembered at pairing time is enough to send.
    phone
        .lan
        .send_magnet(&desktop.id(), MAGNET, "after restart")
        .await
        .unwrap();
    assert_eq!(desktop.received().await.title, "after restart");
}

#[tokio::test]
async fn adding_this_device_or_nonsense_fails() {
    let node = Node::start("Desk", Platform::Desktop).await;
    let err = node.lan.add_address(&node.addr()).await.unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
    let err = node.lan.add_address("not an address").await.unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
}

#[tokio::test]
async fn unreachable_devices_are_reported() {
    let (phone, desktop) = paired().await;
    let name = desktop.lan.identity().name;
    desktop.lan.shutdown().await;
    drop(desktop);
    let err = phone
        .lan
        .send_magnet(&phone.lan.devices()[0].id, MAGNET, "x")
        .await
        .unwrap_err();
    match err {
        Error::Unreachable(who) => assert_eq!(who, name),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn requests_are_rate_limited() {
    let node = Node::start("Desk", Platform::Desktop).await;
    let client = Client::new();
    let addr: SocketAddr = node.addr().parse().unwrap();
    let mut limited = false;
    for _ in 0..200 {
        if let Err(e) = client.hello(addr).await {
            assert!(matches!(e, Error::RateLimited), "{e:?}");
            limited = true;
            break;
        }
    }
    assert!(limited);
}

/// Uses real multicast, which CI runners often block. Run with
/// `cargo test -p hashlark-lan -- --ignored`.
#[tokio::test]
#[ignore = "needs multicast on the host"]
async fn devices_find_each_other_with_mdns() {
    let start = |name: &'static str| async move {
        let (tx, events) = mpsc::unbounded_channel();
        let lan = LanService::start(
            LanConfig {
                port: 0,
                ..LanConfig::new(name, Platform::Desktop)
            },
            Arc::new(MemorySecretStore::default()),
            Arc::new(Recorder(tx)),
        )
        .await
        .unwrap();
        Node {
            lan,
            secrets: Arc::default(),
            events,
        }
    };
    let mut a = start("Alpha").await;
    let b = start("Beta").await;
    assert!(a.lan.discovery_error().is_none());
    let b_id = b.id();
    let found = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            a.wait(|e| matches!(e, Event::Devices).then_some(())).await;
            if let Some(d) = a.lan.devices().into_iter().find(|d| d.id == b_id) {
                return d;
            }
        }
    })
    .await
    .expect("Beta was not discovered");
    assert_eq!(found.name, "Beta");
    assert!(found.online && !found.paired);
}

#[test]
fn local_addresses_are_ipv4_and_not_loopback() {
    for ip in local_addresses() {
        assert!(ip.is_ipv4() && !ip.is_loopback(), "{ip}");
    }
}

/// Forwards connections from a new loopback port to `target`, like a router
/// or `adb forward` does.
async fn forward_to(target: u16) -> u16 {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut inbound, _)) = listener.accept().await {
            tokio::spawn(async move {
                if let Ok(mut outbound) =
                    tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, target)).await
                {
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn typed_addresses_are_used_as_given_behind_port_forwarding() {
    let mut desktop = Node::start("Desk", Platform::Desktop).await;
    let phone = Node::start("Pixel", Platform::Android).await;
    let forwarded = forward_to(phone.lan.port()).await;
    let typed = format!("127.0.0.1:{forwarded}");

    // The desktop reaches the phone only through the forwarded port.
    let found = desktop.lan.add_address(&typed).await.unwrap();
    assert_eq!(found.addr.as_deref(), Some(typed.as_str()));

    let code = phone.lan.open_pairing();
    desktop.lan.pair(&phone.id(), &code.pin).await.unwrap();
    desktop
        .lan
        .send_magnet(&phone.id(), MAGNET, "through the forward")
        .await
        .unwrap();
    let mut phone = phone;
    assert_eq!(phone.received().await.title, "through the forward");

    // After a restart the remembered address is the forwarded one too.
    let secrets = Arc::clone(&desktop.secrets);
    desktop.lan.shutdown().await;
    drop(desktop);
    desktop = Node::start_with("Desk", Platform::Desktop, secrets).await;
    assert_eq!(
        desktop.lan.devices()[0].addr.as_deref(),
        Some(typed.as_str())
    );
    desktop
        .lan
        .send_magnet(&phone.id(), MAGNET, "after restart")
        .await
        .unwrap();
    assert_eq!(phone.received().await.title, "after restart");
    let _ = desktop.events.try_recv();
}
