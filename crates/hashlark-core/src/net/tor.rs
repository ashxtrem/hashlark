// SPDX-License-Identifier: GPL-3.0-or-later

//! Built-in Tor (Arti, the Tor Project's Rust client). It runs inside
//! Hashlark behind a local SOCKS5 bridge, so no separate Tor install is
//! needed and every HTTP client reaches it as an ordinary proxy.

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use arti_client::config::CfgPath;
use arti_client::{BootstrapBehavior, TorClient, TorClientConfig};
use tor_rtcompat::PreferredRuntime;
use url::Url;

use super::TorStatus;
use super::socks::{self, AsyncStream, ConnectFuture, Connector};
use crate::error::{Error, Result};

struct ArtiConnector(Arc<TorClient<PreferredRuntime>>);

impl Connector for ArtiConnector {
    fn connect(&self, host: String, port: u16) -> ConnectFuture {
        let client = Arc::clone(&self.0);
        Box::pin(async move {
            let stream = client
                .connect((host.as_str(), port))
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;
            Ok(Box::new(stream) as Box<dyn AsyncStream>)
        })
    }
}

/// A running Tor client and its SOCKS bridge.
pub struct EmbeddedTor {
    client: Arc<TorClient<PreferredRuntime>>,
    addr: SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for EmbeddedTor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedTor")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl EmbeddedTor {
    /// Starts Tor with its state under `dir`. Returns at once; Tor connects
    /// to the network in the background (usually 5–30 seconds).
    pub async fn start(dir: &Path) -> Result<Arc<Self>> {
        let mut builder = TorClientConfig::builder();
        builder
            .storage()
            .state_dir(CfgPath::new_literal(dir.join("state")))
            .cache_dir(CfgPath::new_literal(dir.join("cache")));
        builder.address_filter().allow_onion_addrs(true);
        let config = builder
            .build()
            .map_err(|e| Error::Config(format!("Tor configuration: {e}")))?;
        let client = TorClient::builder()
            .config(config)
            .bootstrap_behavior(BootstrapBehavior::OnDemand)
            .create_unbootstrapped()
            .map_err(|e| Error::Config(format!("could not start Tor: {e}")))?;

        let bootstrapping = Arc::clone(&client);
        tokio::spawn(async move {
            match bootstrapping.bootstrap().await {
                Ok(()) => tracing::info!("Tor is connected"),
                Err(e) => tracing::warn!(error = %e, "Tor could not connect"),
            }
        });

        let (addr, server) = socks::start(Arc::new(ArtiConnector(Arc::clone(&client)))).await?;
        tracing::info!(%addr, "built-in Tor started");
        Ok(Arc::new(Self {
            client,
            addr,
            server,
        }))
    }

    /// The proxy URL that routes through this Tor client.
    pub fn socks_url(&self) -> Url {
        format!("socks5h://{}", self.addr)
            .parse()
            .expect("valid URL")
    }

    pub fn status(&self) -> TorStatus {
        let status = self.client.bootstrap_status();
        TorStatus {
            built_in: true,
            running: true,
            ready: status.ready_for_traffic(),
            progress: status.as_frac(),
            message: status.to_string(),
        }
    }
}

impl Drop for EmbeddedTor {
    fn drop(&mut self) {
        self.server.abort();
    }
}
