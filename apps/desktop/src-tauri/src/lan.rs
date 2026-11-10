// SPDX-License-Identifier: GPL-3.0-or-later

//! Sharing results with devices on the local network (`hashlark-lan`,
//! docs/lan-sharing-plan.md): sending a result to a paired phone, and opening
//! links paired phones send in the torrent client.
//!
//! Off by default. The on/off choice lives in `<data dir>/lan.json`; the
//! pairings live in the keychain with the other secrets.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use hashlark_core::secrets::SecretStore;
use hashlark_lan::{Device, LanConfig, LanEvents, LanService, PairingCode, Platform, Received};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::commands::{self, OpenOutcome};

/// Event names the UI listens for.
pub mod event {
    /// A paired device sent a link: [`super::ReceivedEvent`].
    pub const RECEIVED: &str = "lan-received";
    /// The device list changed (no payload).
    pub const DEVICES: &str = "lan-devices";
    /// Pairing finished: the paired [`hashlark_lan::Device`].
    pub const PAIRED: &str = "lan-paired";
    /// The PIN changed or pairing closed: `PairingCode | null`.
    pub const PAIRING: &str = "lan-pairing";
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct LanSettings {
    enabled: bool,
}

/// What the Settings page shows.
#[derive(Debug, Serialize)]
pub struct LanStatus {
    enabled: bool,
    running: bool,
    /// This computer's name as phones see it.
    name: Option<String>,
    /// `ip:port` pairs to type on a phone when discovery doesn't work.
    addresses: Vec<String>,
    /// Why automatic discovery isn't running, if it failed.
    discovery_error: Option<String>,
    pairing: Option<PairingCode>,
    /// Why sharing couldn't start.
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceivedEvent {
    from: String,
    title: String,
    url: String,
    /// What happened when it was handed to the torrent client.
    outcome: ReceivedOutcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum ReceivedOutcome {
    Opened,
    NoHandler,
    Failed { message: String },
}

/// Opens received links and forwards the rest to the UI.
struct Events {
    app: AppHandle,
}

impl LanEvents for Events {
    fn received(&self, item: Received) {
        let outcome = match commands::hand_off_magnet(&self.app, &item.url) {
            Ok(OpenOutcome::Opened) => ReceivedOutcome::Opened,
            Ok(OpenOutcome::NoHandler) => ReceivedOutcome::NoHandler,
            Err(message) => ReceivedOutcome::Failed { message },
        };
        let payload = ReceivedEvent {
            from: item.from.name,
            title: item.title,
            url: item.url,
            outcome,
        };
        let _ = self.app.emit(event::RECEIVED, payload);
    }

    fn devices_changed(&self) {
        let _ = self.app.emit(event::DEVICES, ());
    }

    fn paired(&self, device: Device) {
        let _ = self.app.emit(event::PAIRED, device);
    }

    fn pairing_changed(&self, code: Option<PairingCode>) {
        let _ = self.app.emit(event::PAIRING, code);
    }
}

/// LAN sharing state, managed by Tauri.
pub struct Lan {
    app: AppHandle,
    secrets: Arc<dyn SecretStore>,
    settings_path: PathBuf,
    settings: StdMutex<LanSettings>,
    /// Shared so a slow send doesn't hold the lock that status reads need.
    service: Mutex<Option<Arc<LanService>>>,
    error: StdMutex<Option<String>>,
}

impl std::fmt::Debug for Lan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lan")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl Lan {
    pub fn new(app: AppHandle, secrets: Arc<dyn SecretStore>, data_dir: &Path) -> Self {
        let settings_path = data_dir.join("lan.json");
        let settings = match std::fs::read(&settings_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "ignoring an unreadable lan.json");
                LanSettings::default()
            }),
            Err(_) => LanSettings::default(),
        };
        Self {
            app,
            secrets,
            settings_path,
            settings: StdMutex::new(settings),
            service: Mutex::new(None),
            error: StdMutex::new(None),
        }
    }

    fn enabled(&self) -> bool {
        self.settings.lock().expect("lock").enabled
    }

    /// Starts sharing at launch if the user turned it on.
    pub async fn start_if_enabled(&self) {
        if self.enabled() {
            self.start().await;
        }
    }

    async fn start(&self) {
        let mut service = self.service.lock().await;
        if service.is_some() {
            return;
        }
        let config = LanConfig::new(device_name(), Platform::Desktop);
        let events = Arc::new(Events {
            app: self.app.clone(),
        });
        let started = LanService::start(config, Arc::clone(&self.secrets), events).await;
        *self.error.lock().expect("lock") = started.as_ref().err().map(ToString::to_string);
        match started {
            Ok(started) => *service = Some(Arc::new(started)),
            Err(e) => tracing::error!(error = %e, "LAN sharing could not start"),
        }
    }

    /// Stops sharing (on exit or when turned off).
    pub async fn stop(&self) {
        if let Some(service) = self.service.lock().await.take() {
            service.shutdown().await;
        }
    }

    async fn status(&self) -> LanStatus {
        let service = self.service.lock().await;
        let error = self.error.lock().expect("lock").clone();
        match service.as_ref() {
            Some(s) => LanStatus {
                enabled: self.enabled(),
                running: true,
                name: Some(s.identity().name),
                addresses: hashlark_lan::local_addresses()
                    .into_iter()
                    .map(|ip| format!("{ip}:{}", s.port()))
                    .collect(),
                discovery_error: s.discovery_error().map(str::to_owned),
                pairing: s.pairing(),
                error,
            },
            None => LanStatus {
                enabled: self.enabled(),
                running: false,
                name: None,
                addresses: Vec::new(),
                discovery_error: None,
                pairing: None,
                error,
            },
        }
    }

    fn save_settings(&self, settings: &LanSettings) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&self.settings_path, json).map_err(|e| e.to_string())
    }
}

/// The computer's name, e.g. "Ashish's PC".
fn device_name() -> String {
    whoami::devicename()
        .ok()
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Hashlark desktop".to_owned())
}

/// A failed LAN command, with the stable code the UI acts on (`wrong_pin`,
/// `rejected`, `unreachable`, `not_running`, ...).
#[derive(Debug, Serialize)]
pub struct CommandError {
    code: &'static str,
    message: String,
}

impl From<hashlark_lan::Error> for CommandError {
    fn from(err: hashlark_lan::Error) -> Self {
        Self {
            code: err.code(),
            message: err.to_string(),
        }
    }
}

/// Runs `f` against the service, or fails if sharing is off. The lock is
/// released before `f` runs.
async fn with_service<T>(
    lan: &Lan,
    f: impl AsyncFnOnce(&LanService) -> hashlark_lan::Result<T>,
) -> Result<T, CommandError> {
    let service = lan.service.lock().await.clone().ok_or(CommandError {
        code: "not_running",
        message: "turn on Settings → Devices on this network first".into(),
    })?;
    Ok(f(&service).await?)
}

#[tauri::command]
pub async fn lan_status(lan: State<'_, Lan>) -> Result<LanStatus, String> {
    Ok(lan.status().await)
}

/// Turns sharing on or off and remembers the choice.
#[tauri::command]
pub async fn lan_set_enabled(lan: State<'_, Lan>, enabled: bool) -> Result<LanStatus, String> {
    let settings = LanSettings { enabled };
    lan.save_settings(&settings)?;
    *lan.settings.lock().expect("lock") = settings;
    if enabled {
        lan.start().await;
    } else {
        lan.stop().await;
        *lan.error.lock().expect("lock") = None;
    }
    Ok(lan.status().await)
}

/// Paired devices and devices seen on the network.
#[tauri::command]
pub async fn lan_devices(lan: State<'_, Lan>) -> Result<Vec<Device>, String> {
    Ok(lan
        .service
        .lock()
        .await
        .as_ref()
        .map(|s| s.devices())
        .unwrap_or_default())
}

/// Shows a PIN a phone can pair with for the next two minutes.
#[tauri::command]
pub async fn lan_open_pairing(lan: State<'_, Lan>) -> Result<PairingCode, CommandError> {
    with_service(&lan, async |s| Ok(s.open_pairing())).await
}

#[tauri::command]
pub async fn lan_close_pairing(lan: State<'_, Lan>) -> Result<(), CommandError> {
    with_service(&lan, async |s| {
        s.close_pairing();
        Ok(())
    })
    .await
}

/// Forgets a paired device; it can no longer send here until paired again.
#[tauri::command]
pub async fn lan_forget(lan: State<'_, Lan>, device_id: String) -> Result<(), CommandError> {
    with_service(&lan, async |s| s.forget(&device_id).await).await
}

/// Adds a device by address (`192.168.1.20:47821`) when discovery can't see it.
#[tauri::command]
pub async fn lan_add_address(lan: State<'_, Lan>, address: String) -> Result<Device, CommandError> {
    with_service(&lan, async |s| s.add_address(&address).await).await
}

/// Pairs with a device using the PIN it shows.
#[tauri::command]
pub async fn lan_pair(
    lan: State<'_, Lan>,
    device_id: String,
    pin: String,
) -> Result<Device, CommandError> {
    with_service(&lan, async |s| s.pair(&device_id, &pin).await).await
}

/// Sends a magnet link to a paired device, which opens it in its torrent app.
#[tauri::command]
pub async fn lan_send(
    lan: State<'_, Lan>,
    device_id: String,
    url: String,
    title: String,
) -> Result<(), CommandError> {
    with_service(&lan, async |s| {
        s.send_magnet(&device_id, &url, &title).await
    })
    .await
}
