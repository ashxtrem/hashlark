// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors shown to the user when pairing or sending fails.

use axum::http::StatusCode;

/// Everything that can go wrong. [`Error::code`] is a stable string the
/// apps use to pick a message or an action (e.g. "pair again").
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The other device isn't accepting pairing requests right now.
    #[error("the other device isn't ready to pair; open pairing there and try again")]
    PairingClosed,
    #[error("wrong PIN")]
    WrongPin,
    /// This device has no key for the one it tried to send to.
    #[error("this device isn't paired yet")]
    NotPaired,
    /// The other device no longer knows this one (it was forgotten there).
    #[error("the other device no longer recognises this one; pair again")]
    Rejected,
    #[error("couldn't reach {0}; check that it's on the same network and Hashlark is open")]
    Unreachable(String),
    #[error("too many attempts; wait a minute and try again")]
    RateLimited,
    #[error("{0}")]
    Invalid(String),
    #[error("unexpected answer from the other device: {0}")]
    Protocol(String),
    #[error("could not store the pairing: {0}")]
    Storage(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Stable machine-readable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::PairingClosed => "pairing_closed",
            Self::WrongPin => "wrong_pin",
            Self::NotPaired => "not_paired",
            Self::Rejected => "rejected",
            Self::Unreachable(_) => "unreachable",
            Self::RateLimited => "rate_limited",
            Self::Invalid(_) => "invalid",
            Self::Protocol(_) => "protocol",
            Self::Storage(_) => "storage",
            Self::Io(_) => "io",
        }
    }

    /// HTTP status the receiving side answers with.
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::PairingClosed => StatusCode::FORBIDDEN,
            Self::WrongPin | Self::NotPaired | Self::Rejected => StatusCode::UNAUTHORIZED,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Invalid(_) | Self::Protocol(_) => StatusCode::BAD_REQUEST,
            Self::Unreachable(_) | Self::Storage(_) | Self::Io(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Rebuilds the error a receiver reported (`code` from its JSON body).
    pub(crate) fn from_remote(status: StatusCode, code: &str, message: String) -> Self {
        match code {
            "pairing_closed" => Self::PairingClosed,
            "wrong_pin" => Self::WrongPin,
            // The receiver says it doesn't know us.
            "not_paired" | "rejected" => Self::Rejected,
            "rate_limited" => Self::RateLimited,
            "invalid" => Self::Invalid(message),
            _ => Self::Protocol(format!("{status}: {message}")),
        }
    }
}

impl From<hashlark_core::Error> for Error {
    fn from(err: hashlark_core::Error) -> Self {
        Self::Storage(err.to_string())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
