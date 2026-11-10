// SPDX-License-Identifier: GPL-3.0-or-later

//! Detects whether the OS has an app registered for `magnet:` links, and
//! whether that app is Hashlark itself, so the UI can explain what to do
//! instead of the OS showing a confusing prompt, and so a magnet is never
//! handed back to ourselves.

use serde::Serialize;

/// Who opens `magnet:` links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HandlerStatus {
    /// Some app is registered.
    pub registered: bool,
    /// That app is Hashlark.
    pub is_self: bool,
}

impl HandlerStatus {
    /// Handing a magnet to the OS would reach another app.
    pub fn other_app(self) -> bool {
        self.registered && !self.is_self
    }
}

/// `Some(status)` when known, `None` when it can't be determined.
#[cfg(windows)]
pub fn registered() -> Option<HandlerStatus> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};

    let classes = RegKey::predef(HKEY_CLASSES_ROOT);
    // A per-user choice made in Windows Settings (covers Store apps). It wins
    // over the classic registration when present.
    let prog_id: Option<String> = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(
            r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\magnet\UserChoice",
        )
        .and_then(|key| key.get_value("ProgId"))
        .ok();
    let command_key = match &prog_id {
        Some(id) => format!(r"{id}\shell\open\command"),
        // Classic protocol registration (qBittorrent, Transmission, ...).
        None => r"magnet\shell\open\command".to_owned(),
    };
    let command: Option<String> = classes
        .open_subkey(&command_key)
        .and_then(|key| key.get_value(""))
        .ok();
    let registered = prog_id.is_some() || command.is_some();
    let is_self = command
        .as_deref()
        .and_then(command_exe)
        .zip(std::env::current_exe().ok())
        .is_some_and(|(exe, ours)| same_exe(std::path::Path::new(exe), &ours));
    Some(HandlerStatus {
        registered,
        is_self,
    })
}

#[cfg(target_os = "linux")]
pub fn registered() -> Option<HandlerStatus> {
    let output = std::process::Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/magnet"])
        .output()
        .ok()?;
    // xdg-utils exits with 4 when no default handler exists.
    if output.status.code() == Some(4) {
        return Some(HandlerStatus {
            registered: false,
            is_self: false,
        });
    }
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let id = stdout.lines().next().map(str::trim).unwrap_or_default();
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Some(HandlerStatus {
        registered: !id.is_empty(),
        is_self: is_self_desktop_id(id, &exe_name),
    })
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn registered() -> Option<HandlerStatus> {
    None
}

/// The program of a Windows `shell\open\command` value, e.g.
/// `"C:\Program Files\Hashlark\hashlark.exe" "%1"` → the quoted path.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn command_exe(command: &str) -> Option<&str> {
    let command = command.trim_start();
    if let Some(rest) = command.strip_prefix('"') {
        return rest.split('"').next().filter(|s| !s.is_empty());
    }
    // Unquoted: up to `.exe` when there is one (paths may hold spaces),
    // else up to the first space.
    let lower = command.to_ascii_lowercase();
    if let Some(end) = lower.find(".exe") {
        return Some(&command[..end + 4]);
    }
    command.split_whitespace().next()
}

/// Whether two program paths name the same file (case-insensitive on
/// Windows, symlinks resolved where possible).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn same_exe(a: &std::path::Path, b: &std::path::Path) -> bool {
    let norm = |p: &std::path::Path| {
        let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let s = p.to_string_lossy().replace('/', "\\");
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned();
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

/// Whether an `xdg-mime` default desktop id is Hashlark's: the one
/// tauri-plugin-deep-link writes (`<exe>-handler.desktop`) or the one the
/// Linux bundles install.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn is_self_desktop_id(id: &str, exe_name: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    (!exe_name.is_empty() && id == format!("{exe_name}-handler.desktop"))
        || id.eq_ignore_ascii_case("hashlark.desktop")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_program_of_an_open_command() {
        assert_eq!(
            command_exe(r#""C:\Program Files\Hashlark\hashlark.exe" "%1""#),
            Some(r"C:\Program Files\Hashlark\hashlark.exe")
        );
        assert_eq!(
            command_exe(r"C:\Program Files\qBittorrent\qbittorrent.exe %1"),
            Some(r"C:\Program Files\qBittorrent\qbittorrent.exe")
        );
        assert_eq!(command_exe("/usr/bin/foo %u"), Some("/usr/bin/foo"));
        assert_eq!(command_exe(r#""" "%1""#), None);
        assert_eq!(command_exe(""), None);
    }

    #[test]
    fn compares_programs() {
        let ours = std::env::current_exe().unwrap();
        assert!(same_exe(&ours, &ours));
        if cfg!(windows) {
            let upper = std::path::PathBuf::from(ours.to_string_lossy().to_uppercase());
            assert!(same_exe(&upper, &ours));
        }
        assert!(!same_exe(
            std::path::Path::new("/elsewhere/qbittorrent"),
            &ours
        ));
    }

    #[test]
    fn recognises_our_desktop_ids() {
        assert!(is_self_desktop_id("hashlark-handler.desktop", "hashlark"));
        assert!(is_self_desktop_id("Hashlark.desktop", "hashlark"));
        assert!(!is_self_desktop_id(
            "org.qbittorrent.qBittorrent.desktop",
            "hashlark"
        ));
        assert!(!is_self_desktop_id("", "hashlark"));
    }

    #[test]
    fn only_another_app_takes_a_magnet() {
        let s = |registered, is_self| HandlerStatus {
            registered,
            is_self,
        };
        assert!(s(true, false).other_app());
        assert!(!s(true, true).other_app());
        assert!(!s(false, false).other_app());
    }
}
