// SPDX-License-Identifier: GPL-3.0-or-later

//! JSON error responses.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hashlark_core::{Error, ErrorKind};
use serde::Serialize;

/// Body of every error response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorDetail {
    /// Stable machine-readable code, e.g. `not_found`.
    pub code: &'static str,
    pub message: String,
    /// For `provider_error`: what went wrong with the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_error: Option<ErrorKind>,
    /// For `invalid_definition`: every problem found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<String>>,
}

/// An error returned by a handler.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    detail: ErrorDetail,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            detail: ErrorDetail {
                code,
                message: message.into(),
                provider_error: None,
                details: None,
            },
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid credentials",
        )
    }
}

impl From<Error> for ApiError {
    fn from(err: Error) -> Self {
        match err {
            Error::NotFound(_) | Error::UnknownProvider(_) => {
                Self::new(StatusCode::NOT_FOUND, "not_found", err.to_string())
            }
            Error::Invalid(message) => Self::bad_request(message),
            Error::Definition(def) => Self {
                status: StatusCode::BAD_REQUEST,
                detail: ErrorDetail {
                    code: "invalid_definition",
                    message: format!("the definition has {} problem(s)", def.errors.len()),
                    provider_error: None,
                    details: Some(def.errors),
                },
            },
            Error::Provider { ref source, .. } => Self {
                status: StatusCode::BAD_GATEWAY,
                detail: ErrorDetail {
                    code: "provider_error",
                    provider_error: Some(source.kind()),
                    message: err.to_string(),
                    details: None,
                },
            },
            other => {
                tracing::error!(error = %other, "request failed");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error; see the server log",
                )
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ErrorBody { error: self.detail })).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
