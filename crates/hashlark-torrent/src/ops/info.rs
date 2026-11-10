// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only operations: list, details, peers, stats, torrent_file.

use std::sync::Arc;

use hashlark_core::settings::SpeedLimits;
use hashlark_core::{Error, Result};
use librqbit::{Magnet, ManagedTorrent, TorrentStatsState};

use crate::model::{
    PeerInfo, RunMode, ScheduleMode, SessionStats, StateCounts, TorrentDetails, TorrentFile,
    TorrentState, TorrentSummary,
};
use crate::service::{Counter, Desired, Running, TorrentService, torrent_key};
use crate::store::Row;

/// What a summary is built from.
pub(crate) struct Parts<'a> {
    pub row: &'a Row,
    pub handle: Option<&'a Arc<ManagedTorrent>>,
    pub fetching: bool,
    pub counter: Counter,
    pub error: Option<String>,
    pub desired: Option<Desired>,
    /// The schedule mode of the last policy run.
    pub schedule: ScheduleMode,
}

pub(crate) fn summarize(p: Parts<'_>) -> TorrentSummary {
    let row = p.row;
    let stats = p.handle.map(|h| h.stats());
    let live = stats.as_ref().and_then(|s| s.live.as_ref());
    let size = stats.as_ref().map_or(0, |s| s.total_bytes);
    let have = stats.as_ref().map_or(0, |s| s.progress_bytes);
    let finished = stats.as_ref().is_some_and(|s| s.finished);
    let error = p
        .error
        .or_else(|| stats.as_ref().and_then(|s| s.error.clone()));
    let down_bps = live.map_or(0, |l| l.download_speed.as_bytes());
    let up_bps = live.map_or(0, |l| l.upload_speed.as_bytes());
    // Not transferring: the user's Stop and Pause win over the reason the
    // policy gives for the others.
    let parked = |finished: bool| match row.run_mode {
        RunMode::Stopped if finished => TorrentState::Finished,
        RunMode::Stopped => TorrentState::Stopped,
        RunMode::Paused => TorrentState::Paused,
        _ => match p.desired {
            Some(Desired::Stopped) => TorrentState::Stopped,
            Some(Desired::Paused) => TorrentState::Paused,
            Some(Desired::Done) => TorrentState::Done,
            Some(Desired::OnBattery) => TorrentState::OnBattery,
            Some(Desired::NetworkWait) => TorrentState::WaitingForNetwork,
            Some(Desired::Scheduled) if p.schedule == ScheduleMode::Off => {
                TorrentState::ScheduledOff
            }
            Some(Desired::Scheduled) => TorrentState::ScheduledSeedOnly,
            _ if finished && row.done => TorrentState::Done,
            _ => TorrentState::Queued,
        },
    };
    let state = if error.is_some() {
        TorrentState::Error
    } else if p.fetching {
        TorrentState::Metadata
    } else {
        match stats.as_ref().map(|s| s.state) {
            None => parked(false),
            Some(TorrentStatsState::Initializing { paused: false }) => TorrentState::Checking,
            Some(TorrentStatsState::Initializing { paused: true }) => parked(false),
            Some(TorrentStatsState::Live) if finished => TorrentState::Seeding,
            Some(TorrentStatsState::Live) => TorrentState::Downloading,
            Some(TorrentStatsState::Paused) => parked(finished),
            Some(TorrentStatsState::Error) => TorrentState::Error,
        }
    };
    let denominator = p.counter.down.max(size);
    #[allow(clippy::cast_precision_loss)]
    let ratio = if denominator == 0 {
        0.0
    } else {
        p.counter.up as f64 / denominator as f64
    };
    #[allow(clippy::cast_precision_loss)]
    let progress = if size == 0 {
        0.0
    } else {
        have as f64 / size as f64
    };
    let remaining = size.saturating_sub(have);
    let peers = live.map(|l| &l.snapshot.peer_stats);
    TorrentSummary {
        id: row.info_hash.clone(),
        name: row.name.clone(),
        state,
        run_mode: row.run_mode,
        source: row.source,
        progress,
        size,
        have,
        downloaded_total: p.counter.down,
        uploaded_total: p.counter.up,
        ratio,
        down_bps,
        up_bps,
        eta_secs: (state == TorrentState::Downloading && down_bps > 0)
            .then(|| remaining.div_ceil(down_bps)),
        peers: peers.map_or(0, |p| p.live),
        seeds: peers.map_or(0, |p| p.live_seeders),
        queue_pos: row.queue_pos,
        save_dir: row.save_dir.clone(),
        added_at: row.added_at,
        completed_at: row.completed_at,
        done: row.done,
        error,
    }
}

impl TorrentService {
    /// Summaries of a running client, in queue order.
    pub(crate) fn summaries(&self, run: &Running) -> Vec<TorrentSummary> {
        let handles = self.handles(run);
        let st = run.state.lock().expect("lock");
        let mut rows: Vec<&Row> = st.rows.values().collect();
        rows.sort_by_key(|r| (r.queue_pos, r.added_at));
        rows.into_iter()
            .map(|row| {
                let hash = &row.info_hash;
                summarize(Parts {
                    row,
                    handle: handles.get(hash),
                    fetching: st.pending.contains_key(hash),
                    counter: st.counters.get(hash).copied().unwrap_or_default(),
                    error: st.errors.get(hash).cloned(),
                    desired: st.desired.get(hash).copied(),
                    schedule: st.schedule,
                })
            })
            .collect()
    }

    pub(crate) fn summary(&self, run: &Running, id: &str) -> Result<TorrentSummary> {
        self.summaries(run)
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| Error::NotFound(format!("torrent `{id}`")))
    }

    /// Every torrent, in queue order. Works while the client is off too.
    pub async fn list(&self) -> Result<Vec<TorrentSummary>> {
        match self.running() {
            Some(run) => Ok(self.summaries(&run)),
            None => {
                let rows = crate::store::list(self.engine().store()).await?;
                Ok(rows
                    .iter()
                    .map(|row| {
                        summarize(Parts {
                            row,
                            handle: None,
                            fetching: false,
                            counter: Counter::from_row(row),
                            error: None,
                            desired: Some(Desired::Paused),
                            schedule: ScheduleMode::Normal,
                        })
                    })
                    .collect())
            }
        }
    }

    /// Summary, files, trackers, limits and magnet of one torrent.
    pub async fn details(&self, id: &str) -> Result<TorrentDetails> {
        let id = id.to_ascii_lowercase();
        let key = torrent_key(&id)?;
        let (summary, row, handle) = match self.running() {
            Some(run) => {
                let summary = self.summary(&run, &id)?;
                let row = run.state.lock().expect("lock").rows.get(&id).cloned();
                (summary, row, run.session.get(key))
            }
            None => {
                let summary = self
                    .list()
                    .await?
                    .into_iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| Error::NotFound(format!("torrent `{id}`")))?;
                let rows = crate::store::list(self.engine().store()).await?;
                (summary, rows.into_iter().find(|r| r.info_hash == id), None)
            }
        };
        let row = row.ok_or_else(|| Error::NotFound(format!("torrent `{id}`")))?;
        let mut files = Vec::new();
        let mut trackers = Vec::new();
        let mut magnet = std::fs::read_to_string(self.inner.opts.magnet_file(&id)).ok();
        if let Some(handle) = &handle {
            let stats = handle.stats();
            let selected = handle.only_files();
            let _ = handle.with_metadata(|m| {
                for (index, fi) in m.file_infos.iter().enumerate() {
                    if fi.attrs.padding {
                        continue;
                    }
                    let path = fi
                        .relative_filename
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/");
                    files.push(TorrentFile {
                        index,
                        path,
                        size: fi.len,
                        have: stats.file_progress.get(index).copied().unwrap_or(0),
                        selected: selected.as_ref().is_none_or(|s| s.contains(&index)),
                    });
                }
            });
            trackers = handle
                .shared()
                .trackers
                .iter()
                .map(ToString::to_string)
                .collect();
            trackers.sort();
            if magnet.is_none() {
                magnet =
                    Some(Magnet::from_id20(handle.info_hash(), trackers.clone(), None).to_string());
            }
        }
        Ok(TorrentDetails {
            summary,
            files,
            trackers,
            limits: SpeedLimits {
                down_kib: row.down_kib,
                up_kib: row.up_kib,
            },
            peer_limit: row.peer_limit,
            seed: row.seed,
            magnet,
            result_id: row.result_id.clone(),
            seeding_since: row.seeding_since,
        })
    }

    /// Connected peers of a live torrent (empty when it isn't live).
    pub async fn peers(&self, id: &str) -> Result<Vec<PeerInfo>> {
        let id = id.to_ascii_lowercase();
        let run = self.require()?;
        let key = torrent_key(&id)?;
        if !run.state.lock().expect("lock").rows.contains_key(&id) {
            return Err(Error::NotFound(format!("torrent `{id}`")));
        }
        let Some(live) = run.session.get(key).and_then(|h| h.live()) else {
            return Ok(Vec::new());
        };
        let snapshot = live.per_peer_stats_snapshot(Default::default());
        let mut peers: Vec<PeerInfo> = snapshot
            .peers
            .into_iter()
            .map(|(addr, p)| PeerInfo {
                addr,
                client: p.client_name,
                conn_kind: p.conn_kind.map(|k| format!("{k:?}").to_ascii_lowercase()),
                state: p.state.to_owned(),
                downloaded: p.counters.fetched_bytes,
                uploaded: p.counters.uploaded_bytes,
            })
            .collect();
        peers.sort_by(|a, b| a.addr.cmp(&b.addr));
        Ok(peers)
    }

    /// Session-wide numbers.
    pub async fn stats(&self) -> Result<SessionStats> {
        match self.running() {
            Some(run) => Ok(self.session_stats(&run)),
            None => {
                let device = self.device();
                let settings = self.engine().settings().torrents;
                let list = self.list().await?;
                Ok(SessionStats {
                    running: false,
                    down_bps: 0,
                    up_bps: 0,
                    listen_port: None,
                    dht_nodes: None,
                    schedule_mode: self.schedule_mode(&settings),
                    turtle: settings.turtle,
                    on_battery: device.on_battery,
                    network_allowed: device.network_allowed,
                    peers_connected: 0,
                    counts: counts(&list),
                })
            }
        }
    }

    pub(crate) fn session_stats(&self, run: &Running) -> SessionStats {
        let snap = run.session.stats_snapshot();
        let settings = run.settings.lock().expect("lock").clone();
        let device = self.device();
        let dht_nodes = run.session.get_dht().map(|d| {
            let s = d.stats();
            u64::try_from(s.routing_table_size + s.routing_table_size_v6).unwrap_or(u64::MAX)
        });
        SessionStats {
            running: true,
            down_bps: snap.download_speed.as_bytes(),
            up_bps: snap.upload_speed.as_bytes(),
            listen_port: run.session.listen_addr().map(|a| a.port()),
            dht_nodes,
            schedule_mode: self.schedule_mode(&settings),
            turtle: settings.turtle,
            on_battery: device.on_battery,
            network_allowed: device.network_allowed,
            peers_connected: snap.peers.live,
            counts: counts(&self.summaries(run)),
        }
    }

    /// The `.torrent` file: the stored copy of a created torrent, else the
    /// metadata librqbit has.
    pub async fn torrent_file(&self, id: &str) -> Result<Vec<u8>> {
        let id = id.to_ascii_lowercase();
        let key = torrent_key(&id)?;
        if let Ok(bytes) = tokio::fs::read(self.inner.opts.created_file(&id)).await {
            return Ok(bytes);
        }
        self.running()
            .and_then(|run| run.session.get(key))
            .and_then(|h| h.metadata.load().as_ref().map(|m| m.torrent_bytes.to_vec()))
            .ok_or_else(|| Error::NotFound(format!("metadata of torrent `{id}`")))
    }
}

/// Torrents per filter tab.
pub(crate) fn counts(list: &[TorrentSummary]) -> StateCounts {
    let mut c = StateCounts {
        all: u32::try_from(list.len()).unwrap_or(u32::MAX),
        ..StateCounts::default()
    };
    for t in list {
        let slot = match t.state {
            TorrentState::Metadata | TorrentState::Checking | TorrentState::Downloading => {
                &mut c.downloading
            }
            TorrentState::Seeding => &mut c.seeding,
            TorrentState::Queued
            | TorrentState::ScheduledOff
            | TorrentState::ScheduledSeedOnly
            | TorrentState::OnBattery
            | TorrentState::WaitingForNetwork => &mut c.queued,
            TorrentState::Paused => &mut c.paused,
            TorrentState::Stopped => &mut c.stopped,
            TorrentState::Finished | TorrentState::Done => &mut c.finished,
            TorrentState::Error => &mut c.error,
        };
        *slot += 1;
    }
    c
}
