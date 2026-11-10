// SPDX-License-Identifier: GPL-3.0-or-later

//! Sending results to (and receiving them from) Hashlark on other devices on
//! the local network: a thin layer over `hashlark-lan` with JSON payloads,
//! like the rest of this crate.
//!
//! JSON shapes: `Device` `{id, name, platform, paired, online, addr}`,
//! `PairingCode` `{pin, expires_at}`, `Received` `{from: Device, url, title}`,
//! `Hello` `{id, name, platform, port, v}`.

use std::future::Future;
use std::sync::Arc;

use hashlark_lan::{Device, LanConfig, LanEvents, LanService, PairingCode, Platform, Received};
use serde::Serialize;

use crate::{ForeignSecrets, HashlarkError, SecretStore, runtime, to_json};

impl From<hashlark_lan::Error> for HashlarkError {
    fn from(err: hashlark_lan::Error) -> Self {
        // The LAN codes (`wrong_pin`, `pairing_closed`, `unreachable`, ...)
        // pass through so the app can react to each.
        Self::new(err.code(), err.to_string())
    }
}

/// What the app hears about, as JSON. Called in order on a background
/// thread; implementations should return quickly.
#[uniffi::export(with_foreign)]
pub trait LanListener: Send + Sync {
    /// A paired device sent a link: a `Received`.
    fn on_received(&self, received_json: String);
    /// The device list changed.
    fn on_devices_changed(&self);
    /// Pairing finished: the paired `Device`.
    fn on_paired(&self, device_json: String);
    /// The PIN changed (a `PairingCode`) or pairing closed (`None`).
    fn on_pairing_changed(&self, code_json: Option<String>);
}

struct ForeignEvents(Arc<dyn LanListener>);

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("serialisable")
}

impl LanEvents for ForeignEvents {
    fn received(&self, item: Received) {
        self.0.on_received(json(&item));
    }
    fn devices_changed(&self) {
        self.0.on_devices_changed();
    }
    fn paired(&self, device: Device) {
        self.0.on_paired(json(&device));
    }
    fn pairing_changed(&self, code: Option<PairingCode>) {
        self.0.on_pairing_changed(code.as_ref().map(json));
    }
}

/// Starts LAN sharing for this device, named `name` (e.g. "Pixel 9").
/// Pairings are kept in `secrets`, the same store the engine uses.
#[uniffi::export]
pub async fn open_lan(
    name: String,
    secrets: Arc<dyn SecretStore>,
    listener: Arc<dyn LanListener>,
) -> Result<Arc<HashlarkLan>, HashlarkError> {
    let service = run(async move {
        LanService::start(
            LanConfig::new(name, Platform::Android),
            Arc::new(ForeignSecrets(secrets)),
            Arc::new(ForeignEvents(listener)),
        )
        .await
    })
    .await?;
    Ok(Arc::new(HashlarkLan { service }))
}

/// Like [`open_lan`] but on loopback, without mDNS, for tests.
#[cfg(test)]
pub(crate) async fn open_test_lan(
    name: &str,
    secrets: Arc<dyn SecretStore>,
    listener: Arc<dyn LanListener>,
) -> Result<Arc<HashlarkLan>, HashlarkError> {
    let config = LanConfig {
        bind: std::net::Ipv4Addr::LOCALHOST.into(),
        port: 0,
        discovery: false,
        ..LanConfig::new(name, Platform::Android)
    };
    let service = run(async move {
        LanService::start(
            config,
            Arc::new(ForeignSecrets(secrets)),
            Arc::new(ForeignEvents(listener)),
        )
        .await
    })
    .await?;
    Ok(Arc::new(HashlarkLan { service }))
}

/// This device's IPv4 addresses on local networks.
#[uniffi::export]
pub fn lan_local_addresses() -> Vec<String> {
    hashlark_lan::local_addresses()
        .into_iter()
        .map(|ip| ip.to_string())
        .collect()
}

async fn run<T, F>(future: F) -> Result<T, HashlarkError>
where
    T: Send + 'static,
    F: Future<Output = hashlark_lan::Result<T>> + Send + 'static,
{
    runtime()
        .spawn(future)
        .await
        .map_err(|e| HashlarkError::internal(format!("task failed: {e}")))?
        .map_err(Into::into)
}

/// LAN sharing while the app is open. Call [`shutdown`](Self::shutdown)
/// when the app goes to the background.
#[derive(Debug, uniffi::Object)]
pub struct HashlarkLan {
    service: LanService,
}

#[uniffi::export]
impl HashlarkLan {
    /// This device as others see it: a `Hello`.
    pub fn identity(&self) -> Result<String, HashlarkError> {
        to_json(&self.service.identity())
    }

    /// Why automatic discovery isn't running, if it failed to start.
    pub fn discovery_error(&self) -> Option<String> {
        self.service.discovery_error().map(str::to_owned)
    }

    /// Paired devices and devices seen on the network: a list of `Device`.
    pub fn devices(&self) -> Result<String, HashlarkError> {
        to_json(&self.service.devices())
    }

    /// Adds a device by address (`192.168.1.20` or `192.168.1.20:47821`).
    /// Returns the `Device`.
    pub async fn add_address(self: Arc<Self>, address: String) -> Result<String, HashlarkError> {
        let device = run(async move { self.service.add_address(&address).await }).await?;
        to_json(&device)
    }

    /// Opens pairing on this device; returns the `PairingCode` to show.
    pub fn open_pairing(&self) -> Result<String, HashlarkError> {
        to_json(&self.service.open_pairing())
    }

    pub fn close_pairing(&self) {
        self.service.close_pairing();
    }

    /// The open `PairingCode`, if any.
    pub fn pairing(&self) -> Result<Option<String>, HashlarkError> {
        self.service.pairing().as_ref().map(to_json).transpose()
    }

    /// Pairs with `device_id` using the PIN it shows. Returns the `Device`.
    pub async fn pair(
        self: Arc<Self>,
        device_id: String,
        pin: String,
    ) -> Result<String, HashlarkError> {
        let device = run(async move { self.service.pair(&device_id, &pin).await }).await?;
        to_json(&device)
    }

    pub async fn forget(self: Arc<Self>, device_id: String) -> Result<(), HashlarkError> {
        run(async move { self.service.forget(&device_id).await }).await
    }

    /// Sends a magnet link to a paired device, which opens it.
    pub async fn send_magnet(
        self: Arc<Self>,
        device_id: String,
        url: String,
        title: String,
    ) -> Result<(), HashlarkError> {
        run(async move { self.service.send_magnet(&device_id, &url, &title).await }).await
    }

    /// Stops listening and advertising.
    pub async fn shutdown(self: Arc<Self>) -> Result<(), HashlarkError> {
        run(async move {
            self.service.shutdown().await;
            Ok(())
        })
        .await
    }
}
