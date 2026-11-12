// SPDX-License-Identifier: GPL-3.0-or-later

//! [`TorrentService`]: the librqbit session's lifecycle, the 1 s tick and
//! the event broadcast (plan §3.1).
//!
//! State is split three ways:
//! - librqbit's JSON persistence and fastresume (`<data>/torrents/session`)
//!   hold the torrents and their pieces;
//! - the `torrents` table holds what librqbit doesn't (run mode, queue,
//!   limits, cumulative totals), cached in [`State::rows`];
//! - `<data>/torrents/magnets/<hash>.magnet` keeps the magnet of a torrent
//!   added by magnet, so a fetch of its metadata can be resumed after a
//!   restart and its `x.pe` peers reused.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use hashlark_core::settings::{Settings, SpeedLimits, TorrentSettings};
use hashlark_core::{Engine, Error, Result};
use librqbit::api::TorrentIdOrHash;
use librqbit::dht::DhtPersistenceConfig;
use librqbit::limits::LimitsConfig;
use librqbit::{
    ConnectionOptions, DhtSessionConfig, ListenerMode, ListenerOptions, ManagedTorrent, Session,
    SessionOptions, SessionPersistenceConfig,
};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::clock::{Clock, system_clock};
use crate::model::{RunMode, ScheduleMode, TorrentEvent};
use crate::paths::PathPolicy;
use crate::store::{self, Row};

mod apply;
mod hook;
mod readd;
mod tick;

pub(crate) use apply::is_active;
pub(crate) use hook::{Desired, TorrentFacts, desired_states, peer_share};
pub(crate) use readd::ReaddOpts;
pub(crate) use tick::{Counter, live_counters};

/// How often cumulative totals are written to SQLite.
const FLUSH_EVERY: Duration = Duration::from_secs(30);
/// Ports picked when `listen_port` is 0.
const RANDOM_PORTS: std::ops::RangeInclusive<u16> = 49152..=65534;

/// Options for [`TorrentService::new`].
#[derive(Debug, Clone)]
pub struct ServiceOptions {
    /// App data folder; the client keeps its state in `<data_dir>/torrents`.
    pub data_dir: PathBuf,
    /// Used when `torrents.download_dir` is not set.
    pub default_download_dir: PathBuf,
    pub path_policy: PathPolicy,
    /// Listen on this port instead of `torrents.listen_port` (the server
    /// uses 6881). `Some(0)` lets the OS pick one (tests).
    pub listen_port_override: Option<u16>,
    /// Address to listen on; `0.0.0.0` by default.
    pub listen_ip: IpAddr,
    /// How often the watched folder is polled (5 s).
    pub watch_interval: Duration,
}

impl ServiceOptions {
    pub fn new(data_dir: impl Into<PathBuf>, default_download_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            default_download_dir: default_download_dir.into(),
            path_policy: PathPolicy::Unrestricted,
            listen_port_override: None,
            listen_ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            watch_interval: Duration::from_secs(5),
        }
    }

    fn root(&self) -> PathBuf {
        self.data_dir.join("torrents")
    }

    pub(crate) fn persistence_dir(&self) -> PathBuf {
        self.root().join("session")
    }

    pub(crate) fn created_file(&self, hash: &str) -> PathBuf {
        self.root().join("created").join(format!("{hash}.torrent"))
    }

    pub(crate) fn magnet_file(&self, hash: &str) -> PathBuf {
        self.root().join("magnets").join(format!("{hash}.magnet"))
    }
}

/// The built-in torrent client. Cheap to clone.
#[derive(Clone)]
pub struct TorrentService {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub engine: Engine,
    pub opts: ServiceOptions,
    pub running: RwLock<Option<Arc<Running>>>,
    /// Serializes start / stop / apply_settings.
    lifecycle: tokio::sync::Mutex<()>,
    pub events: broadcast::Sender<TorrentEvent>,
    pub ctx: Mutex<Ctx>,
    /// Where the policies get the time from.
    pub clock: Mutex<Clock>,
}

/// Device state reported from outside.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    pub on_battery: bool,
    pub network_allowed: bool,
}

/// A started librqbit session and what goes with it.
pub(crate) struct Running {
    pub session: Arc<Session>,
    pub cancel: CancellationToken,
    tick: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// The settings the session was started with (or last applied).
    pub settings: Mutex<TorrentSettings>,
    pub state: Mutex<State>,
    /// Held by the tick and by every operation that changes torrents.
    pub op_lock: tokio::sync::Mutex<()>,
}

/// In-memory state of a running client. Never held across an `.await`.
#[derive(Default)]
pub(crate) struct State {
    /// Rows of the `torrents` table by info hash.
    pub rows: BTreeMap<String, Row>,
    pub counters: HashMap<String, Counter>,
    /// Magnets whose metadata is being fetched.
    pub pending: HashMap<String, CancellationToken>,
    /// Our own errors (metadata fetch failed, data missing).
    pub errors: HashMap<String, String>,
    /// Torrents with an error event already sent.
    reported: HashSet<String>,
    /// Torrents seen incomplete in this run (for `Completed` events).
    pub incomplete: HashSet<String>,
    /// The policy's last decision per torrent.
    pub desired: HashMap<String, Desired>,
    /// The schedule mode of the last policy run.
    pub schedule: ScheduleMode,
    /// The session speed limits last applied.
    pub applied_limits: Option<SpeedLimits>,
    last_flush: Option<Instant>,
}

/// KiB/s limits as librqbit byte rates.
pub(crate) fn limits_config(limits: SpeedLimits) -> LimitsConfig {
    let bps = |kib: Option<u32>| kib.and_then(|k| NonZeroU32::new(k.saturating_mul(1024)));
    LimitsConfig {
        upload_bps: bps(limits.up_kib),
        download_bps: bps(limits.down_kib),
    }
}

/// Validates an id and turns it into a librqbit key.
pub(crate) fn torrent_key(id: &str) -> Result<TorrentIdOrHash> {
    if id.len() != 40 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::NotFound(format!("torrent `{id}`")));
    }
    TorrentIdOrHash::parse(&id.to_ascii_lowercase())
        .map_err(|_| Error::NotFound(format!("torrent `{id}`")))
}

pub(crate) fn invalid(context: &str, e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("{context}: {e:#}"))
}

/// Peers listed as `x.pe` in a magnet.
pub(crate) fn magnet_peers(magnet: &str) -> Vec<SocketAddr> {
    url::Url::parse(magnet)
        .map(|u| {
            u.query_pairs()
                .filter(|(k, _)| k == "x.pe")
                .filter_map(|(_, v)| v.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn random_port() -> u16 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    let span = u64::from(RANDOM_PORTS.end() - RANDOM_PORTS.start()) + 1;
    RANDOM_PORTS.start() + u16::try_from(h.finish() % span).unwrap_or(0)
}

/// Settings whose change needs a new session.
fn needs_restart(old: &TorrentSettings, new: &TorrentSettings) -> bool {
    old.listen_port != new.listen_port
        || old.dht != new.dht
        || old.lsd != new.lsd
        || old.upnp != new.upnp
        || old.utp != new.utp
        || old.proxy != new.proxy
        || old.blocklist_url != new.blocklist_url
        || old.download_dir != new.download_dir
}

impl std::fmt::Debug for TorrentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let running = self.inner.running.read().expect("lock").clone();
        let torrents = running
            .as_ref()
            .map(|r| r.state.lock().expect("lock").rows.len());
        f.debug_struct("TorrentService")
            .field("running", &running.is_some())
            .field("torrents", &torrents)
            .field("data_dir", &self.inner.opts.data_dir)
            .finish()
    }
}

impl TorrentService {
    /// Creates a stopped service; [`start`](Self::start) builds the session.
    pub fn new(engine: Engine, opts: ServiceOptions) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(Inner {
                engine,
                opts,
                running: RwLock::new(None),
                lifecycle: tokio::sync::Mutex::new(()),
                events,
                ctx: Mutex::new(Ctx {
                    on_battery: false,
                    network_allowed: true,
                }),
                clock: Mutex::new(system_clock()),
            }),
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.inner.engine
    }

    pub fn path_policy(&self) -> &PathPolicy {
        &self.inner.opts.path_policy
    }

    /// True while the session runs.
    pub fn is_running(&self) -> bool {
        self.inner.running.read().expect("lock").is_some()
    }

    /// Live updates: a `Snapshot` every second, plus `Completed` and `Error`.
    pub fn subscribe(&self) -> broadcast::Receiver<TorrentEvent> {
        self.inner.events.subscribe()
    }

    pub(crate) fn running(&self) -> Option<Arc<Running>> {
        self.inner.running.read().expect("lock").clone()
    }

    /// The running session, or `Invalid` when the client is off.
    pub(crate) fn require(&self) -> Result<Arc<Running>> {
        self.running()
            .ok_or_else(|| Error::Invalid("the built-in client is off".into()))
    }

    /// Replaces the clock the policies read (the schedule and the seed
    /// minutes). For tests; the default is the system clock.
    pub fn set_clock(&self, clock: Clock) {
        *self.inner.clock.lock().expect("lock") = clock;
    }

    /// The download folder new torrents go to.
    pub fn download_dir(&self) -> PathBuf {
        self.inner
            .engine
            .settings()
            .torrents
            .download_dir
            .unwrap_or_else(|| self.inner.opts.default_download_dir.clone())
    }

    // ----- lifecycle ------------------------------------------------------------

    /// Starts the session when `torrents.enabled`. Does nothing when it
    /// already runs.
    pub async fn start(&self) -> Result<()> {
        let _l = self.inner.lifecycle.lock().await;
        let settings = self.inner.engine.settings().torrents;
        self.start_locked(settings).await
    }

    /// Stops the session: cancels the tick, writes the totals and stops
    /// librqbit. Torrents stay persisted.
    pub async fn stop(&self) {
        let _l = self.inner.lifecycle.lock().await;
        self.stop_locked().await;
    }

    /// Reacts to saved settings (plan §3.1).
    pub async fn apply_settings(&self, settings: &Settings) -> Result<()> {
        let _l = self.inner.lifecycle.lock().await;
        let new = settings.torrents.clone();
        let Some(run) = self.running() else {
            return if new.enabled {
                self.start_locked(new).await
            } else {
                Ok(())
            };
        };
        if !new.enabled {
            self.stop_locked().await;
            return Ok(());
        }
        let old = run.settings.lock().expect("lock").clone();
        if needs_restart(&old, &new) {
            self.stop_locked().await;
            return self.start_locked(new).await;
        }
        // The reconcile applies the new limits.
        *run.settings.lock().expect("lock") = new;
        let _op = run.op_lock.lock().await;
        self.reconcile_locked(&run).await;
        Ok(())
    }

    async fn start_locked(&self, mut t: TorrentSettings) -> Result<()> {
        if !t.enabled || self.is_running() {
            return Ok(());
        }
        let opts = &self.inner.opts;
        // The configured folder (or the default) must pass the path policy
        // before anything is created: `PUT /settings` could name any path.
        let download_dir = t
            .download_dir
            .clone()
            .unwrap_or_else(|| opts.default_download_dir.clone());
        let download_dir = opts.path_policy.check(&download_dir).inspect_err(|e| {
            tracing::warn!(
                dir = %download_dir.display(),
                error = %e,
                "torrent download folder is not allowed; the client stays off"
            );
        })?;
        let port = match opts.listen_port_override {
            Some(port) => port,
            None if t.listen_port != 0 => t.listen_port,
            None => {
                let port = random_port();
                let mut all = self.inner.engine.settings();
                all.torrents.listen_port = port;
                self.inner.engine.set_settings(all).await?;
                t.listen_port = port;
                port
            }
        };
        std::fs::create_dir_all(&download_dir)?;
        let persistence = opts.persistence_dir();
        std::fs::create_dir_all(&persistence)?;
        for dir in ["created", "magnets"] {
            std::fs::create_dir_all(opts.root().join(dir))?;
        }

        let limits = self.context(&t, self.now()).1;
        let listen_addr = SocketAddr::new(opts.listen_ip, port);
        let session_opts = SessionOptions {
            dht: t.dht.then(|| DhtSessionConfig {
                persistence: Some(DhtPersistenceConfig {
                    config_filename: Some(opts.root().join("dht.json")),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(persistence),
            }),
            listen: Some(ListenerOptions {
                mode: if t.utp {
                    ListenerMode::TcpAndUtp
                } else {
                    ListenerMode::TcpOnly
                },
                listen_addr,
                enable_upnp_port_forwarding: t.upnp,
                ipv4_only: listen_addr.is_ipv4(),
                ..Default::default()
            }),
            connect: Some(ConnectionOptions {
                proxy_url: t.proxy.as_ref().map(ToString::to_string),
                ..Default::default()
            }),
            disable_local_service_discovery: !t.lsd,
            ratelimits: limits_config(limits),
            blocklist_url: t.blocklist_url.as_ref().map(ToString::to_string),
            // Restored torrents get this until they are re-added; explicit
            // adds compute their share from the torrents running then.
            peer_limit: Some(peer_share(
                t.peer_limit_global,
                t.peer_limit_torrent,
                usize::from(t.max_active_downloads) + usize::from(t.max_active_seeds),
            ) as usize),
            ..Default::default()
        };
        let session = Session::new_with_opts(download_dir, session_opts)
            .await
            .map_err(|e| invalid("could not start the torrent client", e))?;

        let run = Arc::new(Running {
            session,
            cancel: CancellationToken::new(),
            tick: Mutex::new(None),
            settings: Mutex::new(t),
            state: Mutex::new(State::default()),
            op_lock: tokio::sync::Mutex::new(()),
        });
        if let Err(e) = self.load(&run).await {
            run.session.stop().await;
            return Err(e);
        }
        *self.inner.running.write().expect("lock") = Some(run.clone());

        let svc = self.clone();
        let tick_run = run.clone();
        let handle = tokio::spawn(async move {
            let mut every = tokio::time::interval(Duration::from_secs(1));
            every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    () = tick_run.cancel.cancelled() => break,
                    _ = every.tick() => svc.tick(&tick_run).await,
                }
            }
        });
        *run.tick.lock().expect("lock") = Some(handle);
        self.spawn_watch(&run);
        tracing::info!(addr = ?run.session.listen_addr(), "torrent client started");
        Ok(())
    }

    async fn stop_locked(&self) {
        let Some(run) = self.inner.running.write().expect("lock").take() else {
            return;
        };
        run.cancel.cancel();
        let tick = run.tick.lock().expect("lock").take();
        if let Some(tick) = tick {
            let _ = tick.await;
        }
        let _op = run.op_lock.lock().await;
        self.observe_counters(&run);
        if let Err(e) = self.flush(&run).await {
            tracing::warn!(error = %e, "could not save torrent totals");
        }
        run.session.stop().await;
        tracing::info!("torrent client stopped");
    }

    /// Loads the rows, adopts torrents only librqbit knows, and re-adds
    /// those with their own limits (librqbit takes them only at add).
    async fn load(&self, run: &Arc<Running>) -> Result<()> {
        let store = self.inner.engine.store();
        let mut rows: BTreeMap<String, Row> = store::list(store)
            .await?
            .into_iter()
            .map(|r| (r.info_hash.clone(), r))
            .collect();
        let handles = self.handles(run);
        let mut next_pos = rows.values().map(|r| r.queue_pos + 1).max().unwrap_or(0);
        for (hash, handle) in &handles {
            if rows.contains_key(hash) {
                continue;
            }
            let row = Row {
                info_hash: hash.clone(),
                name: handle.name().unwrap_or_else(|| hash.clone()),
                save_dir: handle.output_folder().to_path_buf(),
                source: crate::model::Origin::Torrent,
                result_id: None,
                added_at: self.now().ms,
                completed_at: None,
                seeding_since: None,
                run_mode: RunMode::Auto,
                queue_pos: next_pos,
                peer_limit: None,
                seed: Default::default(),
                down_kib: None,
                up_kib: None,
                downloaded_total: 0,
                uploaded_total: 0,
                done: false,
            };
            next_pos += 1;
            store::insert(store, &row).await?;
            rows.insert(hash.clone(), row);
        }
        let mut readd = Vec::new();
        {
            let mut st = run.state.lock().expect("lock");
            for (hash, row) in &rows {
                st.counters.insert(hash.clone(), Counter::from_row(row));
                if handles.contains_key(hash) {
                    if row.has_add_options() {
                        readd.push((hash.clone(), row.clone()));
                    }
                } else if !self.inner.opts.magnet_file(hash).exists() {
                    st.errors.insert(
                        hash.clone(),
                        "the torrent's data is missing; remove it and add it again".into(),
                    );
                }
            }
            st.rows = rows;
            st.last_flush = Some(Instant::now());
        }
        let _op = run.op_lock.lock().await;
        for (hash, row) in readd {
            // librqbit restored it with the session defaults.
            let previous = Row {
                peer_limit: None,
                down_kib: None,
                up_kib: None,
                ..row
            };
            if let Err(e) = self.readd(run, &hash, &previous).await {
                tracing::warn!(%hash, error = %e, "could not apply torrent limits");
            }
        }
        self.reconcile_locked(run).await;
        Ok(())
    }

    /// librqbit's torrents by info hash.
    pub(crate) fn handles(&self, run: &Running) -> HashMap<String, Arc<ManagedTorrent>> {
        run.session.with_torrents(|it| {
            it.map(|(_, h)| (h.info_hash().as_string(), h.clone()))
                .collect()
        })
    }

    // ----- context ---------------------------------------------------------------

    /// Android Wi-Fi gate (plan §3.3).
    pub async fn set_network_allowed(&self, allowed: bool) {
        self.inner.ctx.lock().expect("lock").network_allowed = allowed;
        self.reconcile_now().await;
    }

    /// Battery state from the desktop shell or Android (plan §3.3).
    pub async fn set_power(&self, power: crate::model::PowerState) {
        self.inner.ctx.lock().expect("lock").on_battery = power.on_battery;
        self.reconcile_now().await;
    }

    async fn reconcile_now(&self) {
        if let Some(run) = self.running() {
            let _op = run.op_lock.lock().await;
            self.reconcile_locked(&run).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_pe_peers() {
        let m = "magnet:?xt=urn:btih:1dc689206d6f6ef6021d558d95350ed09004b40c\
                 &x.pe=127.0.0.1:6881&x.pe=bad&x.pe=10.0.0.2:51413";
        let peers = magnet_peers(m);
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].port(), 6881);
    }

    #[test]
    fn port_in_range() {
        for _ in 0..50 {
            assert!(RANDOM_PORTS.contains(&random_port()));
        }
    }

    #[test]
    fn keys() {
        assert!(torrent_key("1dc689206d6f6ef6021d558d95350ed09004b40c").is_ok());
        assert!(matches!(torrent_key("nope"), Err(Error::NotFound(_))));
    }
}
