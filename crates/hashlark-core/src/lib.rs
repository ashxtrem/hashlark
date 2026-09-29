// SPDX-License-Identifier: GPL-3.0-or-later

//! Hashlark core: providers, aggregation, definitions, networking and
//! storage. Front ends (HTTP server, CLI, Android FFI) build on this crate,
//! usually through [`Engine`].

pub mod aggregator;
pub mod definition;
pub mod engine;
pub mod error;
pub mod health;
pub mod instrument;
pub mod magnet;
pub mod model;
pub mod net;
pub mod paths;
pub mod provider;
pub mod ranking;
pub mod registry;
pub mod repos;
pub mod secrets;
pub mod settings;
pub mod store;
mod util;

pub use aggregator::{Aggregator, AggregatorConfig, BoundProvider, SearchEvent, SearchOutcome};
pub use engine::{Engine, EngineOptions, ProviderPatch, ProviderView, TestReport};
pub use error::{Error, ErrorKind, ProviderError, Result};
pub use model::{
    Category, DownloadTarget, InfoHash, MergedResult, ProviderId, SearchQuery, SearchResult,
    SortOrder,
};
pub use paths::AppPaths;
pub use provider::{ProviderCtx, SearchProvider, builtin_providers};
pub use settings::Settings;
pub use store::Store;
pub use tokio_util::sync::CancellationToken;

/// Crate version, shared by every Hashlark binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
