// SPDX-License-Identifier: GPL-3.0-or-later

//! Desktop-only commands. Everything else goes through the HTTP API, so the
//! same UI also works in a browser against the headless server.

use std::path::PathBuf;

use hashlark_core::engine::{Session, SessionCookie};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::{Backend, magnet_handler};

#[derive(Debug, Serialize)]
pub struct ApiEndpoint {
    base_url: String,
    token: String,
}

/// Where the UI finds the in-process API.
#[tauri::command]
pub fn api_endpoint(backend: State<'_, Backend>) -> ApiEndpoint {
    ApiEndpoint {
        base_url: backend.server.base_url(),
        token: backend.server.token.clone(),
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OpenOutcome {
    Opened,
    /// No app is registered for `magnet:` links.
    NoHandler,
}

/// Hands a magnet link to the app the OS has registered for it (ADR 0008).
#[tauri::command]
pub fn open_magnet(app: AppHandle, url: String) -> Result<OpenOutcome, String> {
    if !url.starts_with("magnet:?") {
        return Err("not a magnet link".into());
    }
    if magnet_handler::registered() == Some(false) {
        return Ok(OpenOutcome::NoHandler);
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(OpenOutcome::Opened)
}

/// Opens a web page (e.g. a result's details page) in the default browser.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only web links can be opened".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Downloads a `.torrent` file into the downloads folder and, if `open` is
/// set, opens it with the default app. Returns the saved path.
#[tauri::command]
pub async fn save_torrent(
    app: AppHandle,
    backend: State<'_, Backend>,
    url: String,
    title: String,
    open: bool,
) -> Result<String, String> {
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    let path = backend
        .engine
        .save_torrent(&url, &title)
        .await
        .map_err(|e| e.to_string())?;
    if open {
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Shows a saved file in the file manager. Limited to the downloads folder.
#[tauri::command]
pub fn reveal_path(
    app: AppHandle,
    backend: State<'_, Backend>,
    path: String,
) -> Result<(), String> {
    let path = PathBuf::from(path);
    let allowed = backend.engine.download_dir();
    let inside = path
        .canonicalize()
        .ok()
        .zip(allowed.canonicalize().ok())
        .is_some_and(|(p, dir)| p.starts_with(dir));
    if !inside {
        return Err("only files in the downloads folder can be shown".into());
    }
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

/// Label of the window used for browser checks.
const CHALLENGE_WINDOW: &str = "challenge";

/// Opens a site in a separate window so the user can complete its browser
/// check ("Just a moment…"). The window has no access to Hashlark's
/// commands.
#[tauri::command]
pub async fn open_challenge(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only web pages can be opened".into());
    }
    if let Some(window) = app.get_webview_window(CHALLENGE_WINDOW) {
        window.navigate(parsed).map_err(|e| e.to_string())?;
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, CHALLENGE_WINDOW, WebviewUrl::External(parsed))
        .title("Complete the check, then return to Hashlark")
        .inner_size(1000.0, 760.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Takes the cookies the site set in the check window, gives them to the
/// provider and closes the window. `user_agent` is the webview's own user
/// agent (the same engine as the check window), which challenge cookies
/// are tied to. Returns how many cookies were stored.
#[tauri::command]
pub async fn finish_challenge(
    app: AppHandle,
    backend: State<'_, Backend>,
    provider_id: String,
    url: String,
    user_agent: String,
) -> Result<usize, String> {
    let window = app
        .get_webview_window(CHALLENGE_WINDOW)
        .ok_or("the check window was closed; open the site again")?;
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    let cookies = window
        .cookies_for_url(url.clone())
        .map_err(|e| e.to_string())?;
    let _ = window.close();
    let expires_at = cookies
        .iter()
        .filter_map(|c| c.expires_datetime())
        .map(|t| t.unix_timestamp() * 1000)
        .min();
    let cookies: Vec<SessionCookie> = cookies
        .iter()
        .map(|c| SessionCookie {
            name: c.name().to_owned(),
            value: c.value().to_owned(),
        })
        .collect();
    let count = cookies.len();
    backend
        .engine
        .set_session(
            &provider_id,
            Session {
                url,
                cookies,
                user_agent: Some(user_agent),
                expires_at,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(count)
}

/// Whether an app is registered for magnet links (`null` if unknown).
#[tauri::command]
pub fn magnet_handler_status() -> Option<bool> {
    magnet_handler::registered()
}
