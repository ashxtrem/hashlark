// SPDX-License-Identifier: GPL-3.0-or-later

//! The tray icon (plan `docs/torrent-client-plan.md` §2 "Desktop keeps
//! running in the tray"): Show, Pause all / Resume all, Alternative speed,
//! "When all done ▸" and Quit. Closing the main window hides it here while
//! the built-in client is on and `close_to_tray` is set.

use std::path::Path;

use hashlark_torrent::model::{BulkAction, BulkRequest};
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};
use tauri_plugin_notification::NotificationExt;

use crate::Backend;
use crate::power::{Power, WhenDoneAction};

/// Sent to the UI when the window was hidden to the tray: `{ first }`.
pub const CLOSED_TO_TRAY: &str = "closed-to-tray";
/// Marker file: the "still running in the tray" notice was shown.
const NOTICE_FILE: &str = "tray-notice-shown";

const SHOW: &str = "show";
const PAUSE_ALL: &str = "pause_all";
const RESUME_ALL: &str = "resume_all";
const TURTLE: &str = "turtle";
const QUIT: &str = "quit";
const WHEN_DONE_PREFIX: &str = "when_done:";

/// Menu items whose check marks follow the app state.
pub struct Tray {
    turtle: CheckMenuItem<tauri::Wry>,
    when_done: Vec<(WhenDoneAction, CheckMenuItem<tauri::Wry>)>,
}

/// Builds the tray icon and its menu.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let turtle =
        CheckMenuItem::with_id(app, TURTLE, "Alternative speed", true, false, None::<&str>)?;
    let when_done: Vec<_> = WhenDoneAction::ALL
        .into_iter()
        .filter(|a| a.supported())
        .map(|a| {
            CheckMenuItem::with_id(
                app,
                format!("{WHEN_DONE_PREFIX}{}", a.as_str()),
                a.label(),
                true,
                a == WhenDoneAction::Nothing,
                None::<&str>,
            )
            .map(|item| (a, item))
        })
        .collect::<tauri::Result<_>>()?;
    let when_done_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = when_done
        .iter()
        .map(|(_, item)| item as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let submenu = Submenu::with_items(app, "When all done", true, &when_done_refs)?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, SHOW, "Show Hashlark", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, PAUSE_ALL, "Pause all", true, None::<&str>)?,
            &MenuItem::with_id(app, RESUME_ALL, "Resume all", true, None::<&str>)?,
            &turtle,
            &submenu,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, "Quit Hashlark", true, None::<&str>)?,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Hashlark")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(Tray { turtle, when_done });
    Ok(())
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    match id {
        SHOW => crate::show_main_window(app),
        PAUSE_ALL => bulk(app, BulkAction::Pause),
        RESUME_ALL => bulk(app, BulkAction::Resume),
        TURTLE => {
            // The click toggled the check mark; make it the new state.
            let on = app
                .try_state::<Tray>()
                .and_then(|t| t.turtle.is_checked().ok())
                .unwrap_or(false);
            bulk(app, BulkAction::Turtle(on));
        }
        QUIT => app.exit(0),
        _ => {
            if let Some(name) = id.strip_prefix(WHEN_DONE_PREFIX)
                && let Some(action) = WhenDoneAction::ALL.into_iter().find(|a| a.as_str() == name)
            {
                let power = app.state::<Power>();
                if let Err(err) = power.set(app, action) {
                    tracing::warn!(%err, "when all done");
                }
            }
        }
    }
}

fn bulk(app: &AppHandle, action: BulkAction) {
    let torrents = app.state::<Backend>().torrents.clone();
    tauri::async_runtime::spawn(async move {
        let req = BulkRequest { ids: None, action };
        if let Err(err) = torrents.bulk(req).await {
            tracing::warn!(%err, "tray action failed");
        }
    });
}

/// Mirrors turtle mode in the tray.
pub fn sync_turtle(app: &AppHandle, on: bool) {
    if let Some(tray) = app.try_state::<Tray>()
        && tray.turtle.is_checked().ok() != Some(on)
    {
        let _ = tray.turtle.set_checked(on);
    }
}

/// Mirrors the "When all done" choice in the tray (one check mark).
pub fn sync_when_done(app: &AppHandle, action: WhenDoneAction) {
    if let Some(tray) = app.try_state::<Tray>() {
        for (a, item) in &tray.when_done {
            let _ = item.set_checked(*a == action);
        }
    }
}

/// Whether closing the main window should hide it instead.
pub fn hide_on_close(close_to_tray: bool, client_enabled: bool) -> bool {
    close_to_tray && client_enabled
}

/// Hides the main window to the tray instead of closing it, when set.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    if window.label() != "main" {
        return;
    }
    let app = window.app_handle();
    let Some(backend) = app.try_state::<Backend>() else {
        return;
    };
    let settings = backend.engine.settings().torrents;
    if !hide_on_close(settings.close_to_tray, settings.enabled) {
        return;
    }
    api.prevent_close();
    let _ = window.hide();
    let first = first_time(&backend.data_dir);
    tracing::info!(first, "window hidden to the tray");
    if first {
        let _ = app
            .notification()
            .builder()
            .title("Hashlark is still running")
            .body("Downloads go on in the background. Use the tray icon to open Hashlark or quit.")
            .show();
    }
    let _ = app.emit(CLOSED_TO_TRAY, serde_json::json!({ "first": first }));
}

/// True the first time ever; leaves a marker file behind.
fn first_time(data_dir: &Path) -> bool {
    let marker = data_dir.join(NOTICE_FILE);
    if marker.exists() {
        return false;
    }
    if let Err(err) = std::fs::write(&marker, b"") {
        tracing::warn!(%err, "could not remember the tray notice");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_only_with_the_setting_and_the_client_on() {
        assert!(hide_on_close(true, true));
        assert!(!hide_on_close(false, true));
        assert!(!hide_on_close(true, false));
    }

    #[test]
    fn shows_the_notice_once() {
        let dir = std::env::temp_dir().join(format!("hashlark-tray-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join(NOTICE_FILE));
        assert!(first_time(&dir));
        assert!(!first_time(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
