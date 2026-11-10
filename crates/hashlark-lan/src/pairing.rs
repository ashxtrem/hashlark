// SPDX-License-Identifier: GPL-3.0-or-later

//! The receiving side of pairing: a short window, opened from the UI, during
//! which one device may pair by proving it knows the PIN on screen.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::crypto::{self, Exchange, PairKeys, Role};
use crate::error::{Error, Result};
use crate::protocol::Hello;

/// How long the PIN stays valid after the user opens pairing.
pub const WINDOW: Duration = Duration::from_secs(120);
/// How long a started pairing may wait for its confirmation.
const SESSION_TTL: Duration = Duration::from_secs(60);
/// Wrong PINs allowed before the PIN changes.
pub const MAX_FAILURES: u8 = 3;
/// Pairings that may be in progress at once.
const MAX_SESSIONS: usize = 4;

/// The PIN shown on screen while pairing is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCode {
    pub pin: String,
    /// Unix milliseconds.
    pub expires_at: i64,
}

/// A device that started pairing and hasn't confirmed yet.
#[derive(Debug)]
pub(crate) struct Pending {
    pub from: Hello,
    pub addr: SocketAddr,
    pub keys: PairKeys,
    created: Instant,
}

#[derive(Debug)]
struct Window {
    code: PairingCode,
    deadline: Instant,
    failures: u8,
    sessions: HashMap<String, Pending>,
}

impl Window {
    fn new() -> Self {
        Self {
            code: PairingCode {
                pin: crypto::new_pin(),
                expires_at: crate::now_millis() + WINDOW.as_millis() as i64,
            },
            deadline: Instant::now() + WINDOW,
            failures: 0,
            sessions: HashMap::new(),
        }
    }
}

/// Result of a confirmation.
#[derive(Debug)]
pub(crate) enum Confirmed {
    /// The PIN was right; pairing is now closed.
    Paired(Box<Pending>),
    /// The PIN was wrong. `rotated` is the new code when too many attempts
    /// failed.
    WrongPin { rotated: Option<PairingCode> },
}

#[derive(Debug, Default)]
pub(crate) struct Pairing {
    window: Option<Window>,
}

impl Pairing {
    /// Opens pairing with a fresh PIN (replacing any open window).
    pub fn open(&mut self) -> PairingCode {
        let window = Window::new();
        let code = window.code.clone();
        self.window = Some(window);
        code
    }

    pub fn close(&mut self) {
        self.window = None;
    }

    /// The current code, if pairing is open and hasn't expired.
    pub fn current(&mut self) -> Option<PairingCode> {
        self.live().map(|w| w.code.clone())
    }

    fn live(&mut self) -> Option<&mut Window> {
        if self
            .window
            .as_ref()
            .is_some_and(|w| Instant::now() >= w.deadline)
        {
            self.window = None;
        }
        self.window.as_mut()
    }

    /// First message from a device that wants to pair. Returns the session
    /// id and this side's SPAKE2 message.
    pub fn start(
        &mut self,
        self_id: &str,
        from: Hello,
        their_msg: &[u8],
        addr: SocketAddr,
    ) -> Result<(String, Vec<u8>)> {
        let window = self.live().ok_or(Error::PairingClosed)?;
        window
            .sessions
            .retain(|_, s| s.created.elapsed() < SESSION_TTL);
        if window.sessions.len() >= MAX_SESSIONS {
            return Err(Error::RateLimited);
        }
        let exchange = Exchange::start(Role::Responder, &window.code.pin, &from.id, self_id);
        let msg = exchange.msg.clone();
        let keys = exchange
            .finish(their_msg)
            .ok_or_else(|| Error::Invalid("malformed pairing message".into()))?;
        let session = crypto::new_id();
        window.sessions.insert(
            session.clone(),
            Pending {
                from,
                addr,
                keys,
                created: Instant::now(),
            },
        );
        Ok((session, msg))
    }

    /// Second message: the initiator's proof that it used the same PIN.
    pub fn confirm(&mut self, session: &str, mac: &[u8]) -> Result<Confirmed> {
        let window = self.live().ok_or(Error::PairingClosed)?;
        let pending = window
            .sessions
            .remove(session)
            .filter(|s| s.created.elapsed() < SESSION_TTL)
            .ok_or(Error::PairingClosed)?;
        if pending.keys.verify(Role::Initiator, mac) {
            self.window = None;
            return Ok(Confirmed::Paired(Box::new(pending)));
        }
        window.failures += 1;
        if window.failures >= MAX_FAILURES {
            let fresh = Window::new();
            let code = fresh.code.clone();
            *window = fresh;
            return Ok(Confirmed::WrongPin {
                rotated: Some(code),
            });
        }
        Ok(Confirmed::WrongPin { rotated: None })
    }
}
