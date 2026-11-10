// SPDX-License-Identifier: GPL-3.0-or-later

//! Calls to another device's listener.

use std::net::SocketAddr;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::protocol::{
    ErrorBody, Hello, PairConfirm, PairConfirmReply, PairStart, PairStartReply, SendRequest,
};

#[derive(Debug, Clone)]
pub(crate) struct Client(reqwest::Client);

impl Client {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            // Local addresses must never go through the user's proxy or Tor.
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .user_agent(concat!("hashlark-lan/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        Self(client)
    }

    fn url(addr: SocketAddr, path: &str) -> String {
        format!("http://{addr}/lan/v1/{path}")
    }

    pub async fn hello(&self, addr: SocketAddr) -> Result<Hello> {
        let response = self
            .0
            .get(Self::url(addr, "hello"))
            .send()
            .await
            .map_err(|e| transport(e, addr))?;
        decode(response, addr).await
    }

    pub async fn pair_start(&self, addr: SocketAddr, body: &PairStart) -> Result<PairStartReply> {
        self.post(addr, "pair/start", body).await
    }

    pub async fn pair_confirm(
        &self,
        addr: SocketAddr,
        body: &PairConfirm,
    ) -> Result<PairConfirmReply> {
        self.post(addr, "pair/confirm", body).await
    }

    pub async fn send(&self, addr: SocketAddr, body: &SendRequest) -> Result<()> {
        let response = self
            .0
            .post(Self::url(addr, "send"))
            .json(body)
            .send()
            .await
            .map_err(|e| transport(e, addr))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(remote_error(response).await)
        }
    }

    async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        addr: SocketAddr,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let response = self
            .0
            .post(Self::url(addr, path))
            .json(body)
            .send()
            .await
            .map_err(|e| transport(e, addr))?;
        decode(response, addr).await
    }
}

fn transport(err: reqwest::Error, addr: SocketAddr) -> Error {
    if err.is_connect() || err.is_timeout() {
        Error::Unreachable(addr.to_string())
    } else {
        Error::Protocol(err.to_string())
    }
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response, addr: SocketAddr) -> Result<T> {
    if !response.status().is_success() {
        return Err(remote_error(response).await);
    }
    response
        .json()
        .await
        .map_err(|e| Error::Protocol(format!("{addr}: {e}")))
}

async fn remote_error(response: reqwest::Response) -> Error {
    let status = response.status();
    match response.json::<ErrorBody>().await {
        Ok(body) => Error::from_remote(status, &body.code, body.message),
        Err(_) => Error::Protocol(status.to_string()),
    }
}
