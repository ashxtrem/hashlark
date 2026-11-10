//! Two librqbit sessions on 127.0.0.1, fully offline: session A creates a
//! torrent from a temp file and seeds it, session B fetches it by magnet with
//! A as its only peer. No DHT, LSD, UPnP or trackers, so this runs in CI.
//!
//! The second test probes what survives a session restart (plan §12, T1 spike).

use std::net::{Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use librqbit::api::TorrentIdOrHash;
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerMode, ListenerOptions,
    ManagedTorrent, Session, SessionOptions, SessionPersistenceConfig, TorrentStatsState,
};

const FILE_LEN: usize = 3 * 1024 * 1024;
const TRANSFER_LIMIT: Duration = Duration::from_secs(60);

/// Deterministic, incompressible-looking bytes (xorshift64).
fn test_bytes(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// Options for an offline session: no DHT, LSD, UPnP or trackers. With
/// `listen`, it accepts TCP peers on an ephemeral 127.0.0.1 port.
fn offline_opts(listen: bool, persistence: Option<PathBuf>) -> SessionOptions {
    SessionOptions {
        dht: None,
        disable_trackers: true,
        disable_local_service_discovery: true,
        ipv4_only: true,
        listen: listen.then(|| ListenerOptions {
            mode: ListenerMode::TcpOnly,
            listen_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            enable_upnp_port_forwarding: false,
            ipv4_only: true,
            ..Default::default()
        }),
        fastresume: persistence.is_some(),
        persistence: persistence.map(|folder| SessionPersistenceConfig::Json {
            folder: Some(folder),
        }),
        ..Default::default()
    }
}

async fn session(dir: &Path, opts: SessionOptions) -> Arc<Session> {
    Session::new_with_opts(dir.to_path_buf(), opts)
        .await
        .expect("session starts")
}

fn write_payload(dir: &Path) -> (PathBuf, Vec<u8>) {
    let data = test_bytes(FILE_LEN);
    let path = dir.join("payload.bin");
    std::fs::write(&path, &data).expect("write payload");
    (path, data)
}

/// Seeds `path` from session `a`; returns the seeding handle and its magnet.
async fn seed(a: &Arc<Session>, path: &Path) -> (Arc<ManagedTorrent>, String) {
    let (created, handle) = a
        .create_and_serve_torrent(
            path,
            CreateTorrentOptions {
                piece_length: Some(256 * 1024),
                ..Default::default()
            },
        )
        .await
        .expect("create and serve");
    handle.wait_until_initialized().await.expect("seeder ready");
    assert!(handle.stats().finished, "seeder has the whole file");
    (handle, created.as_magnet().to_string())
}

async fn fetch(
    b: &Arc<Session>,
    magnet: &str,
    peer: SocketAddr,
    out: &Path,
) -> Arc<ManagedTorrent> {
    b.add_torrent(
        AddTorrent::from_url(magnet),
        Some(AddTorrentOptions {
            initial_peers: Some(vec![peer]),
            output_folder: Some(out.to_str().expect("utf-8 path").to_owned()),
            ..Default::default()
        }),
    )
    .await
    .expect("add magnet")
    .into_handle()
    .expect("a handle, not list-only")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn magnet_transfer_over_loopback() {
    let seed_dir = tempfile::tempdir().unwrap();
    let leech_dir = tempfile::tempdir().unwrap();
    let (src, data) = write_payload(seed_dir.path());

    let a = session(seed_dir.path(), offline_opts(true, None)).await;
    let peer = a.listen_addr().expect("seeder listens");
    assert!(peer.ip().is_loopback() && peer.port() != 0);
    let (seeder, magnet) = seed(&a, &src).await;

    let b = session(leech_dir.path(), offline_opts(false, None)).await;
    let started = Instant::now();
    let leecher = fetch(&b, &magnet, peer, leech_dir.path()).await;
    tokio::time::timeout(TRANSFER_LIMIT, leecher.wait_until_completed())
        .await
        .expect("transfer finishes within 60 s")
        .expect("transfer succeeds");
    let elapsed = started.elapsed();

    let got = std::fs::read(leech_dir.path().join("payload.bin")).expect("downloaded file");
    assert_eq!(got.len(), data.len());
    assert!(got == data, "downloaded bytes match the source");

    let seeder_up = seeder.stats().uploaded_bytes;
    eprintln!("loopback: {FILE_LEN} bytes in {elapsed:?}; seeder uploaded_bytes={seeder_up}");
    assert!(seeder_up >= FILE_LEN as u64, "seeder counted the upload");

    b.stop().await;
    a.stop().await;
}

/// Spike probe (T1): a per-torrent download limit set at add time throttles
/// the transfer, and the per-peer snapshot shows live peers mid-transfer
/// (once both sides are complete they disconnect and the snapshot is empty).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_peer_stats_while_throttled() {
    let seed_dir = tempfile::tempdir().unwrap();
    let leech_dir = tempfile::tempdir().unwrap();
    let (src, _) = write_payload(seed_dir.path());

    let a = session(seed_dir.path(), offline_opts(true, None)).await;
    let peer = a.listen_addr().expect("seeder listens");
    let (_seeder, magnet) = seed(&a, &src).await;

    let b = session(leech_dir.path(), offline_opts(false, None)).await;
    let started = Instant::now();
    let leecher = b
        .add_torrent(
            AddTorrent::from_url(magnet.as_str()),
            Some(AddTorrentOptions {
                initial_peers: Some(vec![peer]),
                output_folder: Some(leech_dir.path().to_str().unwrap().to_owned()),
                ratelimits: LimitsConfig {
                    download_bps: NonZeroU32::new(1024 * 1024),
                    upload_bps: None,
                },
                ..Default::default()
            }),
        )
        .await
        .expect("add magnet")
        .into_handle()
        .expect("a handle");

    // Sample once the first piece has landed (256 KiB pieces, 1 MiB burst):
    // with a 1 MiB/s limit the 3 MiB file is still far from done.
    let stats = tokio::time::timeout(TRANSFER_LIMIT, async {
        loop {
            let stats = leecher.stats();
            if stats.progress_bytes > 0 && leecher.live().is_some() {
                break stats;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("transfer starts");
    assert!(!stats.finished, "the 1 MiB/s limit throttles the transfer");
    // `PeerStatsFilter` is not nameable outside librqbit; `Default` = live peers.
    let snapshot = leecher
        .live()
        .expect("leecher is live")
        .per_peer_stats_snapshot(Default::default());
    for (addr, p) in &snapshot.peers {
        eprintln!(
            "peer {addr}: state={} kind={:?} client={:?} fetched={} uploaded={} progress={}",
            p.state,
            p.conn_kind,
            p.client_name,
            p.counters.fetched_bytes,
            p.counters.uploaded_bytes,
            stats.progress_bytes,
        );
    }
    let seen = snapshot
        .peers
        .get(&peer.to_string())
        .expect("seeder listed");
    assert_eq!(seen.state, "live");
    assert!(seen.counters.fetched_bytes > 0);

    tokio::time::timeout(TRANSFER_LIMIT, leecher.wait_until_completed())
        .await
        .expect("throttled transfer finishes within 60 s")
        .expect("transfer succeeds");
    eprintln!("throttled: {FILE_LEN} bytes in {:?}", started.elapsed());

    b.stop().await;
    a.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uploaded_bytes_after_session_restart() {
    let seed_dir = tempfile::tempdir().unwrap();
    let leech_dir = tempfile::tempdir().unwrap();
    let state_dir = tempfile::tempdir().unwrap();
    let (src, _) = write_payload(seed_dir.path());
    let persist = state_dir.path().to_path_buf();

    let a = session(seed_dir.path(), offline_opts(true, Some(persist.clone()))).await;
    let peer = a.listen_addr().expect("seeder listens");
    let (seeder, magnet) = seed(&a, &src).await;
    let info_hash = seeder.info_hash();

    let b = session(leech_dir.path(), offline_opts(false, None)).await;
    let leecher = fetch(&b, &magnet, peer, leech_dir.path()).await;
    tokio::time::timeout(TRANSFER_LIMIT, leecher.wait_until_completed())
        .await
        .expect("transfer finishes within 60 s")
        .expect("transfer succeeds");
    let before = seeder.stats().uploaded_bytes;
    assert!(before >= FILE_LEN as u64);
    b.stop().await;
    a.stop().await;
    drop((seeder, a));

    // Same JSON persistence folder + fastresume: the torrent comes back...
    let a2 = session(seed_dir.path(), offline_opts(true, Some(persist))).await;
    let restored = a2
        .get(TorrentIdOrHash::Hash(info_hash))
        .expect("torrent restored from persistence");
    restored
        .wait_until_initialized()
        .await
        .expect("restored ready");
    let stats = restored.stats();
    eprintln!(
        "restart: uploaded_bytes before={before} after={} state={} finished={}",
        stats.uploaded_bytes, stats.state, stats.finished
    );
    assert!(stats.finished, "fastresume keeps the pieces");
    assert!(matches!(stats.state, TorrentStatsState::Live));
    // ...but librqbit does not persist the upload counter: it lives in the
    // per-live-state atomics and restarts at 0. The service must keep
    // `uploaded_bytes` itself (the `torrents` table) to compute ratios.
    assert_eq!(stats.uploaded_bytes, 0, "librqbit resets uploaded_bytes");

    a2.stop().await;
}

/// [`TorrentService`] end to end: two services on 127.0.0.1 with DHT, LSD,
/// UPnP and uTP off, so peers only come from the magnet's `x.pe`.
mod service {
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use hashlark_core::model::{SearchQuery, SearchResult};
    use hashlark_core::provider::{Capabilities, ProviderInfo, ProviderKind};
    use hashlark_core::secrets::MemorySecretStore;
    use hashlark_core::settings::{OnBattery, SpeedLimits, TorrentSettings, WeekGrid};
    use hashlark_core::{
        AppPaths, Engine, EngineOptions, Error, ProviderCtx, ProviderError, SearchProvider, Store,
    };
    use hashlark_torrent::model::{
        AddRequest, AddSource, BulkAction, BulkRequest, CreateRequest, Origin, PowerState, RunMode,
        ScheduleMode, SeedLimits, TorrentAction, TorrentEvent, TorrentPatch, TorrentState,
        TorrentSummary,
    };
    use hashlark_torrent::{Clock, Moment, ServiceOptions, TorrentService};
    use tempfile::TempDir;

    use super::{FILE_LEN, TRANSFER_LIMIT, test_bytes};

    struct Node {
        dir: TempDir,
        engine: Engine,
        svc: TorrentService,
    }

    impl Node {
        fn downloads(&self) -> PathBuf {
            self.dir.path().join("downloads")
        }
    }

    /// A node with no seed limit, so the transfer tests keep seeding.
    async fn node(builtins: Vec<Arc<dyn SearchProvider>>) -> Node {
        node_with(builtins, |t| t.seed_ratio = None).await
    }

    async fn node_with(
        builtins: Vec<Arc<dyn SearchProvider>>,
        tweak: impl FnOnce(&mut TorrentSettings),
    ) -> Node {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let store = Store::open_in_memory().await.unwrap();
        let engine = Engine::with_store(
            AppPaths::at(&data),
            store,
            EngineOptions {
                secrets: Arc::new(MemorySecretStore::default()),
                builtins,
                definitions: Vec::new(),
            },
        )
        .await
        .unwrap();
        let mut settings = engine.settings();
        let t = &mut settings.torrents;
        t.dht = false;
        t.lsd = false;
        t.upnp = false;
        t.utp = false;
        tweak(t);
        engine.set_settings(settings).await.unwrap();
        let mut opts = ServiceOptions::new(&data, dir.path().join("downloads"));
        opts.listen_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        opts.listen_port_override = Some(0);
        opts.watch_interval = Duration::from_millis(200);
        let svc = TorrentService::new(engine.clone(), opts);
        svc.start().await.unwrap();
        assert!(svc.is_running());
        Node { dir, engine, svc }
    }

    /// Seeds a fresh payload from `seeder`; returns its id and a magnet
    /// pointing at the seeder.
    async fn seed(seeder: &Node, file: &str, salt: u8) -> (String, String, Vec<u8>) {
        let share = seeder.dir.path().join("share");
        std::fs::create_dir_all(&share).unwrap();
        let mut data = test_bytes(FILE_LEN);
        data[0] = salt;
        let path = share.join(file);
        std::fs::write(&path, &data).unwrap();
        let created = seeder
            .svc
            .create(CreateRequest {
                path,
                trackers: Vec::new(),
                name: None,
                piece_length: Some(256 * 1024),
            })
            .await
            .unwrap();
        assert!(!created.already);
        assert_eq!(created.torrent.source, Origin::Created);
        let id = created.torrent.id.clone();
        wait(&seeder.svc, &id, "seeder ready", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        let port = seeder.svc.stats().await.unwrap().listen_port.unwrap();
        let magnet = format!("{}&x.pe=127.0.0.1:{port}", created.magnet);
        (id, magnet, data)
    }

    async fn wait(
        svc: &TorrentService,
        id: &str,
        what: &str,
        ok: impl Fn(&TorrentSummary) -> bool,
    ) -> TorrentSummary {
        let started = Instant::now();
        loop {
            let list = svc.list().await.unwrap();
            if let Some(t) = list.iter().find(|t| t.id == id)
                && ok(t)
            {
                return t.clone();
            }
            assert!(
                started.elapsed() < TRANSFER_LIMIT,
                "timed out waiting for {what}: {list:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn action(a: TorrentAction) -> TorrentPatch {
        TorrentPatch {
            action: Some(a),
            ..TorrentPatch::default()
        }
    }

    fn base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn file_in(dir: &Path, name: &str) -> PathBuf {
        dir.join(name)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn add_update_and_remove() {
        let a = node(Vec::new()).await;
        let b = node(Vec::new()).await;
        let (id, magnet, data) = seed(&a, "payload.bin", 1).await;
        let mut events = b.svc.subscribe();

        let added = b
            .svc
            .add(AddRequest::new(AddSource::Magnet(magnet.clone())))
            .await
            .unwrap();
        assert!(!added.already);
        assert_eq!(added.torrent.id, id);
        assert_eq!(added.torrent.source, Origin::Magnet);
        let list = b.svc.list().await.unwrap();
        assert_eq!(list.len(), 1);
        wait(&b.svc, &id, "download", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        let path = file_in(&b.downloads(), "payload.bin");
        assert!(std::fs::read(&path).unwrap() == data, "same bytes");
        let (mut snapshot, mut completed) = (false, false);
        let started = Instant::now();
        while !(snapshot && completed) && started.elapsed() < Duration::from_secs(5) {
            match tokio::time::timeout(Duration::from_secs(2), events.recv()).await {
                Ok(Ok(TorrentEvent::Snapshot { torrents, .. })) => {
                    snapshot |= torrents.iter().any(|t| t.id == id);
                }
                Ok(Ok(TorrentEvent::Completed { id: done, .. })) => completed |= done == id,
                _ => {}
            }
        }
        assert!(snapshot, "snapshot events arrive");
        assert!(completed, "a completed event arrives");

        let again = b
            .svc
            .add(AddRequest::new(AddSource::Magnet(magnet.clone())))
            .await
            .unwrap();
        assert!(
            again.already,
            "a duplicate add returns the existing torrent"
        );

        let details = b.svc.details(&id).await.unwrap();
        assert_eq!(details.files.len(), 1);
        assert!(details.files[0].selected);
        assert_eq!(details.files[0].size, FILE_LEN as u64);
        assert_eq!(details.summary.size, FILE_LEN as u64);
        assert!(details.magnet.as_deref().is_some_and(|m| m.contains(&id)));
        assert!(details.trackers.is_empty());

        // Files, limits and the connection limit.
        b.svc
            .update(
                &id,
                TorrentPatch {
                    files: Some(vec![0]),
                    ..TorrentPatch::default()
                },
            )
            .await
            .unwrap();
        let bad_file = TorrentPatch {
            files: Some(vec![7]),
            ..TorrentPatch::default()
        };
        assert!(matches!(
            b.svc.update(&id, bad_file).await,
            Err(Error::Invalid(_))
        ));
        let limits = SpeedLimits {
            down_kib: Some(512),
            up_kib: Some(64),
        };
        let details = b
            .svc
            .update(
                &id,
                TorrentPatch {
                    limits: Some(limits),
                    peer_limit: Some(50),
                    ..TorrentPatch::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(details.limits, limits);
        assert_eq!(details.peer_limit, Some(50));
        let bad_peers = TorrentPatch {
            peer_limit: Some(5),
            ..TorrentPatch::default()
        };
        assert!(matches!(
            b.svc.update(&id, bad_peers).await,
            Err(Error::Invalid(_))
        ));
        // Re-added with the new limits, it comes back complete (fastresume).
        wait(&b.svc, &id, "re-added", |t| {
            t.state == TorrentState::Seeding
        })
        .await;

        // Each action.
        let t = b
            .svc
            .update(&id, action(TorrentAction::Pause))
            .await
            .unwrap();
        assert_eq!(t.summary.run_mode, RunMode::Paused);
        assert_eq!(t.summary.state, TorrentState::Paused);
        let t = b
            .svc
            .update(&id, action(TorrentAction::Resume))
            .await
            .unwrap();
        assert_eq!(t.summary.run_mode, RunMode::Auto);
        wait(&b.svc, &id, "resumed", |t| t.state == TorrentState::Seeding).await;
        let t = b
            .svc
            .update(&id, action(TorrentAction::Stop))
            .await
            .unwrap();
        assert_eq!(t.summary.run_mode, RunMode::Stopped);
        assert_eq!(t.summary.state, TorrentState::Finished);
        let t = b
            .svc
            .update(&id, action(TorrentAction::Start))
            .await
            .unwrap();
        assert_eq!(t.summary.run_mode, RunMode::Auto);
        wait(&b.svc, &id, "started", |t| t.state == TorrentState::Seeding).await;
        let t = b
            .svc
            .update(&id, action(TorrentAction::ForceStart))
            .await
            .unwrap();
        assert_eq!(t.summary.run_mode, RunMode::Forced);
        wait(&b.svc, &id, "forced", |t| t.state == TorrentState::Seeding).await;

        // Totals: B downloaded it, A uploaded it.
        wait(&b.svc, &id, "download total", |t| {
            t.downloaded_total >= FILE_LEN as u64
        })
        .await;
        wait(&a.svc, &id, "upload total", |t| {
            t.uploaded_total >= FILE_LEN as u64
        })
        .await;
        assert!(b.svc.peers(&id).await.is_ok());

        // Remove keeping the files, re-add from the .torrent, remove with files.
        b.svc.remove(&id, false).await.unwrap();
        assert!(b.svc.list().await.unwrap().is_empty());
        assert!(path.exists(), "files kept");
        assert!(matches!(
            b.svc.remove(&id, false).await,
            Err(Error::NotFound(_))
        ));
        let bytes = a.svc.torrent_file(&id).await.unwrap();
        let added = b
            .svc
            .add(AddRequest::new(AddSource::Torrent(base64(&bytes))))
            .await
            .unwrap();
        assert!(!added.already);
        assert_eq!(added.torrent.source, Origin::Torrent);
        wait(&b.svc, &id, "re-check of kept files", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        b.svc.remove(&id, true).await.unwrap();
        assert!(!path.exists(), "files deleted");

        b.svc.stop().await;
        a.svc.stop().await;
    }

    #[derive(Debug)]
    struct Stub(ProviderInfo, String, String);

    #[async_trait]
    impl SearchProvider for Stub {
        fn info(&self) -> &ProviderInfo {
            &self.0
        }

        async fn search(
            &self,
            _: &ProviderCtx,
            _: &SearchQuery,
        ) -> Result<Vec<SearchResult>, ProviderError> {
            Ok(vec![SearchResult {
                info_hash: Some(self.1.parse().unwrap()),
                magnet: Some(self.2.clone()),
                ..SearchResult::new(self.0.id.clone(), "payload")
            }])
        }
    }

    fn stub(hash: &str, magnet: &str) -> Arc<dyn SearchProvider> {
        Arc::new(Stub(
            ProviderInfo {
                id: "stub".into(),
                name: "Stub".into(),
                kind: ProviderKind::Native,
                description: "test".into(),
                categories: vec![],
                capabilities: Capabilities {
                    text_search: true,
                    imdb_search: false,
                    paging: false,
                },
            },
            hash.to_owned(),
            magnet.to_owned(),
        ))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn result_id_and_bulk() {
        let a = node(Vec::new()).await;
        let (one, magnet_one, _) = seed(&a, "one.bin", 1).await;
        let (two, magnet_two, _) = seed(&a, "two.bin", 2).await;
        // B's only search provider returns A's first torrent.
        let b = node(vec![stub(&one, &magnet_one)]).await;
        let outcome = b.engine.search_all(SearchQuery::text("x")).await.unwrap();
        let result_id = outcome.results[0].id.clone();
        let added = b
            .svc
            .add(AddRequest::new(AddSource::ResultId(result_id.clone())))
            .await
            .unwrap();
        assert_eq!(added.torrent.id, one);
        assert_eq!(added.torrent.source, Origin::Result);
        assert_eq!(
            b.svc.details(&one).await.unwrap().result_id,
            Some(result_id)
        );
        b.svc
            .add(AddRequest::new(AddSource::Magnet(magnet_two)))
            .await
            .unwrap();
        for id in [&one, &two] {
            wait(&b.svc, id, "download", |t| t.state == TorrentState::Seeding).await;
        }

        let mode =
            |list: &[TorrentSummary], id: &str| list.iter().find(|t| t.id == id).unwrap().run_mode;
        let r = b
            .svc
            .bulk(BulkRequest {
                ids: Some(vec![one.clone(), "f".repeat(40)]),
                action: BulkAction::Pause,
            })
            .await
            .unwrap();
        assert_eq!(r.ok, vec![one.clone()]);
        assert_eq!(r.failed.len(), 1);
        let list = b.svc.list().await.unwrap();
        assert_eq!(mode(&list, &one), RunMode::Paused);
        assert_eq!(mode(&list, &two), RunMode::Auto);

        for (act, want) in [
            (BulkAction::Stop, RunMode::Stopped),
            (BulkAction::Start, RunMode::Auto),
            (BulkAction::ForceStart, RunMode::Forced),
            (BulkAction::Pause, RunMode::Paused),
            (BulkAction::Resume, RunMode::Auto),
        ] {
            let r = b
                .svc
                .bulk(BulkRequest {
                    ids: None,
                    action: act,
                })
                .await
                .unwrap();
            assert_eq!(r.ok.len(), 2);
            let list = b.svc.list().await.unwrap();
            assert!(list.iter().all(|t| t.run_mode == want), "{want:?}");
        }

        b.svc
            .bulk(BulkRequest {
                ids: None,
                action: BulkAction::Turtle(true),
            })
            .await
            .unwrap();
        assert!(b.engine.settings().torrents.turtle);
        assert!(b.svc.stats().await.unwrap().turtle);

        let r = b
            .svc
            .bulk(BulkRequest {
                ids: None,
                action: BulkAction::Remove { delete_files: true },
            })
            .await
            .unwrap();
        assert_eq!(r.ok.len(), 2);
        assert!(b.svc.list().await.unwrap().is_empty());
        assert!(!b.downloads().join("one.bin").exists());

        b.svc.stop().await;
        a.svc.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn restart_keeps_torrents_and_totals() {
        let a = node(Vec::new()).await;
        let b = node(Vec::new()).await;
        let (id, magnet, _) = seed(&a, "payload.bin", 3).await;
        b.svc
            .add(AddRequest::new(AddSource::Magnet(magnet)))
            .await
            .unwrap();
        wait(&b.svc, &id, "download total", |t| {
            t.state == TorrentState::Seeding && t.downloaded_total >= FILE_LEN as u64
        })
        .await;
        wait(&a.svc, &id, "upload total", |t| {
            t.uploaded_total >= FILE_LEN as u64
        })
        .await;
        let limits = SpeedLimits {
            down_kib: Some(100),
            up_kib: None,
        };
        b.svc
            .update(
                &id,
                TorrentPatch {
                    limits: Some(limits),
                    ..TorrentPatch::default()
                },
            )
            .await
            .unwrap();

        // stop / start.
        b.svc.stop().await;
        assert!(!b.svc.is_running());
        let off = b.svc.list().await.unwrap();
        assert_eq!(off.len(), 1, "listed while off");
        assert!(off[0].downloaded_total >= FILE_LEN as u64);
        assert!(matches!(
            b.svc
                .add(AddRequest::new(AddSource::Magnet("magnet:?x".into())))
                .await,
            Err(Error::Invalid(_))
        ));
        b.svc.start().await.unwrap();
        let t = wait(&b.svc, &id, "restored", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        assert!(
            t.downloaded_total >= FILE_LEN as u64,
            "totals survive a restart"
        );
        assert_eq!(b.svc.details(&id).await.unwrap().limits, limits);

        // apply_settings off and on.
        let mut settings = b.engine.settings();
        settings.torrents.enabled = false;
        let saved = b.engine.set_settings(settings).await.unwrap();
        b.svc.apply_settings(&saved).await.unwrap();
        assert!(!b.svc.is_running());
        let mut settings = b.engine.settings();
        settings.torrents.enabled = true;
        let saved = b.engine.set_settings(settings).await.unwrap();
        b.svc.apply_settings(&saved).await.unwrap();
        assert!(b.svc.is_running());
        let t = wait(&b.svc, &id, "restored again", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        assert!(t.downloaded_total >= FILE_LEN as u64);

        // The seeder keeps its upload total too.
        a.svc.stop().await;
        a.svc.start().await.unwrap();
        let t = wait(&a.svc, &id, "seeder restored", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        assert!(t.uploaded_total >= FILE_LEN as u64, "upload total survives");
        assert!(a.svc.torrent_file(&id).await.is_ok());

        b.svc.stop().await;
        a.svc.stop().await;
    }

    /// A clock that reads Monday at the hour in `hour`.
    fn monday(hour: &Arc<AtomicU8>) -> Clock {
        let hour = hour.clone();
        Arc::new(move || Moment::new(1_800_000_000_000, 0, hour.load(Ordering::SeqCst)))
    }

    async fn apply(node: &Node, tweak: impl FnOnce(&mut TorrentSettings)) {
        let mut settings = node.engine.settings();
        tweak(&mut settings.torrents);
        let saved = node.engine.set_settings(settings).await.unwrap();
        node.svc.apply_settings(&saved).await.unwrap();
    }

    async fn wait_limits(svc: &TorrentService, want: SpeedLimits) {
        let started = Instant::now();
        while svc.applied_limits() != Some(want) {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "limits are {:?}, want {want:?}",
                svc.applied_limits()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn seed_ratio_one_marks_done_and_pauses() {
        let a = node_with(Vec::new(), |t| t.seed_ratio = Some(1.0)).await;
        let b = node(Vec::new()).await;
        let (id, magnet, _) = seed(&a, "payload.bin", 7).await;
        b.svc
            .add(AddRequest::new(AddSource::Magnet(magnet)))
            .await
            .unwrap();
        // A uploads the whole file to B: ratio 1.0.
        let t = wait(&a.svc, &id, "seeder done", |t| {
            t.state == TorrentState::Done
        })
        .await;
        assert!(t.done);
        assert!(t.uploaded_total >= FILE_LEN as u64);
        let details = a.svc.details(&id).await.unwrap();
        assert!(details.summary.done);
        assert!(details.seeding_since.is_some());
        wait(&b.svc, &id, "leecher seeds", |t| {
            t.state == TorrentState::Seeding
        })
        .await;

        // The mark survives a restart, and a higher limit seeds again.
        a.svc.stop().await;
        a.svc.start().await.unwrap();
        wait(&a.svc, &id, "still done", |t| t.state == TorrentState::Done).await;
        apply(&a, |t| t.seed_ratio = Some(5.0)).await;
        let t = wait(&a.svc, &id, "seeding again", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        assert!(!t.done);

        // A per-torrent override wins over the global limit.
        let patch = TorrentPatch {
            seed: Some(SeedLimits {
                ratio: Some(0.5),
                minutes: None,
            }),
            ..TorrentPatch::default()
        };
        a.svc.update(&id, patch).await.unwrap();
        wait(&a.svc, &id, "done by override", |t| {
            t.state == TorrentState::Done
        })
        .await;
        b.svc.stop().await;
        a.svc.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn schedule_follows_the_injected_clock() {
        let limits = SpeedLimits {
            down_kib: Some(5000),
            up_kib: None,
        };
        let alt_limits = SpeedLimits {
            down_kib: Some(1000),
            up_kib: Some(500),
        };
        // Monday 10:00 off, 11:00 alternative speed, 12:00 seed only.
        let mut cells = vec!['n'; WeekGrid::LEN];
        cells[10] = 'o';
        cells[11] = 'a';
        cells[12] = 's';
        let grid = WeekGrid(cells.into_iter().collect());
        let a = node(Vec::new()).await;
        let b = node_with(Vec::new(), |t| {
            t.limits = limits;
            t.alt_limits = alt_limits;
            t.schedule = Some(grid);
        })
        .await;
        let hour = Arc::new(AtomicU8::new(10));
        b.svc.set_clock(monday(&hour));
        let (id, magnet, data) = seed(&a, "payload.bin", 8).await;

        // Off pauses everything, a forced torrent too.
        b.svc
            .add(AddRequest::new(AddSource::Magnet(magnet)))
            .await
            .unwrap();
        b.svc
            .update(&id, action(TorrentAction::ForceStart))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let t = b.svc.list().await.unwrap().remove(0);
        assert_eq!(t.state, TorrentState::ScheduledOff);
        assert_eq!(t.have, 0);
        let stats = b.svc.stats().await.unwrap();
        assert_eq!(stats.schedule_mode, ScheduleMode::Off);
        wait_limits(&b.svc, limits).await;

        // Alternative speed applies the alternative limits and downloads.
        hour.store(11, Ordering::SeqCst);
        wait_limits(&b.svc, alt_limits).await;
        let t = wait(&b.svc, &id, "downloaded", |t| {
            t.state == TorrentState::Seeding
        })
        .await;
        assert!((t.progress - 1.0).abs() < f64::EPSILON);
        assert!(std::fs::read(b.downloads().join("payload.bin")).unwrap() == data);

        // Seed only holds downloads, not seeds.
        hour.store(12, Ordering::SeqCst);
        wait_limits(&b.svc, limits).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert_eq!(b.svc.list().await.unwrap()[0].state, TorrentState::Seeding);

        // Off stops the seed too; Normal brings it back.
        hour.store(10, Ordering::SeqCst);
        wait(&b.svc, &id, "off", |t| {
            t.state == TorrentState::ScheduledOff
        })
        .await;
        hour.store(9, Ordering::SeqCst);
        wait(&b.svc, &id, "normal", |t| t.state == TorrentState::Seeding).await;

        // Turtle mode forces the alternative limits.
        apply(&b, |t| t.turtle = true).await;
        wait_limits(&b.svc, alt_limits).await;
        apply(&b, |t| t.turtle = false).await;
        wait_limits(&b.svc, limits).await;
        b.svc.stop().await;
        a.svc.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn power_and_network_gates() {
        let limits = SpeedLimits {
            down_kib: Some(5000),
            up_kib: None,
        };
        let alt_limits = SpeedLimits {
            down_kib: Some(1000),
            up_kib: Some(500),
        };
        let a = node(Vec::new()).await;
        let b = node_with(Vec::new(), |t| {
            t.limits = limits;
            t.alt_limits = alt_limits;
            t.on_battery = OnBattery::Pause;
        })
        .await;
        let (id, magnet, _) = seed(&a, "payload.bin", 9).await;

        // Pause: nothing runs on battery.
        b.svc.set_power(PowerState { on_battery: true }).await;
        assert!(b.svc.stats().await.unwrap().on_battery);
        b.svc
            .add(AddRequest::new(AddSource::Magnet(magnet)))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let t = b.svc.list().await.unwrap().remove(0);
        assert_eq!(t.state, TorrentState::OnBattery);
        assert_eq!(t.have, 0);
        b.svc.set_power(PowerState { on_battery: false }).await;
        wait(&b.svc, &id, "downloaded", |t| {
            t.state == TorrentState::Seeding
        })
        .await;

        // Alt speed: the alternative limits while on battery.
        apply(&b, |t| t.on_battery = OnBattery::AltSpeed).await;
        assert_eq!(b.svc.applied_limits(), Some(limits));
        b.svc.set_power(PowerState { on_battery: true }).await;
        assert_eq!(b.svc.applied_limits(), Some(alt_limits));
        assert_eq!(b.svc.list().await.unwrap()[0].state, TorrentState::Seeding);
        // Nothing: no effect.
        apply(&b, |t| t.on_battery = OnBattery::Nothing).await;
        assert_eq!(b.svc.applied_limits(), Some(limits));
        b.svc.set_power(PowerState { on_battery: false }).await;

        // The Wi-Fi gate.
        b.svc.set_network_allowed(false).await;
        assert!(!b.svc.stats().await.unwrap().network_allowed);
        wait(&b.svc, &id, "waiting", |t| {
            t.state == TorrentState::WaitingForNetwork
        })
        .await;
        b.svc.set_network_allowed(true).await;
        wait(&b.svc, &id, "back", |t| t.state == TorrentState::Seeding).await;
        b.svc.stop().await;
        a.svc.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn watched_folder_adds_and_renames() {
        let a = node(Vec::new()).await;
        let watch = tempfile::tempdir().unwrap();
        let dir = watch.path().to_path_buf();
        let b = node_with(Vec::new(), |t| t.watch_dir = Some(dir)).await;
        let (id, _, _) = seed(&a, "payload.bin", 10).await;
        let torrent = a.svc.torrent_file(&id).await.unwrap();

        std::fs::write(watch.path().join("good.torrent"), &torrent).unwrap();
        std::fs::write(watch.path().join("bad.torrent"), b"not a torrent").unwrap();
        std::fs::write(watch.path().join("notes.txt"), b"ignored").unwrap();
        let started = Instant::now();
        let (added, failed) = (
            watch.path().join("good.torrent.added"),
            watch.path().join("bad.torrent.failed"),
        );
        while !(added.exists() && failed.exists()) {
            assert!(started.elapsed() < Duration::from_secs(20), "not renamed");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!watch.path().join("good.torrent").exists());
        assert!(!watch.path().join("bad.torrent").exists());
        assert!(watch.path().join("notes.txt").exists());
        let list = b.svc.list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].source, Origin::Watch);

        // A torrent the client already has counts as added.
        std::fs::write(watch.path().join("again.torrent"), &torrent).unwrap();
        let again = watch.path().join("again.torrent.added");
        while !again.exists() {
            assert!(started.elapsed() < Duration::from_secs(30), "not renamed");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(b.svc.list().await.unwrap().len(), 1);
        b.svc.stop().await;
        a.svc.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn remove_keeps_the_sources_of_created_torrents() {
        let a = node(Vec::new()).await;
        let (id, _, data) = seed(&a, "mine.bin", 7).await;
        a.svc.remove(&id, true).await.unwrap();
        assert!(a.svc.list().await.unwrap().is_empty());
        let source = a.dir.path().join("share").join("mine.bin");
        assert_eq!(std::fs::read(source).unwrap(), data);
        a.svc.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn turtle_works_while_the_client_is_off() {
        let a = node(Vec::new()).await;
        a.svc.stop().await;
        assert!(!a.svc.is_running());
        a.svc
            .bulk(BulkRequest {
                ids: None,
                action: BulkAction::Turtle(true),
            })
            .await
            .unwrap();
        assert!(a.engine.settings().torrents.turtle);
        assert!(!a.svc.is_running());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn start_refuses_a_download_dir_outside_the_roots() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let engine = Engine::with_store(
            AppPaths::at(&data),
            Store::open_in_memory().await.unwrap(),
            EngineOptions {
                secrets: Arc::new(MemorySecretStore::default()),
                builtins: Vec::new(),
                definitions: Vec::new(),
            },
        )
        .await
        .unwrap();
        let mut settings = engine.settings();
        settings.torrents.dht = false;
        settings.torrents.lsd = false;
        settings.torrents.upnp = false;
        engine.set_settings(settings).await.unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        let outside = dir.path().join("downloads");
        let mut opts = ServiceOptions::new(&data, &outside);
        opts.path_policy = hashlark_torrent::PathPolicy::Roots(vec![media.clone()]);
        opts.listen_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        opts.listen_port_override = Some(0);
        let svc = TorrentService::new(engine.clone(), opts);

        // The default folder is outside the roots.
        assert!(matches!(svc.start().await, Err(Error::Invalid(_))));
        assert!(!svc.is_running());
        assert!(!outside.exists());

        // So is a configured one; nothing is created there either.
        let elsewhere = dir.path().join("elsewhere");
        let mut settings = engine.settings();
        settings.torrents.download_dir = Some(elsewhere.clone());
        engine.set_settings(settings).await.unwrap();
        assert!(matches!(svc.start().await, Err(Error::Invalid(_))));
        assert!(!elsewhere.exists());

        // One inside a root starts.
        let mut settings = engine.settings();
        settings.torrents.download_dir = Some(media.join("torrents"));
        engine.set_settings(settings).await.unwrap();
        svc.start().await.unwrap();
        assert!(svc.is_running());
        svc.stop().await;
    }
}
