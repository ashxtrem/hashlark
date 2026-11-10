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
