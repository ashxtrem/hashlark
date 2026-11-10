// SPDX-License-Identifier: GPL-3.0-or-later

//! Desktop-only commands. Everything else goes through the HTTP API, so the
//! same UI also works in a browser against the headless server.

use std::path::{Path, PathBuf};

use hashlark_core::engine::{Session, SessionCookie};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::Backend;
use crate::magnet_handler::{self, HandlerStatus};

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

/// Hands a magnet link to the app the OS has registered for it. ADR 0008's
/// OS hand-off is now the path for when the built-in client is off (plan
/// `docs/torrent-client-plan.md` §2); with it on, the UI adds magnets
/// through the HTTP API instead.
#[tauri::command]
pub fn open_magnet(app: AppHandle, url: String) -> Result<OpenOutcome, String> {
    hand_off_magnet(&app, &url)
}

/// Opens `url` in the registered torrent client. Also used for links that
/// arrive from a paired phone while the built-in client is off. When the
/// registered handler is Hashlark itself, that's `NoHandler`: handing the
/// link over would only bring it back here.
pub fn hand_off_magnet(app: &AppHandle, url: &str) -> Result<OpenOutcome, String> {
    if !url.starts_with("magnet:?") {
        return Err("not a magnet link".into());
    }
    if magnet_handler::registered().is_some_and(|s| !s.other_app()) {
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

/// Shows a saved file in the file manager. Limited to the folder of saved
/// `.torrent` files, the torrent client's download folder and the folders
/// its torrents save into.
#[tauri::command]
pub async fn reveal_path(
    app: AppHandle,
    backend: State<'_, Backend>,
    path: String,
) -> Result<(), String> {
    let path = PathBuf::from(path);
    let mut allowed = vec![
        backend.engine.download_dir(),
        backend.torrents.download_dir(),
    ];
    if let Ok(list) = backend.torrents.list().await {
        allowed.extend(list.into_iter().map(|t| t.save_dir));
    }
    if !inside_any(&path, &allowed) {
        return Err("only files in the downloads folders can be shown".into());
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

/// Whether `path` (resolved) lies in one of the folders.
fn inside_any(path: &Path, dirs: &[PathBuf]) -> bool {
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    dirs.iter()
        .filter_map(|d| d.canonicalize().ok())
        .any(|dir| path.starts_with(dir))
}

/// Whether an app is registered for magnet links and whether it is
/// Hashlark (`null` if unknown).
#[tauri::command]
pub fn magnet_handler_status() -> Option<HandlerStatus> {
    magnet_handler::registered()
}

/// Makes Hashlark the `magnet:` handler, or stops being it (Windows and
/// Linux). On macOS the user picks the default app; the bundle lists the
/// scheme. Returns the new status.
#[tauri::command]
pub fn magnet_handler_register(
    app: AppHandle,
    register: bool,
) -> Result<Option<HandlerStatus>, String> {
    let deep_link = app.deep_link();
    let result = if register {
        deep_link.register("magnet")
    } else {
        deep_link.unregister("magnet")
    };
    result.map_err(|e| match e {
        tauri_plugin_deep_link::Error::UnsupportedPlatform => {
            "choose Hashlark as the app for magnet links in your system settings".to_owned()
        }
        e => e.to_string(),
    })?;
    tracing::info!(register, "magnet handler");
    Ok(magnet_handler::registered())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveals_only_inside_the_folders() {
        let root = std::env::temp_dir().join(format!("hashlark-reveal-{}", std::process::id()));
        let a = root.join("a");
        let b = root.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let file = b.join("x.bin");
        std::fs::write(&file, b"").unwrap();
        assert!(!inside_any(&file, std::slice::from_ref(&a)));
        assert!(inside_any(&file, &[a.clone(), b.clone()]));
        assert!(!inside_any(&a.join("../b/x.bin"), std::slice::from_ref(&a)));
        assert!(!inside_any(&a.join("missing"), std::slice::from_ref(&a)));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
