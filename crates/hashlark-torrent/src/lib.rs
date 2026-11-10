// SPDX-License-Identifier: GPL-3.0-or-later

//! Built-in BitTorrent client for Hashlark, on top of librqbit
//! (plan `docs/torrent-client-plan.md` §3).
//!
//! [`TorrentService`] owns the librqbit session; its operations live in
//! `ops`, the JSON types in [`model`] and the folder rules in [`paths`].

mod clock;
pub mod model;
mod ops;
pub mod paths;
mod policy;
mod service;
mod store;
mod watch;

pub use clock::{Clock, Moment, system_clock};
pub use paths::PathPolicy;
pub use policy::schedule::LocalTime;
pub use service::{ServiceOptions, TorrentService};
