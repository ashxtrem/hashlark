// SPDX-License-Identifier: GPL-3.0-or-later

//! Error types shared across the core.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Result alias for core operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Top-level error for core operations (storage, configuration, I/O).
///
/// Provider failures during a search are reported per provider as
/// [`ProviderError`] and never abort a whole search.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("database migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("unknown provider `{0}`")]
    UnknownProvider(String),

    #[error("provider `{provider}` failed: {source}")]
    Provider {
        provider: String,
        #[source]
        source: ProviderError,
    },

    /// A referenced item (result, favourite, repo, ...) doesn't exist.
    #[error("{0} not found")]
    NotFound(String),

    /// The request is well-formed but not acceptable (validation failure).
    #[error("{0}")]
    Invalid(String),

    /// A provider definition was rejected.
    #[error("invalid definition: {0}")]
    Definition(#[from] crate::definition::DefinitionError),

    /// Access to the OS keychain or secrets file failed.
    #[error("secret storage error: {0}")]
    Secrets(String),
}

/// A failure from a single provider. Each variant maps to a stable
/// [`ErrorKind`] so the UI can show a specific message and the health
/// tracker can count failures by type.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderError {
    #[error("timed out")]
    Timeout,

    #[error("blocked: {reason}")]
    Blocked { reason: String },

    #[error("site requires completing a browser challenge")]
    ChallengeRequired,

    #[error("authentication failed")]
    AuthFailed,

    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },

    #[error("could not parse field `{field}`")]
    ParseFailed { field: String },

    #[error("HTTP status {status}")]
    Http { status: u16 },

    #[error("network error: {0}")]
    Network(String),

    /// The definition itself failed at run time (template or site error).
    #[error("{0}")]
    Definition(String),
}

impl ProviderError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Timeout => ErrorKind::Timeout,
            Self::Blocked { .. } => ErrorKind::Blocked,
            Self::ChallengeRequired => ErrorKind::ChallengeRequired,
            Self::AuthFailed => ErrorKind::AuthFailed,
            Self::RateLimited { .. } => ErrorKind::RateLimited,
            Self::ParseFailed { .. } => ErrorKind::ParseFailed,
            Self::Http { .. } => ErrorKind::Http,
            Self::Network(_) => ErrorKind::Network,
            Self::Definition(_) => ErrorKind::ParseFailed,
        }
    }
}

/// Stable, serializable category of a [`ProviderError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Timeout,
    Blocked,
    ChallengeRequired,
    AuthFailed,
    RateLimited,
    ParseFailed,
    Http,
    Network,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Blocked => "blocked",
            Self::ChallengeRequired => "challenge_required",
            Self::AuthFailed => "auth_failed",
            Self::RateLimited => "rate_limited",
            Self::ParseFailed => "parse_failed",
            Self::Http => "http",
            Self::Network => "network",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_serializes_as_snake_case() {
        let kind = ProviderError::ChallengeRequired.kind();
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            "\"challenge_required\""
        );
        assert_eq!(kind.as_str(), "challenge_required");
    }
}
