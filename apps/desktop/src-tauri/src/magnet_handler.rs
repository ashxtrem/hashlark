// SPDX-License-Identifier: GPL-3.0-or-later

//! Detects whether the OS has an app registered for `magnet:` links, so the
//! UI can explain what to do instead of the OS showing a confusing prompt.

/// `Some(true/false)` when known, `None` when it can't be determined.
#[cfg(windows)]
pub fn registered() -> Option<bool> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};

    // Classic protocol registration (qBittorrent, Transmission, Deluge, ...).
    let classic = RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey(r"magnet\shell\open\command")
        .is_ok();
    // A per-user choice made in Windows Settings (covers Store apps).
    let user_choice = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(
            r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\magnet\UserChoice",
        )
        .is_ok();
    Some(classic || user_choice)
}

#[cfg(target_os = "linux")]
pub fn registered() -> Option<bool> {
    let output = std::process::Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/magnet"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn registered() -> Option<bool> {
    None
}
