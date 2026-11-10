// SPDX-License-Identifier: GPL-3.0-or-later

//! Built-in BitTorrent client for Hashlark, on top of librqbit
//! (plan `docs/torrent-client-plan.md` §3).
//!
//! [`TorrentService`] owns the librqbit session; its operations live in
//! `ops`, the JSON types in [`model`] and the folder rules in [`paths`].

pub mod model;
mod ops;
pub mod paths;
mod service;
mod store;

pub use paths::PathPolicy;
pub use service::{ServiceOptions, TorrentService};
