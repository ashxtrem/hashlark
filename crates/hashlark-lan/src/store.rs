// SPDX-License-Identifier: GPL-3.0-or-later

//! This device's id and the devices it is paired with, kept in the
//! [`SecretStore`] (OS keychain on desktop, Android Keystore on Android)
//! because each pairing holds a key.
//!
//! Keys: `lan/self` → `{"id": ...}`, `lan/peers` → `["id", ...]`,
//! `lan/peer/<id>` → [`StoredPeer`].

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use hashlark_core::secrets::SecretStore;
use serde::{Deserialize, Serialize};

use crate::Platform;
use crate::crypto::{self, KEY_LEN};
use crate::error::{Error, Result};

const SELF_KEY: &str = "lan/self";
const INDEX_KEY: &str = "lan/peers";

fn peer_key(id: &str) -> String {
    format!("lan/peer/{id}")
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSelf {
    id: String,
}

/// A paired device as stored.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StoredPeer {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    /// Hex-encoded device key.
    key: String,
    pub last_addr: Option<SocketAddr>,
    /// Unix milliseconds.
    pub paired_at: i64,
}

impl std::fmt::Debug for StoredPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredPeer")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("platform", &self.platform)
            .field("last_addr", &self.last_addr)
            .finish_non_exhaustive()
    }
}

impl StoredPeer {
    pub fn new(
        id: String,
        name: String,
        platform: Platform,
        key: &[u8; KEY_LEN],
        last_addr: Option<SocketAddr>,
    ) -> Self {
        Self {
            id,
            name,
            platform,
            key: data_encoding::HEXLOWER.encode(key),
            last_addr,
            paired_at: crate::now_millis(),
        }
    }

    pub fn key(&self) -> Option<[u8; KEY_LEN]> {
        data_encoding::HEXLOWER
            .decode(self.key.as_bytes())
            .ok()?
            .try_into()
            .ok()
    }
}

/// Write-through cache of the paired devices. Reads never touch the
/// secret store; writes do, so async callers run them on a blocking thread.
#[derive(Debug)]
pub(crate) struct PeerStore {
    secrets: Arc<dyn SecretStore>,
    self_id: String,
    peers: Mutex<BTreeMap<String, StoredPeer>>,
    /// Serialises writes so the index never loses an entry.
    write: Mutex<()>,
}

impl PeerStore {
    /// Loads (or creates) this device's id and the paired devices. Blocks.
    pub fn load(secrets: Arc<dyn SecretStore>) -> Result<Self> {
        let self_id = match secrets.get(SELF_KEY)? {
            Some(json) => {
                serde_json::from_str::<StoredSelf>(&json)
                    .map_err(|e| Error::Storage(format!("lan/self: {e}")))?
                    .id
            }
            None => {
                let id = crypto::new_id();
                let json =
                    serde_json::to_string(&StoredSelf { id: id.clone() }).expect("serialisable");
                secrets.set(SELF_KEY, &json)?;
                id
            }
        };
        let mut peers = BTreeMap::new();
        for id in read_index(secrets.as_ref())? {
            match secrets.get(&peer_key(&id))? {
                Some(json) => match serde_json::from_str::<StoredPeer>(&json) {
                    Ok(peer) if peer.key().is_some() => {
                        peers.insert(id, peer);
                    }
                    _ => tracing::warn!(peer = %id, "ignoring an unreadable LAN pairing"),
                },
                None => tracing::warn!(peer = %id, "LAN pairing listed but missing"),
            }
        }
        Ok(Self {
            secrets,
            self_id,
            peers: Mutex::new(peers),
            write: Mutex::new(()),
        })
    }

    pub fn self_id(&self) -> &str {
        &self.self_id
    }

    pub fn get(&self, id: &str) -> Option<StoredPeer> {
        self.peers.lock().expect("lock").get(id).cloned()
    }

    pub fn all(&self) -> Vec<StoredPeer> {
        self.peers.lock().expect("lock").values().cloned().collect()
    }

    /// Adds or replaces a pairing. Blocks.
    pub fn insert(&self, peer: StoredPeer) -> Result<()> {
        let _guard = self.write.lock().expect("lock");
        let json = serde_json::to_string(&peer).expect("serialisable");
        self.secrets.set(&peer_key(&peer.id), &json)?;
        let mut ids = self.ids_with(&peer.id);
        ids.sort();
        ids.dedup();
        self.write_index(&ids)?;
        self.peers
            .lock()
            .expect("lock")
            .insert(peer.id.clone(), peer);
        Ok(())
    }

    /// Forgets a pairing. Blocks.
    pub fn remove(&self, id: &str) -> Result<bool> {
        let _guard = self.write.lock().expect("lock");
        let existed = self.peers.lock().expect("lock").remove(id).is_some();
        let ids: Vec<String> = self.peers.lock().expect("lock").keys().cloned().collect();
        self.write_index(&ids)?;
        self.secrets.delete(&peer_key(id))?;
        Ok(existed)
    }

    /// Remembers where a paired device was last reached. Blocks; does
    /// nothing when the address hasn't changed.
    pub fn set_last_addr(&self, id: &str, addr: SocketAddr) -> Result<()> {
        let updated = {
            let mut peers = self.peers.lock().expect("lock");
            match peers.get_mut(id) {
                Some(peer) if peer.last_addr != Some(addr) => {
                    peer.last_addr = Some(addr);
                    Some(peer.clone())
                }
                _ => None,
            }
        };
        if let Some(peer) = updated {
            let _guard = self.write.lock().expect("lock");
            let json = serde_json::to_string(&peer).expect("serialisable");
            self.secrets.set(&peer_key(id), &json)?;
        }
        Ok(())
    }

    fn ids_with(&self, id: &str) -> Vec<String> {
        let mut ids: Vec<String> = self.peers.lock().expect("lock").keys().cloned().collect();
        ids.push(id.to_owned());
        ids
    }

    fn write_index(&self, ids: &[String]) -> Result<()> {
        let json = serde_json::to_string(ids).expect("serialisable");
        self.secrets.set(INDEX_KEY, &json)?;
        Ok(())
    }
}

fn read_index(secrets: &dyn SecretStore) -> Result<Vec<String>> {
    match secrets.get(INDEX_KEY)? {
        Some(json) => {
            serde_json::from_str(&json).map_err(|e| Error::Storage(format!("lan/peers: {e}")))
        }
        None => Ok(Vec::new()),
    }
}
