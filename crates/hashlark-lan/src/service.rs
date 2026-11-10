// SPDX-License-Identifier: GPL-3.0-or-later

//! [`LanService`]: the listener, discovery and the sending side together.

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::str::FromStr;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use hashlark_core::secrets::SecretStore;
use tokio::sync::oneshot;

use crate::client::Client;
use crate::crypto::{self, Exchange, NONCE_LEN, Role};
use crate::discovery::Discovery;
use crate::error::{Error, Result};
use crate::pairing::{Pairing, PairingCode};
use crate::protocol::{
    Hello, Message, MessageKind, PROTOCOL_VERSION, PairConfirm, PairStart, SendRequest, b64,
    send_aad, unb64,
};
use crate::store::{PeerStore, StoredPeer};
use crate::{
    DEFAULT_PORT, Device, LanConfig, LanEvents, clean_name, clean_title, now_secs, server,
    validate_link,
};

/// How long a received nonce is remembered (longer than the allowed clock
/// skew, so a replay is always caught).
const REPLAY_MEMORY: Duration = Duration::from_secs(10 * 60);

/// A device seen on the network (by mDNS or an address the user typed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    pub name: String,
    pub platform: crate::Platform,
    pub addr: SocketAddr,
    /// The mDNS instance, `None` for typed addresses.
    pub fullname: Option<String>,
}

type Job = Box<dyn FnOnce(&dyn LanEvents) + Send>;

/// State shared by the listener, discovery and [`LanService`].
pub(crate) struct Shared {
    pub hello: Hello,
    pub store: PeerStore,
    pub pairing: Mutex<Pairing>,
    discovered: Mutex<BTreeMap<String, Seen>>,
    replay: Mutex<HashMap<(String, [u8; NONCE_LEN]), Instant>>,
    pub limiter: DefaultKeyedRateLimiter<IpAddr>,
    pub pair_limiter: DefaultKeyedRateLimiter<IpAddr>,
    /// Events go to one thread, in order, so callbacks (which may call into
    /// Kotlin or the UI) never block the network tasks.
    events: Mutex<mpsc::Sender<Job>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared")
            .field("hello", &self.hello)
            .finish_non_exhaustive()
    }
}

impl Shared {
    fn new(hello: Hello, store: PeerStore, events: Arc<dyn LanEvents>) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("hashlark-lan-events".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    job(events.as_ref());
                }
            })
            .expect("spawn the LAN event thread");
        let per_minute = |n: u32| Quota::per_minute(NonZeroU32::new(n).expect("non-zero"));
        Self {
            hello,
            store,
            pairing: Mutex::default(),
            discovered: Mutex::default(),
            replay: Mutex::default(),
            limiter: RateLimiter::keyed(per_minute(120)),
            pair_limiter: RateLimiter::keyed(per_minute(10)),
            events: Mutex::new(tx),
        }
    }

    /// Runs `job` on the event thread.
    pub fn emit(&self, job: impl FnOnce(&dyn LanEvents) + Send + 'static) {
        let _ = self.events.lock().expect("lock").send(Box::new(job));
    }

    /// Records a device seen on the network; `true` if anything changed.
    pub fn see(&self, id: String, seen: Seen) -> bool {
        let mut map = self.discovered.lock().expect("lock");
        map.insert(id, seen.clone()).as_ref() != Some(&seen)
    }

    /// Forgets the mDNS instance `fullname`; `true` if it was known.
    pub fn unsee(&self, fullname: &str) -> bool {
        let mut map = self.discovered.lock().expect("lock");
        let before = map.len();
        map.retain(|_, s| s.fullname.as_deref() != Some(fullname));
        map.len() != before
    }

    fn compose(id: &str, peer: Option<&StoredPeer>, seen: Option<&Seen>) -> Option<Device> {
        let (name, platform) = match (seen, peer) {
            (Some(s), _) => (s.name.clone(), s.platform),
            (None, Some(p)) => (p.name.clone(), p.platform),
            (None, None) => return None,
        };
        Some(Device {
            id: id.to_owned(),
            name,
            platform,
            paired: peer.is_some(),
            // Only mDNS says a device is still there; a typed address was
            // reachable once, which says nothing about now.
            online: seen.is_some_and(|s| s.fullname.is_some()),
            addr: seen
                .map(|s| s.addr)
                .or_else(|| peer.and_then(|p| p.last_addr))
                .map(|a| a.to_string()),
        })
    }

    pub fn device(&self, id: &str) -> Option<Device> {
        let peer = self.store.get(id);
        let seen = self.discovered.lock().expect("lock").get(id).cloned();
        Self::compose(id, peer.as_ref(), seen.as_ref())
    }

    fn devices(&self) -> Vec<Device> {
        let peers = self.store.all();
        let seen = self.discovered.lock().expect("lock").clone();
        let mut devices: Vec<Device> = peers
            .iter()
            .filter_map(|p| Self::compose(&p.id, Some(p), seen.get(&p.id)))
            .collect();
        devices.extend(
            seen.iter()
                .filter(|(id, _)| self.store.get(id).is_none())
                .filter_map(|(id, s)| Self::compose(id, None, Some(s))),
        );
        devices.sort_by_key(|d| (!d.paired, !d.online, d.name.to_lowercase()));
        devices
    }

    /// Addresses to try for a device: where it was last seen, then where it
    /// was last reached.
    fn candidates(&self, id: &str) -> Vec<SocketAddr> {
        let mut addrs = Vec::new();
        if let Some(seen) = self.discovered.lock().expect("lock").get(id) {
            addrs.push(seen.addr);
        }
        if let Some(addr) = self.store.get(id).and_then(|p| p.last_addr)
            && !addrs.contains(&addr)
        {
            addrs.push(addr);
        }
        addrs
    }

    /// `false` if this (sender, nonce) was already received.
    pub fn replay_check(&self, from: &str, nonce: [u8; NONCE_LEN]) -> bool {
        let mut seen = self.replay.lock().expect("lock");
        seen.retain(|_, at| at.elapsed() < REPLAY_MEMORY);
        seen.insert((from.to_owned(), nonce), Instant::now())
            .is_none()
    }

    /// Stores a pairing on a blocking thread (the keychain may block).
    pub async fn save_peer(self: &Arc<Self>, peer: StoredPeer) -> Result<()> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || this.store.insert(peer))
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
    }

    fn remember_addr(self: &Arc<Self>, id: &str, addr: SocketAddr) {
        if self.store.get(id).and_then(|p| p.last_addr) == Some(addr) {
            return;
        }
        let this = Arc::clone(self);
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = this.store.set_last_addr(&id, addr) {
                tracing::warn!(error = %e, "could not remember a device's address");
            }
        });
    }
}

#[derive(Debug)]
struct Running {
    stop: oneshot::Sender<()>,
    server: tokio::task::JoinHandle<()>,
    discovery: Option<Discovery>,
}

/// Sends links to, and receives links from, paired devices on the LAN.
pub struct LanService {
    shared: Arc<Shared>,
    client: Client,
    running: Mutex<Option<Running>>,
    discovery_error: Option<String>,
}

impl std::fmt::Debug for LanService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanService")
            .field("hello", &self.shared.hello)
            .finish_non_exhaustive()
    }
}

impl LanService {
    /// Loads the pairings, starts listening and (if enabled) advertises this
    /// device and looks for others. Must be called within a Tokio runtime.
    pub async fn start(
        config: LanConfig,
        secrets: Arc<dyn SecretStore>,
        events: Arc<dyn LanEvents>,
    ) -> Result<Self> {
        let store = tokio::task::spawn_blocking(move || PeerStore::load(secrets))
            .await
            .map_err(|e| Error::Storage(e.to_string()))??;

        let listener = match tokio::net::TcpListener::bind((config.bind, config.port)).await {
            Ok(listener) => listener,
            Err(e) if config.port != 0 => {
                tracing::info!(port = config.port, error = %e, "LAN port taken; using another");
                tokio::net::TcpListener::bind((config.bind, 0)).await?
            }
            Err(e) => return Err(e.into()),
        };
        let port = listener.local_addr()?.port();

        let hello = Hello {
            id: store.self_id().to_owned(),
            name: clean_name(&config.name),
            platform: config.platform,
            port,
            v: PROTOCOL_VERSION,
        };
        let shared = Arc::new(Shared::new(hello, store, events));

        let (stop, stopped) = oneshot::channel::<()>();
        let app =
            server::router(Arc::clone(&shared)).into_make_service_with_connect_info::<SocketAddr>();
        let server = tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = stopped.await;
            });
            if let Err(e) = serve.await {
                tracing::error!(error = %e, "LAN listener stopped");
            }
        });

        let (discovery, discovery_error) = if config.discovery {
            match Discovery::start(Arc::clone(&shared)) {
                Ok(d) => (Some(d), None),
                Err(e) => {
                    tracing::warn!(error = %e, "LAN discovery unavailable; addresses can still be typed");
                    (None, Some(e.to_string()))
                }
            }
        } else {
            (None, None)
        };
        tracing::info!(port, discovery = discovery.is_some(), "LAN sharing started");

        Ok(Self {
            shared,
            client: Client::new(),
            running: Mutex::new(Some(Running {
                stop,
                server,
                discovery,
            })),
            discovery_error,
        })
    }

    /// This device as others see it.
    pub fn identity(&self) -> Hello {
        self.shared.hello.clone()
    }

    /// Port the listener is on.
    pub fn port(&self) -> u16 {
        self.shared.hello.port
    }

    /// Why automatic discovery isn't running, if it failed to start.
    pub fn discovery_error(&self) -> Option<&str> {
        self.discovery_error.as_deref()
    }

    /// Paired devices (online or not) and unpaired ones seen on the network.
    pub fn devices(&self) -> Vec<Device> {
        self.shared.devices()
    }

    /// Adds a device by address (`192.168.1.20` or `192.168.1.20:47821`),
    /// for networks where discovery doesn't work.
    pub async fn add_address(&self, text: &str) -> Result<Device> {
        let text = text.trim();
        let addr = SocketAddr::from_str(text)
            .or_else(|_| IpAddr::from_str(text).map(|ip| SocketAddr::new(ip, DEFAULT_PORT)))
            .map_err(|_| {
                Error::Invalid("enter an address like 192.168.1.20 or 192.168.1.20:47821".into())
            })?;
        let hello = self.client.hello(addr).await?;
        if hello.id == self.shared.hello.id {
            return Err(Error::Invalid("that address is this device".into()));
        }
        let seen = Seen {
            name: clean_name(&hello.name),
            platform: hello.platform,
            addr: SocketAddr::new(addr.ip(), hello.port),
            fullname: None,
        };
        if self.shared.see(hello.id.clone(), seen) {
            self.shared.emit(|events| events.devices_changed());
        }
        self.shared
            .device(&hello.id)
            .ok_or_else(|| Error::Protocol("device vanished".into()))
    }

    /// Opens pairing for [`crate::pairing::WINDOW`] with a fresh PIN to show.
    pub fn open_pairing(&self) -> PairingCode {
        let code = self.shared.pairing.lock().expect("lock").open();
        let event = code.clone();
        self.shared
            .emit(move |events| events.pairing_changed(Some(event)));
        code
    }

    pub fn close_pairing(&self) {
        self.shared.pairing.lock().expect("lock").close();
        self.shared.emit(|events| events.pairing_changed(None));
    }

    /// The PIN on screen, if pairing is open.
    pub fn pairing(&self) -> Option<PairingCode> {
        self.shared.pairing.lock().expect("lock").current()
    }

    /// Pairs with `device_id` using the PIN it shows. Pairing is mutual:
    /// afterwards either device can send to the other.
    pub async fn pair(&self, device_id: &str, pin: &str) -> Result<Device> {
        let pin = pin.trim();
        if pin.len() != 6 || !pin.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Invalid("the PIN is 6 digits".into()));
        }
        for addr in self.shared.candidates(device_id) {
            match self.pair_at(addr, device_id, pin).await {
                // Try the next address.
                Err(Error::Unreachable(_)) => {}
                other => return other,
            }
        }
        Err(Error::Unreachable(self.display_name(device_id)))
    }

    async fn pair_at(&self, addr: SocketAddr, device_id: &str, pin: &str) -> Result<Device> {
        let me = &self.shared.hello;
        let exchange = Exchange::start(Role::Initiator, pin, &me.id, device_id);
        let reply = self
            .client
            .pair_start(
                addr,
                &PairStart {
                    from: me.clone(),
                    to: device_id.to_owned(),
                    msg: b64(&exchange.msg),
                },
            )
            .await?;
        if reply.device.id != device_id {
            return Err(Error::Protocol("a different device answered".into()));
        }
        let their_msg =
            unb64(&reply.msg).ok_or_else(|| Error::Protocol("bad pairing message".into()))?;
        let keys = exchange
            .finish(&their_msg)
            .ok_or_else(|| Error::Protocol("bad pairing message".into()))?;
        let confirmed = self
            .client
            .pair_confirm(
                addr,
                &PairConfirm {
                    session: reply.session,
                    mac: b64(&keys.confirmation(Role::Initiator)),
                },
            )
            .await?;
        let mac =
            unb64(&confirmed.mac).ok_or_else(|| Error::Protocol("bad confirmation".into()))?;
        if !keys.verify(Role::Responder, &mac) {
            return Err(Error::Protocol(
                "the other device could not confirm the pairing".into(),
            ));
        }
        let peer = StoredPeer::new(
            device_id.to_owned(),
            clean_name(&reply.device.name),
            reply.device.platform,
            &keys.device_key,
            Some(SocketAddr::new(addr.ip(), reply.device.port)),
        );
        self.shared.save_peer(peer).await?;
        let device = self
            .shared
            .device(device_id)
            .ok_or_else(|| Error::Storage("pairing not saved".into()))?;
        let event = device.clone();
        self.shared.emit(move |events| {
            events.paired(event);
            events.devices_changed();
        });
        Ok(device)
    }

    /// Forgets a pairing on this device.
    pub async fn forget(&self, device_id: &str) -> Result<()> {
        let shared = Arc::clone(&self.shared);
        let id = device_id.to_owned();
        tokio::task::spawn_blocking(move || shared.store.remove(&id))
            .await
            .map_err(|e| Error::Storage(e.to_string()))??;
        self.shared.emit(|events| events.devices_changed());
        Ok(())
    }

    /// Sends a magnet link to a paired device, which opens it in its
    /// torrent client.
    pub async fn send_magnet(&self, device_id: &str, url: &str, title: &str) -> Result<()> {
        validate_link(url)?;
        let peer = self.shared.store.get(device_id).ok_or(Error::NotPaired)?;
        let key = peer.key().ok_or(Error::NotPaired)?;
        let message = Message {
            kind: MessageKind::Magnet,
            url: url.to_owned(),
            title: clean_title(title),
            sent_at: now_secs(),
            port: self.shared.hello.port,
        };
        let plaintext = serde_json::to_vec(&message).expect("serialisable");
        let aad = send_aad(&self.shared.hello.id, device_id);
        for addr in self.shared.candidates(device_id) {
            let (nonce, data) = crypto::seal(&key, &aad, &plaintext);
            let request = SendRequest {
                from: self.shared.hello.id.clone(),
                to: device_id.to_owned(),
                nonce: b64(&nonce),
                data: b64(&data),
            };
            match self.client.send(addr, &request).await {
                Ok(()) => {
                    self.shared.remember_addr(device_id, addr);
                    return Ok(());
                }
                // Try the next address.
                Err(Error::Unreachable(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Err(Error::Unreachable(peer.name))
    }

    #[cfg(test)]
    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    fn display_name(&self, id: &str) -> String {
        self.shared
            .device(id)
            .map_or_else(|| "the device".to_owned(), |d| d.name)
    }

    /// Stops listening and advertising. Idempotent.
    pub async fn shutdown(&self) {
        let running = self.running.lock().expect("lock").take();
        if let Some(running) = running {
            if let Some(discovery) = running.discovery {
                discovery.stop();
            }
            let _ = running.stop.send(());
            let _ = tokio::time::timeout(Duration::from_secs(2), running.server).await;
        }
    }
}

impl Drop for LanService {
    fn drop(&mut self) {
        if let Some(running) = self.running.get_mut().ok().and_then(Option::take) {
            if let Some(discovery) = running.discovery {
                discovery.stop();
            }
            let _ = running.stop.send(());
            running.server.abort();
        }
    }
}
