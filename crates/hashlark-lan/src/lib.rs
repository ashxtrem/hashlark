// SPDX-License-Identifier: GPL-3.0-or-later

//! Sending results between Hashlark devices on the local network.
//!
//! A phone finds the desktop with mDNS, pairs with it once using a PIN shown
//! on the desktop, and from then on can send it magnet links, which the
//! desktop opens in its torrent client (and the other way round).
//!
//! - **Pairing:** SPAKE2 keyed by the 6-digit PIN, then HKDF; both devices
//!   keep the resulting key in their secret store.
//! - **Sending:** the link is sealed with ChaCha20-Poly1305 under that key,
//!   with a timestamp and a nonce the receiver remembers, so it can't be
//!   read, forged or replayed by others on the network.
//! - **Listener:** a small HTTP server separate from the app's API, with
//!   four routes under `/lan/v1` and per-address rate limits.
//!
//! The crate is used directly by the desktop app and through `hashlark-ffi`
//! by the Android app.

mod client;
mod crypto;
mod discovery;
mod error;
mod pairing;
mod protocol;
mod server;
mod service;
mod store;

use std::net::{IpAddr, Ipv4Addr};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub use discovery::SERVICE_TYPE;
pub use error::{Error, Result};
pub use pairing::{MAX_FAILURES, PairingCode, WINDOW as PAIRING_WINDOW};
pub use protocol::{Hello, PROTOCOL_VERSION};
pub use service::LanService;

/// Port tried first; another is used if it's taken (mDNS advertises the
/// real one, and typed addresses without a port assume this one).
pub const DEFAULT_PORT: u16 = 47821;

/// Longest magnet link accepted.
pub const MAX_LINK_LEN: usize = 16 * 1024;

/// What kind of app a device runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Desktop,
    Android,
}

/// How to run the service.
#[derive(Debug, Clone)]
pub struct LanConfig {
    /// Shown to other devices (e.g. the computer's or phone's name).
    pub name: String,
    pub platform: Platform,
    /// Address to listen on; all interfaces by default.
    pub bind: IpAddr,
    /// Port to try first; 0 for any.
    pub port: u16,
    /// Advertise and browse with mDNS.
    pub discovery: bool,
}

impl LanConfig {
    pub fn new(name: impl Into<String>, platform: Platform) -> Self {
        Self {
            name: name.into(),
            platform,
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: DEFAULT_PORT,
            discovery: true,
        }
    }
}

/// Another device, paired or just seen on the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    /// This device holds a key for it, so links can be sent.
    pub paired: bool,
    /// Currently announcing itself on the network (mDNS).
    pub online: bool,
    /// `ip:port` it was last seen or reached at.
    pub addr: Option<String>,
}

/// A link another device sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Received {
    pub from: Device,
    /// A `magnet:?` link.
    pub url: String,
    pub title: String,
}

/// What the app hears about. Called in order on a dedicated thread, so a
/// slow callback delays later events but never the network.
pub trait LanEvents: Send + Sync {
    /// A paired device sent a link. The app opens it (or offers to).
    fn received(&self, item: Received);
    /// The device list changed (found, lost, paired or forgotten).
    fn devices_changed(&self) {}
    /// Pairing finished; on the PIN side this means the window has closed.
    fn paired(&self, _device: Device) {}
    /// The PIN on screen changed (opened, rotated after wrong attempts) or
    /// pairing closed (`None`).
    fn pairing_changed(&self, _code: Option<PairingCode>) {}
}

/// Checks that `url` is a magnet link this crate will carry.
pub fn validate_link(url: &str) -> Result<()> {
    if !url.starts_with("magnet:?") {
        return Err(Error::Invalid("only magnet links can be sent".into()));
    }
    if url.len() > MAX_LINK_LEN {
        return Err(Error::Invalid("the magnet link is too long".into()));
    }
    if url.chars().any(char::is_control) {
        return Err(Error::Invalid("the magnet link is malformed".into()));
    }
    Ok(())
}

/// This device's IPv4 addresses on local networks, private ranges first, to
/// show next to the port for typing on another device.
pub fn local_addresses() -> Vec<IpAddr> {
    let mut addrs: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.is_oper_up() && !i.is_loopback() && !i.is_link_local())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })
        .collect();
    addrs.sort_by_key(|ip| (!ip.is_private(), *ip));
    addrs.dedup();
    addrs.into_iter().map(IpAddr::V4).collect()
}

/// Trims a device name to something safe to show.
pub(crate) fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    let name = name.trim();
    if name.is_empty() {
        "Hashlark device".to_owned()
    } else {
        name.to_owned()
    }
}

/// Trims a result title to something safe to show.
pub(crate) fn clean_title(title: &str) -> String {
    let title: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(300)
        .collect();
    title.trim().to_owned()
}

pub(crate) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub(crate) fn now_secs() -> i64 {
    now_millis() / 1000
}

#[cfg(test)]
mod tests;
