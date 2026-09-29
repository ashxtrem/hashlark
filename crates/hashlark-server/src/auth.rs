// SPDX-License-Identifier: GPL-3.0-or-later

//! Request authentication and `Host` header checking.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use subtle::ConstantTimeEq;

use crate::AppState;
use crate::error::ApiError;

/// Who may call the API.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// A fixed bearer token (desktop mode: random per launch).
    pub token: Option<String>,
    /// Also accept API keys stored in the database (headless mode).
    pub api_keys: bool,
}

impl AuthConfig {
    /// A random 256-bit token, hex encoded.
    pub fn random_token() -> String {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("OS random number generator");
        data_encoding::HEXLOWER.encode(&bytes)
    }

    pub(crate) fn accepts_token(&self, presented: &str) -> bool {
        self.token
            .as_deref()
            .is_some_and(|t| bool::from(t.as_bytes().ct_eq(presented.as_bytes())))
    }
}

/// Limits failed authentication attempts per client address.
#[derive(Debug)]
pub struct FailureLimiter(DefaultKeyedRateLimiter<IpAddr>);

impl Default for FailureLimiter {
    fn default() -> Self {
        let per_minute = NonZeroU32::new(20).expect("non-zero");
        Self(RateLimiter::keyed(Quota::per_minute(per_minute)))
    }
}

impl FailureLimiter {
    /// Records a failure; `false` once the client has failed too often.
    fn record(&self, ip: IpAddr) -> bool {
        self.0.check_key(&ip).is_ok()
    }
}

impl AppState {
    /// Whether `key` is the configured token or a valid API key.
    pub async fn authorize(&self, key: &str) -> bool {
        if self.auth.accepts_token(key) {
            return true;
        }
        self.auth.api_keys && self.engine.verify_api_key(key).await.unwrap_or(false)
    }

    /// Authorises, counting failures against `ip`.
    pub async fn check_credentials(&self, key: Option<&str>, ip: IpAddr) -> Result<(), StatusCode> {
        match key {
            Some(key) if self.authorize(key.trim()).await => Ok(()),
            _ if !self.failures.record(ip) => Err(StatusCode::TOO_MANY_REQUESTS),
            _ => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

/// Which `Host` headers are accepted. Checking it defeats DNS rebinding,
/// where a malicious website points its own domain at 127.0.0.1.
#[derive(Debug, Clone)]
pub enum HostPolicy {
    /// Only `localhost`, `127.0.0.1` and `[::1]` (any port).
    Loopback,
    /// Any host (e.g. a headless server behind a reverse proxy).
    Any,
}

impl HostPolicy {
    fn allows(&self, host: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Loopback => {
                let name = match host.strip_prefix('[') {
                    Some(rest) => rest.split(']').next().unwrap_or_default(),
                    None => host.split(':').next().unwrap_or_default(),
                };
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "localhost" | "127.0.0.1" | "::1"
                )
            }
        }
    }
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

/// The client address, when the server runs with connect info.
pub(crate) fn client_ip<B>(request: &axum::http::Request<B>) -> IpAddr {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |c| c.0.ip())
}

/// Rejects requests without a valid bearer token or API key.
pub async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let ip = client_ip(&request);
    match state.check_credentials(bearer(request.headers()), ip).await {
        Ok(()) => next.run(request).await,
        Err(StatusCode::TOO_MANY_REQUESTS) => ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_attempts",
            "too many failed sign-in attempts; wait a minute",
        )
        .into_response(),
        Err(_) => ApiError::unauthorized().into_response(),
    }
}

/// Rejects requests whose `Host` header isn't allowed.
pub async fn check_host(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .or_else(|| request.uri().authority().map(|a| a.as_str()));
    match host {
        Some(host) if state.host_policy.allows(host) => next.run(request).await,
        _ => ApiError::new(
            StatusCode::MISDIRECTED_REQUEST,
            "bad_host",
            "host not allowed",
        )
        .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_policy() {
        let p = HostPolicy::Loopback;
        for ok in [
            "127.0.0.1:8787",
            "localhost",
            "LOCALHOST:1",
            "[::1]:9000",
            "[::1]",
        ] {
            assert!(p.allows(ok), "{ok}");
        }
        for bad in [
            "evil.example",
            "evil.example:8787",
            "127.0.0.1.evil.example",
            "10.0.0.2",
        ] {
            assert!(!p.allows(bad), "{bad}");
        }
        assert!(HostPolicy::Any.allows("evil.example"));
    }

    #[test]
    fn token_comparison() {
        let auth = AuthConfig {
            token: Some("secret".into()),
            api_keys: false,
        };
        assert!(auth.accepts_token("secret"));
        assert!(!auth.accepts_token("secreT"));
        assert!(!auth.accepts_token(""));
        assert_eq!(AuthConfig::random_token().len(), 64);
    }

    #[test]
    fn failures_are_limited() {
        let limiter = FailureLimiter::default();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!((0..20).all(|_| limiter.record(ip)));
        assert!(!limiter.record(ip));
        assert!(
            limiter.record("10.0.0.2".parse().unwrap()),
            "other clients are unaffected"
        );
    }
}
