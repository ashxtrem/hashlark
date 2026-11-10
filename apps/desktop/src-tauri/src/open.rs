// SPDX-License-Identifier: GPL-3.0-or-later

//! Magnets and `.torrent` files the OS opens with Hashlark (plan
//! `docs/torrent-client-plan.md` §6): from the command line of the first
//! launch, a second launch (single instance) or, on macOS, an "open" event.
//!
//! Each one becomes a `torrent-open` event. Events that arrive before the UI
//! listens are kept until it calls [`take_torrent_opens`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;

use crate::Backend;

/// Tauri event with a [`TorrentOpen`].
pub const EVENT: &str = "torrent-open";

/// What was opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenTarget {
    Magnet(String),
    /// An absolute path to a `.torrent` file.
    Path(PathBuf),
}

/// Payload of `torrent-open`: `{ magnet | path, client_on }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TorrentOpen {
    #[serde(flatten)]
    pub target: OpenTarget,
    /// The built-in client runs. When it doesn't, the UI shows the
    /// no-torrent-client dialog with "Turn on the built-in client" instead
    /// of handing the link back to the OS (which would reach us again).
    pub client_on: bool,
}

/// Picks magnets and `.torrent` paths out of a command line. `args[0]` is
/// the program; relative paths are resolved against `cwd`.
pub fn open_args(args: &[String], cwd: &Path) -> Vec<OpenTarget> {
    args.iter()
        .skip(1)
        .filter_map(|arg| parse_arg(arg, cwd))
        .collect()
}

fn parse_arg(arg: &str, cwd: &Path) -> Option<OpenTarget> {
    let arg = arg.trim();
    if arg.is_empty() || arg.starts_with('-') {
        return None;
    }
    if arg
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("magnet:"))
    {
        return Some(OpenTarget::Magnet(format!("magnet:{}", &arg[7..])));
    }
    let path = if arg
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("file://"))
    {
        Url::parse(arg).ok()?.to_file_path().ok()?
    } else {
        PathBuf::from(arg)
    };
    let is_torrent = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("torrent"));
    if !is_torrent {
        return None;
    }
    Some(OpenTarget::Path(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }))
}

/// Opens that arrived before the UI was ready.
#[derive(Debug, Default)]
pub struct Opens {
    inner: Mutex<OpensInner>,
}

#[derive(Debug, Default)]
struct OpensInner {
    ui_ready: bool,
    pending: Vec<TorrentOpen>,
}

/// Emits `torrent-open` for each target (or keeps it for the UI) and brings
/// the window forward.
pub fn deliver(app: &AppHandle, targets: Vec<OpenTarget>) {
    if targets.is_empty() {
        return;
    }
    let client_on = app
        .try_state::<Backend>()
        .is_some_and(|b| b.torrents.is_running());
    let Some(opens) = app.try_state::<Opens>() else {
        return;
    };
    {
        let mut inner = opens.inner.lock().expect("lock");
        for target in targets {
            let open = TorrentOpen { target, client_on };
            tracing::info!(?open, "opened with Hashlark");
            if inner.ui_ready {
                let _ = app.emit(EVENT, open);
            } else {
                inner.pending.push(open);
            }
        }
    }
    crate::show_main_window(app);
}

/// Handles a command line (first launch or a second instance).
pub fn handle_args(app: &AppHandle, args: &[String], cwd: &Path) {
    deliver(app, open_args(args, cwd));
}

/// Opens that arrived before the UI listened. From then on they come as
/// `torrent-open` events.
#[tauri::command]
pub fn take_torrent_opens(opens: State<'_, Opens>) -> Vec<TorrentOpen> {
    let mut inner = opens.inner.lock().expect("lock");
    inner.ui_ready = true;
    std::mem::take(&mut inner.pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("hashlark")
            .chain(list.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    fn cwd() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn reads_a_magnet() {
        let m = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=x";
        assert_eq!(
            open_args(&args(&[m]), &cwd()),
            vec![OpenTarget::Magnet(m.to_owned())]
        );
        assert_eq!(
            open_args(&args(&["MAGNET:?xt=1"]), &cwd()),
            vec![OpenTarget::Magnet("magnet:?xt=1".to_owned())]
        );
    }

    #[test]
    fn reads_torrent_paths() {
        let abs = cwd().join("a.torrent");
        assert_eq!(
            open_args(&args(&[&abs.to_string_lossy()]), &cwd()),
            vec![OpenTarget::Path(abs.clone())]
        );
        assert_eq!(
            open_args(&args(&["sub/B.TORRENT"]), &cwd()),
            vec![OpenTarget::Path(cwd().join("sub/B.TORRENT"))]
        );
        let url = Url::from_file_path(&abs).unwrap();
        assert_eq!(
            open_args(&args(&[url.as_str()]), &cwd()),
            vec![OpenTarget::Path(abs)]
        );
    }

    #[test]
    fn ignores_the_program_flags_and_other_files() {
        assert!(open_args(&args(&[]), &cwd()).is_empty());
        assert!(open_args(&args(&["--flag", "notes.txt", "https://x.org", ""]), &cwd()).is_empty());
        // The program itself is never taken, even if it looked like a torrent.
        assert!(open_args(&["x.torrent".to_owned()], &cwd()).is_empty());
    }

    #[test]
    fn serialises_as_magnet_or_path() {
        let open = TorrentOpen {
            target: OpenTarget::Magnet("magnet:?xt=1".into()),
            client_on: true,
        };
        assert_eq!(
            serde_json::to_value(open).unwrap(),
            serde_json::json!({ "magnet": "magnet:?xt=1", "client_on": true })
        );
        let open = TorrentOpen {
            target: OpenTarget::Path(PathBuf::from("a.torrent")),
            client_on: false,
        };
        assert_eq!(
            serde_json::to_value(open).unwrap(),
            serde_json::json!({ "path": "a.torrent", "client_on": false })
        );
    }
}
