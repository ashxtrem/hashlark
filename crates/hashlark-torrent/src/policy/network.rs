// SPDX-License-Identifier: GPL-3.0-or-later

//! The network gate (plan §3.3). `network_allowed` is true everywhere except
//! Android, where the app sets it from `ConnectivityManager` when
//! `unmetered_only` is on.

use super::Desired;

/// Holds every torrent while the network is not allowed.
pub(crate) fn gate(network_allowed: bool) -> Option<Desired> {
    (!network_allowed).then_some(Desired::NetworkWait)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_holds_only_when_not_allowed() {
        assert_eq!(gate(false), Some(Desired::NetworkWait));
        assert_eq!(gate(true), None);
    }
}
