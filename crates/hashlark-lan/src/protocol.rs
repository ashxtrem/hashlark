// SPDX-License-Identifier: GPL-3.0-or-later

//! Wire format of `/lan/v1`. Binary fields are base64url without padding.

use serde::{Deserialize, Serialize};

use crate::Platform;

/// Version of the protocol, advertised in mDNS and `/hello`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Who a device is. Public: anyone on the network can ask for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    /// Port the device listens on.
    pub port: u16,
    pub v: u32,
}

/// First pairing message, from the device that will send (SPAKE2 side A).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PairStart {
    pub from: Hello,
    /// Id of the device being paired with, so a stale address fails fast.
    pub to: String,
    pub msg: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PairStartReply {
    pub device: Hello,
    pub session: String,
    pub msg: String,
}

/// Proves the sender derived the same key (it knew the PIN).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PairConfirm {
    pub session: String,
    pub mac: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PairConfirmReply {
    pub mac: String,
}

/// An encrypted [`Message`]. `from` and `to` are authenticated as AAD.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SendRequest {
    pub from: String,
    pub to: String,
    pub nonce: String,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MessageKind {
    Magnet,
}

/// What a send carries, before encryption.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Message {
    pub kind: MessageKind,
    pub url: String,
    pub title: String,
    /// Unix seconds; old messages are refused.
    pub sent_at: i64,
    /// Port the sender listens on, so the receiver can send back.
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ErrorBody {
    pub code: String,
    pub message: String,
}

/// Associated data for a send: binds the ciphertext to both devices.
pub(crate) fn send_aad(from: &str, to: &str) -> Vec<u8> {
    format!("hashlark-lan/v{PROTOCOL_VERSION}/send\0{from}\0{to}").into_bytes()
}

pub(crate) fn b64(bytes: &[u8]) -> String {
    data_encoding::BASE64URL_NOPAD.encode(bytes)
}

pub(crate) fn unb64(text: &str) -> Option<Vec<u8>> {
    data_encoding::BASE64URL_NOPAD.decode(text.as_bytes()).ok()
}
