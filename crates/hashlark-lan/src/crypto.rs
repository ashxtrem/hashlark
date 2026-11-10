// SPDX-License-Identifier: GPL-3.0-or-later

//! Key agreement and encryption.
//!
//! Pairing runs SPAKE2 over Ed25519 with the 6-digit PIN as the password, so
//! someone who records the exchange still can't test PINs offline. The
//! SPAKE2 output and the transcript go through HKDF-SHA256 to give the
//! long-term device key and two confirmation keys. Sends are sealed with
//! ChaCha20-Poly1305 under the device key.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2_10::{Digest, Sha256};
use spake2::{Ed25519Group, Identity, Password, Spake2};

/// Length of the device key shared by a pair of devices.
pub const KEY_LEN: usize = 32;
/// Length of a ChaCha20-Poly1305 nonce.
pub const NONCE_LEN: usize = 12;

/// Bytes from the OS random number generator.
pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("OS random number generator");
    bytes
}

/// A uniformly random 6-digit PIN.
pub(crate) fn new_pin() -> String {
    // 4_294_000_000 is the largest multiple of 1_000_000 below 2^32;
    // rejecting values above it keeps every PIN equally likely.
    loop {
        let n = u32::from_le_bytes(random::<4>());
        if n < 4_294_000_000 {
            return format!("{:06}", n % 1_000_000);
        }
    }
}

/// A random id for a device or a pairing session (hex).
pub(crate) fn new_id() -> String {
    data_encoding::HEXLOWER.encode(&random::<12>())
}

/// Which side of SPAKE2 this device plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// The device that starts pairing (it will usually send).
    Initiator,
    /// The device showing the PIN.
    Responder,
}

/// An in-progress SPAKE2 exchange.
pub(crate) struct Exchange {
    state: Spake2<Ed25519Group>,
    role: Role,
    initiator_id: String,
    responder_id: String,
    /// This side's outgoing message.
    pub msg: Vec<u8>,
}

impl std::fmt::Debug for Exchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Exchange")
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

impl Exchange {
    pub fn start(role: Role, pin: &str, initiator_id: &str, responder_id: &str) -> Self {
        let password = Password::new(pin.as_bytes());
        let a = Identity::new(initiator_id.as_bytes());
        let b = Identity::new(responder_id.as_bytes());
        let (state, msg) = match role {
            Role::Initiator => Spake2::<Ed25519Group>::start_a(&password, &a, &b),
            Role::Responder => Spake2::<Ed25519Group>::start_b(&password, &a, &b),
        };
        Self {
            state,
            role,
            initiator_id: initiator_id.to_owned(),
            responder_id: responder_id.to_owned(),
            msg,
        }
    }

    /// Completes the exchange with the other side's message. A wrong PIN
    /// isn't detected here: both sides just end up with different keys,
    /// which the confirmation MACs reveal.
    pub fn finish(self, their_msg: &[u8]) -> Option<PairKeys> {
        let (initiator_msg, responder_msg) = match self.role {
            Role::Initiator => (self.msg.as_slice(), their_msg),
            Role::Responder => (their_msg, self.msg.as_slice()),
        };
        let transcript = transcript(
            &self.initiator_id,
            &self.responder_id,
            initiator_msg,
            responder_msg,
        );
        let shared = self.state.finish(their_msg).ok()?;
        Some(PairKeys::derive(&shared, transcript))
    }
}

/// Hash of everything both sides saw, length-prefixed.
fn transcript(initiator_id: &str, responder_id: &str, msg_a: &[u8], msg_b: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"hashlark-lan/v1/pair");
    for part in [
        initiator_id.as_bytes(),
        responder_id.as_bytes(),
        msg_a,
        msg_b,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    hash.finalize().into()
}

/// Keys derived from a finished exchange.
pub(crate) struct PairKeys {
    pub device_key: [u8; KEY_LEN],
    confirm_initiator: [u8; 32],
    confirm_responder: [u8; 32],
    transcript: [u8; 32],
}

impl std::fmt::Debug for PairKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairKeys(..)")
    }
}

impl PairKeys {
    fn derive(shared: &[u8], transcript: [u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(&transcript), shared);
        let expand = |info: &[u8]| {
            let mut out = [0u8; 32];
            hk.expand(info, &mut out)
                .expect("32 bytes is a valid HKDF length");
            out
        };
        Self {
            device_key: expand(b"hashlark-lan/v1 device key"),
            confirm_initiator: expand(b"hashlark-lan/v1 confirm initiator"),
            confirm_responder: expand(b"hashlark-lan/v1 confirm responder"),
            transcript,
        }
    }

    fn mac(&self, role: Role) -> Hmac<Sha256> {
        let key = match role {
            Role::Initiator => &self.confirm_initiator,
            Role::Responder => &self.confirm_responder,
        };
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes keys of any length");
        mac.update(&self.transcript);
        mac
    }

    /// The confirmation `role` sends.
    pub fn confirmation(&self, role: Role) -> Vec<u8> {
        self.mac(role).finalize().into_bytes().to_vec()
    }

    /// Checks the confirmation the `role` side sent (constant time).
    pub fn verify(&self, role: Role, presented: &[u8]) -> bool {
        self.mac(role).verify_slice(presented).is_ok()
    }
}

/// Encrypts `plaintext`, returning the random nonce and the ciphertext.
pub(crate) fn seal(
    key: &[u8; KEY_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> ([u8; NONCE_LEN], Vec<u8>) {
    let cipher = ChaCha20Poly1305::new_from_slice(key).expect("32-byte key");
    let nonce = random::<NONCE_LEN>();
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("ChaCha20-Poly1305 encryption cannot fail for small messages");
    (nonce, ciphertext)
}

/// Decrypts and authenticates; `None` if anything was altered or the key
/// is wrong.
pub(crate) fn open(
    key: &[u8; KEY_LEN],
    aad: &[u8],
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).expect("32-byte key");
    cipher
        .decrypt(
            &Nonce::from(*nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .ok()
}
