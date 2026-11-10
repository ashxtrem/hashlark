// SPDX-License-Identifier: GPL-3.0-or-later

//! Adding torrents (magnet, `.torrent`, search result) and creating them.

use std::path::PathBuf;

use hashlark_core::engine::MAX_TORRENT_BYTES;
use hashlark_core::model::{DownloadTarget, TargetKind};
use hashlark_core::{Error, Result};
use librqbit::{AddTorrent, CreateTorrentOptions, Magnet, torrent_from_bytes};

use crate::model::{
    AddRequest, AddResponse, AddSource, CreateRequest, CreatedTorrent, Origin, RunMode, SeedLimits,
};
use crate::service::{Counter, TorrentService, invalid, is_active};
use crate::store::{self, Row};

/// A torrent to add, once its source is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    Magnet {
        uri: String,
        hash: String,
        name: Option<String>,
    },
    Bytes {
        bytes: Vec<u8>,
        hash: String,
        name: Option<String>,
    },
}

impl Source {
    pub fn hash(&self) -> &str {
        match self {
            Self::Magnet { hash, .. } | Self::Bytes { hash, .. } => hash,
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            Self::Magnet { name, .. } | Self::Bytes { name, .. } => name.as_deref(),
        }
    }
}

/// Reads the info hash and name of a magnet.
pub(crate) fn parse_magnet(uri: &str) -> Result<Source> {
    let uri = uri.trim();
    let magnet = Magnet::parse(uri).map_err(|e| invalid("not a valid magnet link", e))?;
    let hash = magnet
        .as_id20()
        .ok_or_else(|| Error::Invalid("the magnet link has no BitTorrent v1 info hash".into()))?
        .as_string();
    Ok(Source::Magnet {
        uri: uri.to_owned(),
        hash,
        name: magnet.name,
    })
}

/// Reads the info hash and name of a `.torrent` file.
pub(crate) fn parse_torrent(bytes: Vec<u8>) -> Result<Source> {
    if bytes.len() as u64 > MAX_TORRENT_BYTES {
        return Err(Error::Invalid("the .torrent file is too large".into()));
    }
    let (hash, name) = {
        let meta =
            torrent_from_bytes(&bytes).map_err(|e| invalid("not a valid .torrent file", e))?;
        let name = meta
            .info
            .data
            .name
            .as_ref()
            .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned())
            .filter(|n| !n.is_empty());
        (meta.info_hash.as_string(), name)
    };
    Ok(Source::Bytes { bytes, hash, name })
}

/// Decodes a base64 `.torrent`.
pub(crate) fn decode_base64(text: &str) -> Result<Vec<u8>> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    // 4 base64 characters carry 3 bytes.
    if clean.len() as u64 / 4 * 3 > MAX_TORRENT_BYTES + 3 {
        return Err(Error::Invalid("the .torrent file is too large".into()));
    }
    data_encoding::BASE64
        .decode(clean.as_bytes())
        .or_else(|_| data_encoding::BASE64_NOPAD.decode(clean.as_bytes()))
        .map_err(|e| invalid("the .torrent file is not valid base64", e))
}

/// Adds `so=` (selected files) to a magnet, so a resumed fetch keeps them.
fn with_selection(uri: &str, files: Option<&[usize]>) -> String {
    match files {
        Some(files) if !files.is_empty() => {
            let list: Vec<String> = files.iter().map(ToString::to_string).collect();
            format!("{uri}&so={}", list.join(","))
        }
        _ => uri.to_owned(),
    }
}

impl TorrentService {
    /// Adds a torrent (plan §3.2). A known info hash returns the existing
    /// torrent with `already: true`.
    pub async fn add(&self, req: AddRequest) -> Result<AddResponse> {
        let origin = match req.source {
            AddSource::Magnet(_) => Origin::Magnet,
            AddSource::Torrent(_) => Origin::Torrent,
            AddSource::ResultId(_) => Origin::Result,
        };
        self.add_from(req, origin).await
    }

    /// [`add`](Self::add) with an explicit origin (the watched folder uses
    /// [`Origin::Watch`]).
    pub async fn add_from(&self, req: AddRequest, origin: Origin) -> Result<AddResponse> {
        let run = self.require()?;
        let (source, result_id) = self.read_source(&req.source).await?;
        let hash = source.hash().to_owned();
        if run.state.lock().expect("lock").rows.contains_key(&hash) {
            return Ok(AddResponse {
                torrent: self.summary(&run, &hash)?,
                already: true,
            });
        }
        if req.files.as_ref().is_some_and(Vec::is_empty) {
            return Err(Error::Invalid("select at least one file".into()));
        }
        let save_dir = self.check_dir(req.save_dir.clone())?;

        let _op = run.op_lock.lock().await;
        let row = {
            let mut st = run.state.lock().expect("lock");
            if st.rows.contains_key(&hash) {
                drop(st);
                return Ok(AddResponse {
                    torrent: self.summary(&run, &hash)?,
                    already: true,
                });
            }
            let queue_pos = st.rows.values().map(|r| r.queue_pos + 1).max().unwrap_or(0);
            let row = Row {
                info_hash: hash.clone(),
                name: source.name().unwrap_or(&hash).to_owned(),
                save_dir: save_dir.clone(),
                source: origin,
                result_id,
                added_at: self.now().ms,
                completed_at: None,
                seeding_since: None,
                run_mode: if req.paused {
                    RunMode::Paused
                } else {
                    RunMode::Auto
                },
                queue_pos,
                peer_limit: None,
                seed: SeedLimits::default(),
                down_kib: None,
                up_kib: None,
                downloaded_total: 0,
                uploaded_total: 0,
                done: false,
            };
            st.rows.insert(hash.clone(), row.clone());
            st.counters.insert(hash.clone(), Counter::from_row(&row));
            row
        };
        let added = self
            .add_new(&run, &row, &source, req.files.as_deref())
            .await;
        if let Err(e) = added {
            {
                let mut st = run.state.lock().expect("lock");
                st.rows.remove(&hash);
                st.counters.remove(&hash);
            }
            let _ = std::fs::remove_file(self.inner.opts.magnet_file(&hash));
            let _ = store::delete(self.engine().store(), &hash).await;
            return Err(e);
        }
        self.reconcile_locked(&run).await;
        Ok(AddResponse {
            torrent: self.summary(&run, &hash)?,
            already: false,
        })
    }

    async fn add_new(
        &self,
        run: &std::sync::Arc<crate::service::Running>,
        row: &Row,
        source: &Source,
        files: Option<&[usize]>,
    ) -> Result<()> {
        store::insert(self.engine().store(), row).await?;
        match source {
            Source::Magnet { uri, hash, .. } => {
                // The fetch starts when the policy lets it run.
                std::fs::write(
                    self.inner.opts.magnet_file(hash),
                    with_selection(uri, files),
                )?;
            }
            Source::Bytes { bytes, .. } => {
                let running = self.handles(run).values().filter(|h| is_active(h)).count();
                let mut opts = self.add_options(run, row, Vec::new(), running + 1, true);
                opts.only_files = files.map(<[usize]>::to_vec);
                run.session
                    .add_torrent(AddTorrent::from_bytes(bytes.clone()), Some(opts))
                    .await
                    .map_err(|e| invalid("could not add the torrent", e))?;
            }
        }
        Ok(())
    }

    /// Resolves the source of an add request.
    async fn read_source(&self, source: &AddSource) -> Result<(Source, Option<String>)> {
        match source {
            AddSource::Magnet(uri) => Ok((parse_magnet(uri)?, None)),
            AddSource::Torrent(b64) => Ok((parse_torrent(decode_base64(b64)?)?, None)),
            AddSource::ResultId(id) => {
                let engine = self.engine();
                let source = match engine.resolve(id, Some(TargetKind::Magnet)).await? {
                    DownloadTarget::Magnet(uri) => parse_magnet(&uri)?,
                    DownloadTarget::TorrentFile(url) => {
                        parse_torrent(engine.fetch_torrent(&url).await?)?
                    }
                };
                Ok((source, Some(id.clone())))
            }
        }
    }

    /// Checks a save folder against the path policy and creates it.
    fn check_dir(&self, dir: Option<PathBuf>) -> Result<PathBuf> {
        let dir = dir.unwrap_or_else(|| self.download_dir());
        let dir = self.path_policy().check(&dir)?;
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Creates a torrent from a file or folder and seeds it at once.
    pub async fn create(&self, req: CreateRequest) -> Result<CreatedTorrent> {
        let run = self.require()?;
        let path = self.path_policy().check(&req.path)?;
        if !path.exists() {
            return Err(Error::Invalid(format!(
                "`{}` doesn't exist",
                path.display()
            )));
        }
        let _op = run.op_lock.lock().await;
        let (created, handle) = run
            .session
            .create_and_serve_torrent(
                &path,
                CreateTorrentOptions {
                    name: req.name.as_deref().filter(|n| !n.is_empty()),
                    trackers: req.trackers.clone(),
                    piece_length: req.piece_length,
                },
            )
            .await
            .map_err(|e| invalid("could not create the torrent", e))?;
        let hash = created.info_hash().as_string();
        let magnet = created.as_magnet().to_string();
        let bytes = created
            .as_bytes()
            .map_err(|e| invalid("could not create the torrent", e))?;
        let known = run.state.lock().expect("lock").rows.contains_key(&hash);
        if !known {
            tokio::fs::write(self.inner.opts.created_file(&hash), &bytes).await?;
            let now = self.now().ms;
            let queue_pos = {
                let st = run.state.lock().expect("lock");
                st.rows.values().map(|r| r.queue_pos + 1).max().unwrap_or(0)
            };
            let row = Row {
                info_hash: hash.clone(),
                name: handle.name().unwrap_or_else(|| hash.clone()),
                save_dir: created.output_folder.clone(),
                source: Origin::Created,
                result_id: None,
                added_at: now,
                completed_at: Some(now),
                seeding_since: Some(now),
                run_mode: RunMode::Auto,
                queue_pos,
                peer_limit: None,
                seed: SeedLimits::default(),
                down_kib: None,
                up_kib: None,
                downloaded_total: 0,
                uploaded_total: 0,
                done: false,
            };
            store::insert(self.engine().store(), &row).await?;
            let mut st = run.state.lock().expect("lock");
            st.counters.insert(hash.clone(), Counter::from_row(&row));
            st.rows.insert(hash.clone(), row);
        }
        drop(handle);
        self.reconcile_locked(&run).await;
        Ok(CreatedTorrent {
            torrent: self.summary(&run, &hash)?,
            magnet,
            already: known,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "1dc689206d6f6ef6021d558d95350ed09004b40c";

    #[test]
    fn magnets() {
        let s = parse_magnet(&format!("magnet:?xt=urn:btih:{HASH}&dn=Ubuntu")).unwrap();
        assert_eq!(s.hash(), HASH);
        assert_eq!(s.name(), Some("Ubuntu"));
        let upper = parse_magnet(&format!("magnet:?xt=urn:btih:{}", HASH.to_uppercase())).unwrap();
        assert_eq!(upper.hash(), HASH);
        assert!(matches!(
            parse_magnet("magnet:?dn=x"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(parse_magnet("https://x"), Err(Error::Invalid(_))));
    }

    #[test]
    fn torrents_and_base64() {
        assert!(matches!(
            parse_torrent(b"not bencode".to_vec()),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(decode_base64("@@@"), Err(Error::Invalid(_))));
        assert_eq!(decode_base64("ZDEyOmU=").unwrap(), b"d12:e");
        assert_eq!(decode_base64("ZDEy\nOmU").unwrap(), b"d12:e");
        let huge = "A".repeat(15 * 1024 * 1024);
        assert!(matches!(decode_base64(&huge), Err(Error::Invalid(m)) if m.contains("large")));
    }

    #[test]
    fn selection_in_magnet() {
        assert_eq!(
            with_selection("magnet:?x", Some(&[0, 2])),
            "magnet:?x&so=0,2"
        );
        assert_eq!(with_selection("magnet:?x", None), "magnet:?x");
    }
}
