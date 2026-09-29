// SPDX-License-Identifier: GPL-3.0-or-later

//! HTTP clients and network policy. Every request Hashlark makes goes
//! through a client built here, so DoH, proxies, Tor and rate limits apply
//! everywhere (PLAN §8).

pub mod doh;
pub mod socks;
#[cfg(feature = "tor")]
pub mod tor;

use std::num::NonZeroU32;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use reqwest::cookie::Jar;
use url::Url;

use crate::error::{Error, Result};
use crate::registry::{NetworkPolicy, Route};
use crate::settings::{NetworkSettings, TorMode};
use doh::{DohResolve, DohResolverClient, Endpoint};

pub const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (compatible; Hashlark/",
    env!("CARGO_PKG_VERSION"),
    "; +https://github.com/ashxtrem/hashlark)"
);

/// Upper bound for any single HTTP request. Searches apply a tighter
/// per-provider timeout on top.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Tor circuits are slower to set up.
const TOR_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// How to build a client.
#[derive(Debug, Clone)]
pub struct ClientOptions {
    /// Keep cookies (login sessions, challenge clearance) in this jar.
    pub cookies: Option<Arc<Jar>>,
    /// Overrides the default user agent (e.g. to match a browser that passed
    /// a challenge).
    pub user_agent: Option<String>,
    /// HTTP(S) or SOCKS5 proxy for every request.
    pub proxy: Option<Url>,
    /// Resolve names over DNS-over-HTTPS.
    pub resolver: Option<Arc<DohResolverClient>>,
    pub follow_redirects: bool,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            cookies: None,
            user_agent: None,
            proxy: None,
            resolver: None,
            follow_redirects: true,
        }
    }
}

pub fn build_client(options: &ClientOptions) -> Result<reqwest::Client> {
    let through_tor = options
        .proxy
        .as_ref()
        .is_some_and(|p| p.scheme().starts_with("socks"));
    let mut builder = reqwest::Client::builder()
        .user_agent(options.user_agent.as_deref().unwrap_or(USER_AGENT))
        .connect_timeout(if through_tor {
            TOR_CONNECT_TIMEOUT
        } else {
            CONNECT_TIMEOUT
        })
        .timeout(REQUEST_TIMEOUT)
        .redirect(if options.follow_redirects {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        });
    if let Some(jar) = &options.cookies {
        builder = builder.cookie_provider(Arc::clone(jar));
    }
    if let Some(proxy) = &options.proxy {
        let proxy = reqwest::Proxy::all(proxy.as_str())
            .map_err(|e| Error::Config(format!("invalid proxy `{proxy}`: {e}")))?;
        builder = builder.proxy(proxy);
    }
    if let Some(resolver) = &options.resolver {
        builder = builder.dns_resolver(Arc::new(DohResolve(Arc::clone(resolver))));
    }
    builder
        .build()
        .map_err(|e| Error::Config(format!("could not build HTTP client: {e}")))
}

/// State of Tor, for the UI.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TorStatus {
    /// Built-in Tor is available in this build.
    pub built_in: bool,
    pub running: bool,
    /// Tor can carry traffic.
    pub ready: bool,
    /// 0.0 to 1.0.
    pub progress: f32,
    pub message: String,
}

impl TorStatus {
    pub fn stopped() -> Self {
        Self {
            built_in: cfg!(feature = "tor"),
            running: false,
            ready: false,
            progress: 0.0,
            message: "not running".into(),
        }
    }
}

/// The global network settings, prepared for building clients.
#[derive(Debug, Clone)]
pub struct Network {
    resolver: Option<Arc<DohResolverClient>>,
    proxy: Option<Url>,
    tor_enabled: bool,
    tor_proxy: Option<Url>,
}

/// Where one provider's traffic goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteChoice {
    pub proxy: Option<Url>,
    /// Traffic goes through Tor, so `.onion` mirrors are usable.
    pub onion: bool,
}

impl Network {
    /// `embedded_tor` is the SOCKS address of the built-in Tor, if running.
    pub fn from_settings(settings: &NetworkSettings, embedded_tor: Option<Url>) -> Result<Self> {
        let resolver = if settings.doh.enabled {
            Some(DohResolverClient::new(
                Endpoint::for_settings(&settings.doh)?,
                settings.doh.fallback_to_system,
            )?)
        } else {
            None
        };
        Ok(Self {
            resolver,
            proxy: settings.proxy.clone(),
            tor_enabled: settings.tor.enabled,
            tor_proxy: match settings.tor.mode {
                TorMode::Embedded => embedded_tor,
                TorMode::External => settings.tor.socks_url.clone(),
            },
        })
    }

    /// No DoH, proxy or Tor.
    pub fn direct() -> Self {
        Self {
            resolver: None,
            proxy: None,
            tor_enabled: false,
            tor_proxy: None,
        }
    }

    pub fn resolver(&self) -> Option<&Arc<DohResolverClient>> {
        self.resolver.as_ref()
    }

    fn tor(&self) -> Result<RouteChoice> {
        let proxy = self.tor_proxy.clone().ok_or_else(|| {
            Error::Config(if cfg!(feature = "tor") {
                "Tor is not available: turn it on in Settings, or set the address of your own Tor"
                    .into()
            } else {
                "this build has no built-in Tor: set the address of a running Tor in Settings"
                    .into()
            })
        })?;
        Ok(RouteChoice {
            proxy: Some(proxy),
            onion: true,
        })
    }

    /// Applies a provider's policy on top of the global settings.
    pub fn route(&self, policy: &NetworkPolicy) -> Result<RouteChoice> {
        match policy.route {
            Route::Direct => Ok(RouteChoice {
                proxy: None,
                onion: false,
            }),
            Route::Proxy => Ok(RouteChoice {
                proxy: Some(policy.proxy.clone().ok_or_else(|| {
                    Error::Invalid("this provider's proxy URL is missing".into())
                })?),
                onion: false,
            }),
            Route::Tor => self.tor(),
            Route::Default if self.tor_enabled => self.tor(),
            Route::Default => Ok(RouteChoice {
                proxy: self.proxy.clone(),
                onion: false,
            }),
        }
    }

    /// Options for a client following `policy`.
    pub fn client_options(&self, policy: &NetworkPolicy) -> Result<(ClientOptions, bool)> {
        let route = self.route(policy)?;
        Ok((
            ClientOptions {
                proxy: route.proxy,
                resolver: self.resolver.clone(),
                ..ClientOptions::default()
            },
            route.onion,
        ))
    }
}

type Limiter = DefaultKeyedRateLimiter<String>;

static LIMITER: RwLock<Option<Arc<Limiter>>> = RwLock::new(None);

/// Sets the per-host request rate for all providers. Bursts of up to three
/// requests are allowed.
pub fn set_rate_limit(per_second: u32) {
    let rate = NonZeroU32::new(per_second.max(1)).expect("non-zero");
    let burst = NonZeroU32::new(per_second.max(3)).expect("non-zero");
    let limiter = RateLimiter::keyed(Quota::per_second(rate).allow_burst(burst));
    *LIMITER.write().expect("lock") = Some(Arc::new(limiter));
}

/// Waits until a request to `url`'s host is allowed. Loopback hosts (local
/// Jackett/Prowlarr, tests) are never throttled.
pub async fn throttle(url: &Url) {
    let Some(host) = url.host_str() else { return };
    let loopback = host == "localhost"
        || host
            .trim_matches(|c| c == '[' || c == ']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if loopback {
        return;
    }
    let limiter = LIMITER.read().expect("lock").clone();
    if let Some(limiter) = limiter {
        limiter.until_key_ready(&host.to_ascii_lowercase()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn policy(route: Route, proxy: Option<&str>) -> NetworkPolicy {
        NetworkPolicy {
            route,
            proxy: proxy.map(|p| p.parse().unwrap()),
        }
    }

    #[test]
    fn routes_follow_policy_and_global_settings() {
        let mut settings = Settings::default().network;
        settings.doh.enabled = false;
        settings.proxy = Some("http://proxy.example:8080".parse().unwrap());
        settings.tor.mode = crate::settings::TorMode::External;
        settings.tor.socks_url = Some("socks5h://127.0.0.1:9050".parse().unwrap());
        let net = Network::from_settings(&settings, None).unwrap();

        let default = net.route(&policy(Route::Default, None)).unwrap();
        assert_eq!(
            default.proxy.unwrap().as_str(),
            "http://proxy.example:8080/"
        );
        assert!(!default.onion);
        assert_eq!(net.route(&policy(Route::Direct, None)).unwrap().proxy, None);
        let tor = net.route(&policy(Route::Tor, None)).unwrap();
        assert!(tor.onion);
        assert_eq!(tor.proxy.unwrap().scheme(), "socks5h");
        assert!(net.route(&policy(Route::Proxy, None)).is_err());
        assert_eq!(
            net.route(&policy(Route::Proxy, Some("socks5://1.2.3.4:1080")))
                .unwrap()
                .proxy
                .unwrap()
                .as_str(),
            "socks5://1.2.3.4:1080"
        );

        settings.tor.enabled = true;
        let net = Network::from_settings(&settings, None).unwrap();
        assert!(
            net.route(&policy(Route::Default, None)).unwrap().onion,
            "Tor for everything"
        );
        assert!(!net.route(&policy(Route::Direct, None)).unwrap().onion);

        settings.tor.socks_url = None;
        let net = Network::from_settings(&settings, None).unwrap();
        assert!(net.route(&policy(Route::Default, None)).is_err());
    }

    #[test]
    fn clients_build_with_proxies_and_doh() {
        let settings = Settings::default().network;
        let net = Network::from_settings(&settings, None).unwrap();
        assert!(net.resolver().is_some(), "DoH is on by default");
        let (mut options, _) = net.client_options(&NetworkPolicy::default()).unwrap();
        options.proxy = Some("socks5h://127.0.0.1:9050".parse().unwrap());
        build_client(&options).unwrap();
    }

    #[tokio::test]
    async fn loopback_is_never_throttled() {
        set_rate_limit(1);
        let url: Url = "http://127.0.0.1:1/x".parse().unwrap();
        let started = std::time::Instant::now();
        for _ in 0..20 {
            throttle(&url).await;
        }
        assert!(started.elapsed() < Duration::from_millis(200));
    }
}
