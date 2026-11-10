// SPDX-License-Identifier: GPL-3.0-or-later

//! Power features of the built-in torrent client (plan
//! `docs/torrent-client-plan.md` §2 "Power", §6): keeping the computer
//! awake while it downloads, reporting battery power to the client, and
//! "When all done" (quit, sleep, hibernate or shut down after the last
//! download, with a cancel countdown). Also shows "Download finished"
//! notifications, since it already follows the client's events.

use std::sync::Mutex;
use std::time::Duration;

use hashlark_core::settings::KeepAwake;
use hashlark_torrent::TorrentService;
use hashlark_torrent::model::{PowerState, TorrentEvent, TorrentState, TorrentSummary};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast::error::RecvError;

use crate::Backend;

/// Event names the UI listens for.
pub mod event {
    /// The countdown started: [`super::Countdown`].
    pub const COUNTDOWN: &str = "when-done-countdown";
    /// The choice changed, fired or was cancelled: [`super::WhenDoneStatus`].
    pub const CHANGED: &str = "when-done-changed";
}

/// Length of the "When all done" countdown.
const COUNTDOWN_SECS: u64 = 60;
/// Overrides [`COUNTDOWN_SECS`], for checking the feature by hand.
const COUNTDOWN_ENV: &str = "HASHLARK_WHEN_DONE_SECS";
/// How often the battery is checked.
const BATTERY_POLL: Duration = Duration::from_secs(30);
/// With no snapshot for this long the client is off: let the computer sleep.
const SNAPSHOT_GAP: Duration = Duration::from_secs(5);
/// After sleep or hibernate, wait before starting the client again.
const WAKE_DELAY: Duration = Duration::from_secs(15);

/// What to do once nothing downloads any more. Memory only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenDoneAction {
    #[default]
    Nothing,
    Quit,
    Sleep,
    Hibernate,
    Shutdown,
}

impl WhenDoneAction {
    pub const ALL: [Self; 5] = [
        Self::Nothing,
        Self::Quit,
        Self::Sleep,
        Self::Hibernate,
        Self::Shutdown,
    ];

    /// `system_shutdown` can't hibernate a Mac.
    pub fn supported(self) -> bool {
        !(self == Self::Hibernate && cfg!(target_os = "macos"))
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Nothing => "Do nothing",
            Self::Quit => "Quit Hashlark",
            Self::Sleep => "Sleep",
            Self::Hibernate => "Hibernate",
            Self::Shutdown => "Shut down",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Quit => "quit",
            Self::Sleep => "sleep",
            Self::Hibernate => "hibernate",
            Self::Shutdown => "shutdown",
        }
    }
}

// ----- pure rules -----------------------------------------------------------

/// The torrent is fetching data (or its metadata).
fn downloading(t: &TorrentSummary) -> bool {
    matches!(t.state, TorrentState::Downloading | TorrentState::Metadata)
}

/// Whether the keep-awake guard should be held.
pub fn keep_awake_wanted(mode: KeepAwake, torrents: &[TorrentSummary]) -> bool {
    match mode {
        KeepAwake::Off => false,
        KeepAwake::Downloading => torrents.iter().any(downloading),
        KeepAwake::Active => torrents
            .iter()
            .any(|t| downloading(t) || t.state == TorrentState::Seeding),
    }
}

/// "Not all done": something downloads, or waits (queued or checking) to
/// download. Seeding doesn't count.
pub fn download_pending(torrents: &[TorrentSummary]) -> bool {
    torrents.iter().any(|t| {
        downloading(t)
            || (matches!(t.state, TorrentState::Queued | TorrentState::Checking)
                && t.progress < 1.0)
    })
}

/// What a snapshot means for "When all done".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    None,
    /// Start the countdown for this action.
    Fire(WhenDoneAction),
    /// A download started again during the countdown: stop counting.
    Abort,
}

/// The arming logic of "When all done", fed one snapshot at a time.
#[derive(Debug, Default, Clone)]
pub struct WhenDone {
    pub action: WhenDoneAction,
    /// The countdown runs.
    pub counting: bool,
    /// The previous snapshot had downloads pending.
    busy: bool,
}

impl WhenDone {
    /// Feeds one snapshot. `running` is false when the client is off, which
    /// never counts as "all done".
    pub fn observe(&mut self, running: bool, busy: bool) -> Step {
        let was_busy = std::mem::replace(&mut self.busy, running && busy);
        if !running {
            return Step::None;
        }
        if self.counting {
            if busy {
                self.counting = false;
                return Step::Abort;
            }
            return Step::None;
        }
        if was_busy && !busy && self.action != WhenDoneAction::Nothing {
            self.counting = true;
            return Step::Fire(self.action);
        }
        Step::None
    }

    pub fn set(&mut self, action: WhenDoneAction) {
        self.action = action;
        self.counting = false;
    }

    /// The countdown ran out: the action runs and the choice resets.
    pub fn fired(&mut self) -> WhenDoneAction {
        let action = self.action;
        self.action = WhenDoneAction::Nothing;
        self.counting = false;
        action
    }
}

/// The countdown length, `HASHLARK_WHEN_DONE_SECS` or 60 s.
pub fn countdown_secs(env: Option<&str>) -> u64 {
    env.and_then(|s| s.trim().parse().ok())
        .filter(|&s: &u64| s > 0)
        .unwrap_or(COUNTDOWN_SECS)
}

// ----- state and commands ----------------------------------------------------

/// Power state, managed by Tauri.
#[derive(Debug, Default)]
pub struct Power {
    when_done: Mutex<WhenDone>,
    /// Bumped to cancel a running countdown.
    generation: Mutex<u64>,
}

/// Payload of `when-done-countdown`.
#[derive(Debug, Clone, Serialize)]
pub struct Countdown {
    pub action: WhenDoneAction,
    pub seconds: u64,
}

/// What the UI shows for "When all done".
#[derive(Debug, Clone, Serialize)]
pub struct WhenDoneStatus {
    pub action: WhenDoneAction,
    /// The countdown runs.
    pub counting: bool,
    /// Actions this OS supports, for the menu.
    pub actions: Vec<WhenDoneAction>,
}

impl Power {
    pub fn status(&self) -> WhenDoneStatus {
        let w = self.when_done.lock().expect("lock");
        WhenDoneStatus {
            action: w.action,
            counting: w.counting,
            actions: WhenDoneAction::ALL
                .into_iter()
                .filter(|a| a.supported())
                .collect(),
        }
    }

    fn bump(&self) -> u64 {
        let mut g = self.generation.lock().expect("lock");
        *g += 1;
        *g
    }

    fn current(&self) -> u64 {
        *self.generation.lock().expect("lock")
    }

    /// Sets the choice (cancelling a countdown) and tells the UI and tray.
    pub fn set(&self, app: &AppHandle, action: WhenDoneAction) -> Result<(), String> {
        if !action.supported() {
            return Err(format!(
                "{} isn't available on this computer",
                action.label()
            ));
        }
        self.bump();
        self.when_done.lock().expect("lock").set(action);
        tracing::info!(action = action.as_str(), "when all done");
        self.changed(app);
        Ok(())
    }

    fn changed(&self, app: &AppHandle) {
        let status = self.status();
        crate::tray::sync_when_done(app, status.action);
        let _ = app.emit(event::CHANGED, status);
    }
}

/// Chooses what happens when all downloads are done.
#[tauri::command]
pub fn set_when_done(
    app: AppHandle,
    power: State<'_, Power>,
    action: WhenDoneAction,
) -> Result<WhenDoneStatus, String> {
    power.set(&app, action)?;
    Ok(power.status())
}

/// The current "When all done" choice.
#[tauri::command]
pub fn when_done(power: State<'_, Power>) -> WhenDoneStatus {
    power.status()
}

/// Stops a running countdown and resets the choice to "Do nothing".
#[tauri::command]
pub fn cancel_when_done(app: AppHandle, power: State<'_, Power>) -> WhenDoneStatus {
    let _ = power.set(&app, WhenDoneAction::Nothing);
    power.status()
}

// ----- background tasks ------------------------------------------------------

/// Starts the event follower and the battery poll.
pub fn spawn(app: &AppHandle, torrents: TorrentService) {
    let handle = app.clone();
    let events = torrents.clone();
    tauri::async_runtime::spawn(async move { follow_events(handle, events).await });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { poll_battery(handle, torrents).await });
}

/// Follows the client's events: keep-awake, "When all done" and
/// notifications.
async fn follow_events(app: AppHandle, torrents: TorrentService) {
    let mut rx = torrents.subscribe();
    let mut awake: Option<keepawake::KeepAwake> = None;
    loop {
        let event = match tokio::time::timeout(SNAPSHOT_GAP, rx.recv()).await {
            Ok(Ok(event)) => Some(event),
            Ok(Err(RecvError::Lagged(_))) => continue,
            Ok(Err(RecvError::Closed)) => break,
            // No snapshots: the client is off.
            Err(_) => None,
        };
        match event {
            Some(TorrentEvent::Snapshot {
                session,
                torrents: list,
            }) => {
                let mode = app.state::<Backend>().engine.settings().torrents.keep_awake;
                set_awake(
                    &mut awake,
                    session.running && keep_awake_wanted(mode, &list),
                );
                crate::tray::sync_turtle(&app, session.turtle);
                let step = {
                    let power = app.state::<Power>();
                    let mut w = power.when_done.lock().expect("lock");
                    w.observe(session.running, download_pending(&list))
                };
                match step {
                    Step::None => {}
                    Step::Fire(action) => start_countdown(&app, action),
                    Step::Abort => {
                        let power = app.state::<Power>();
                        power.bump();
                        tracing::info!("a download started again; when-all-done countdown stopped");
                        power.changed(&app);
                    }
                }
            }
            Some(TorrentEvent::Completed { name, .. }) => {
                let shown = app
                    .notification()
                    .builder()
                    .title("Hashlark")
                    .body(format!("Download finished: {name}"))
                    .show();
                if let Err(err) = shown {
                    tracing::warn!(%err, "could not show a notification");
                }
            }
            Some(TorrentEvent::Error { .. }) => {}
            None => {
                set_awake(&mut awake, false);
                let power = app.state::<Power>();
                power.when_done.lock().expect("lock").observe(false, false);
            }
        }
    }
    set_awake(&mut awake, false);
}

/// Takes or drops the keep-awake guard (idle sleep only; the display may
/// still turn off).
fn set_awake(guard: &mut Option<keepawake::KeepAwake>, wanted: bool) {
    if wanted == guard.is_some() {
        return;
    }
    if wanted {
        match keepawake::Builder::default()
            .idle(true)
            .reason("Downloading torrents")
            .app_name("Hashlark")
            .app_reverse_domain("org.hashlark.desktop")
            .create()
        {
            Ok(g) => {
                tracing::info!("keep-awake on: torrents are downloading");
                *guard = Some(g);
            }
            Err(err) => tracing::warn!(%err, "could not keep the computer awake"),
        }
    } else {
        *guard = None;
        tracing::info!("keep-awake off");
    }
}

fn start_countdown(app: &AppHandle, action: WhenDoneAction) {
    let power = app.state::<Power>();
    let generation = power.bump();
    let seconds = countdown_secs(std::env::var(COUNTDOWN_ENV).ok().as_deref());
    tracing::info!(
        action = action.as_str(),
        seconds,
        "all done: countdown started"
    );
    crate::show_main_window(app);
    let _ = app.emit(event::COUNTDOWN, Countdown { action, seconds });
    power.changed(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        let power = app.state::<Power>();
        if power.current() != generation {
            tracing::info!("when-all-done countdown cancelled");
            return;
        }
        let action = power.when_done.lock().expect("lock").fired();
        power.changed(&app);
        run_action(&app, action).await;
    });
}

async fn run_action(app: &AppHandle, action: WhenDoneAction) {
    tracing::info!(action = action.as_str(), "when all done: running");
    let torrents = app.state::<Backend>().torrents.clone();
    let os_call: fn() -> std::io::Result<()> = match action {
        WhenDoneAction::Nothing => return,
        WhenDoneAction::Quit => {
            torrents.stop().await;
            app.exit(0);
            return;
        }
        WhenDoneAction::Sleep => system_shutdown::sleep,
        WhenDoneAction::Hibernate => system_shutdown::hibernate,
        WhenDoneAction::Shutdown => system_shutdown::shutdown,
    };
    // Flush fastresume and totals first.
    torrents.stop().await;
    let result = tauri::async_runtime::spawn_blocking(os_call)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))
        .and_then(|r| r);
    if let Err(err) = &result {
        tracing::error!(%err, action = action.as_str(), "when all done failed");
    }
    if action == WhenDoneAction::Shutdown && result.is_ok() {
        return;
    }
    // Back from sleep (or the call failed): run the client again.
    if result.is_ok() {
        tokio::time::sleep(WAKE_DELAY).await;
    }
    if let Err(err) = torrents.start().await {
        tracing::error!(%err, "the torrent client did not start again");
    }
}

/// Polls the battery and tells the client when it switches power source.
async fn poll_battery(_app: AppHandle, torrents: TorrentService) {
    let mut last: Option<bool> = None;
    loop {
        let on_battery = tauri::async_runtime::spawn_blocking(on_battery)
            .await
            .unwrap_or(None);
        if let Some(now) = on_battery {
            if last != Some(now) {
                tracing::info!(on_battery = now, "power source");
                torrents.set_power(PowerState { on_battery: now }).await;
                last = Some(now);
            }
        } else if last.is_none() {
            // Unknown, e.g. no battery API: treat as mains power, once.
            last = Some(false);
        }
        tokio::time::sleep(BATTERY_POLL).await;
    }
}

/// `Some(true)` when a battery discharges. Desktops have no battery, so they
/// are never on battery.
fn on_battery() -> Option<bool> {
    let manager = starship_battery::Manager::new().ok()?;
    let batteries = manager.batteries().ok()?;
    let mut states = Vec::new();
    for battery in batteries.flatten() {
        states.push(battery.state() == starship_battery::State::Discharging);
    }
    Some(states.into_iter().any(|d| d))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hashlark_torrent::model::{Origin, RunMode};

    use super::*;

    fn t(state: TorrentState, progress: f64) -> TorrentSummary {
        TorrentSummary {
            id: "0".repeat(40),
            name: "x".into(),
            state,
            run_mode: RunMode::Auto,
            source: Origin::Magnet,
            progress,
            size: 0,
            have: 0,
            downloaded_total: 0,
            uploaded_total: 0,
            ratio: 0.0,
            down_bps: 0,
            up_bps: 0,
            eta_secs: None,
            peers: 0,
            seeds: 0,
            queue_pos: 0,
            save_dir: PathBuf::new(),
            added_at: 0,
            completed_at: None,
            done: false,
            error: None,
        }
    }

    #[test]
    fn keeps_awake_only_while_downloading() {
        let dl = [t(TorrentState::Downloading, 0.5)];
        let seed = [t(TorrentState::Seeding, 1.0)];
        let idle = [t(TorrentState::Paused, 0.5), t(TorrentState::Finished, 1.0)];
        assert!(keep_awake_wanted(KeepAwake::Downloading, &dl));
        assert!(keep_awake_wanted(
            KeepAwake::Downloading,
            &[t(TorrentState::Metadata, 0.0)]
        ));
        assert!(!keep_awake_wanted(KeepAwake::Downloading, &seed));
        assert!(!keep_awake_wanted(KeepAwake::Downloading, &idle));
        assert!(!keep_awake_wanted(KeepAwake::Downloading, &[]));
        assert!(keep_awake_wanted(KeepAwake::Active, &seed));
        assert!(keep_awake_wanted(KeepAwake::Active, &dl));
        assert!(!keep_awake_wanted(KeepAwake::Active, &idle));
        assert!(!keep_awake_wanted(KeepAwake::Off, &dl));
    }

    #[test]
    fn pending_means_downloading_or_waiting_to_download() {
        assert!(download_pending(&[t(TorrentState::Downloading, 0.1)]));
        assert!(download_pending(&[t(TorrentState::Queued, 0.1)]));
        assert!(download_pending(&[t(TorrentState::Checking, 0.1)]));
        // A seed waiting for a seeding slot is done.
        assert!(!download_pending(&[t(TorrentState::Queued, 1.0)]));
        assert!(!download_pending(&[t(TorrentState::Seeding, 1.0)]));
        assert!(!download_pending(&[t(TorrentState::Paused, 0.3)]));
        assert!(!download_pending(&[]));
    }

    #[test]
    fn fires_when_the_last_download_finishes() {
        let mut w = WhenDone::default();
        w.set(WhenDoneAction::Quit);
        assert_eq!(w.observe(true, true), Step::None);
        assert_eq!(w.observe(true, true), Step::None);
        assert_eq!(w.observe(true, false), Step::Fire(WhenDoneAction::Quit));
        assert!(w.counting);
        // Stays quiet while counting and idle.
        assert_eq!(w.observe(true, false), Step::None);
        assert_eq!(w.fired(), WhenDoneAction::Quit);
        assert_eq!(w.action, WhenDoneAction::Nothing);
        // Reset: the next finish does nothing.
        assert_eq!(w.observe(true, true), Step::None);
        assert_eq!(w.observe(true, false), Step::None);
    }

    #[test]
    fn needs_a_transition_from_busy() {
        let mut w = WhenDone::default();
        // Chosen while idle: waits for a download to start and finish.
        w.set(WhenDoneAction::Shutdown);
        assert_eq!(w.observe(true, false), Step::None);
        assert_eq!(w.observe(true, false), Step::None);
        assert_eq!(w.observe(true, true), Step::None);
        assert_eq!(w.observe(true, false), Step::Fire(WhenDoneAction::Shutdown));
    }

    #[test]
    fn nothing_never_fires() {
        let mut w = WhenDone::default();
        assert_eq!(w.observe(true, true), Step::None);
        assert_eq!(w.observe(true, false), Step::None);
    }

    #[test]
    fn turning_the_client_off_is_not_done() {
        let mut w = WhenDone::default();
        w.set(WhenDoneAction::Sleep);
        assert_eq!(w.observe(true, true), Step::None);
        assert_eq!(w.observe(false, false), Step::None);
        // Back on with nothing pending: no stale transition.
        assert_eq!(w.observe(true, false), Step::None);
    }

    #[test]
    fn a_new_download_aborts_the_countdown_and_cancel_resets() {
        let mut w = WhenDone::default();
        w.set(WhenDoneAction::Sleep);
        w.observe(true, true);
        assert_eq!(w.observe(true, false), Step::Fire(WhenDoneAction::Sleep));
        assert_eq!(w.observe(true, true), Step::Abort);
        assert!(!w.counting);
        // Still armed: fires again after this download.
        assert_eq!(w.observe(true, false), Step::Fire(WhenDoneAction::Sleep));
        // Cancel = set Nothing.
        w.set(WhenDoneAction::Nothing);
        assert!(!w.counting);
        w.observe(true, true);
        assert_eq!(w.observe(true, false), Step::None);
    }

    #[test]
    fn countdown_length_can_be_overridden() {
        assert_eq!(countdown_secs(None), 60);
        assert_eq!(countdown_secs(Some("5")), 5);
        assert_eq!(countdown_secs(Some("0")), 60);
        assert_eq!(countdown_secs(Some("soon")), 60);
    }

    #[test]
    fn hibernate_is_hidden_on_macos() {
        assert_eq!(
            WhenDoneAction::Hibernate.supported(),
            !cfg!(target_os = "macos")
        );
        assert!(WhenDoneAction::Quit.supported());
        assert_eq!(
            serde_json::to_value(WhenDoneAction::Shutdown).unwrap(),
            "shutdown"
        );
    }
}
