// SPDX-License-Identifier: GPL-3.0-or-later

//! The listener other devices call. It exposes nothing but pairing and
//! receiving links; the app's own API stays on loopback.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::crypto::{self, NONCE_LEN, Role};
use crate::error::Error;
use crate::pairing::Confirmed;
use crate::protocol::{
    ErrorBody, Hello, Message, MessageKind, PairConfirm, PairConfirmReply, PairStart,
    PairStartReply, SendRequest, b64, send_aad, unb64,
};
use crate::service::Shared;
use crate::store::StoredPeer;
use crate::{Received, clean_name, clean_title, now_secs, validate_link};

/// Largest request body accepted (a magnet with many trackers, encrypted
/// and base64 encoded, fits easily).
const MAX_BODY: usize = 48 * 1024;
/// How far a message's timestamp may be from this device's clock.
const MAX_SKEW_SECS: i64 = 5 * 60;

pub(crate) fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/lan/v1/hello", get(hello))
        .route("/lan/v1/pair/start", post(pair_start))
        .route("/lan/v1/pair/confirm", post(pair_confirm))
        .route("/lan/v1/send", post(send))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(shared)
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            code: self.code().to_owned(),
            message: self.to_string(),
        };
        (self.status(), Json(body)).into_response()
    }
}

type Peer = ConnectInfo<SocketAddr>;

async fn hello(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): Peer,
) -> Result<Json<Hello>, Error> {
    shared.limit(peer.ip(), false)?;
    Ok(Json(shared.hello.clone()))
}

fn is_device_id(id: &str) -> bool {
    id.len() == 24
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn check_recipient(shared: &Shared, to: &str) -> Result<(), Error> {
    if to == shared.hello.id {
        Ok(())
    } else {
        Err(Error::Invalid(
            "a different device now answers at this address".into(),
        ))
    }
}

async fn pair_start(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): Peer,
    Json(request): Json<PairStart>,
) -> Result<Json<PairStartReply>, Error> {
    shared.limit(peer.ip(), true)?;
    check_recipient(&shared, &request.to)?;
    let mut from = request.from;
    if !is_device_id(&from.id) || from.id == shared.hello.id {
        return Err(Error::Invalid("bad device id".into()));
    }
    from.name = clean_name(&from.name);
    let msg = unb64(&request.msg).ok_or_else(|| Error::Invalid("bad pairing message".into()))?;
    let addr = SocketAddr::new(peer.ip(), from.port);
    let (session, reply) =
        shared
            .pairing
            .lock()
            .expect("lock")
            .start(&shared.hello.id, from, &msg, addr)?;
    Ok(Json(PairStartReply {
        device: shared.hello.clone(),
        session,
        msg: b64(&reply),
    }))
}

async fn pair_confirm(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): Peer,
    Json(request): Json<PairConfirm>,
) -> Result<Json<PairConfirmReply>, Error> {
    shared.limit(peer.ip(), true)?;
    let mac = unb64(&request.mac).ok_or_else(|| Error::Invalid("bad confirmation".into()))?;
    let outcome = shared
        .pairing
        .lock()
        .expect("lock")
        .confirm(&request.session, &mac)?;
    match outcome {
        Confirmed::Paired(pending) => {
            let reply = pending.keys.confirmation(Role::Responder);
            let stored = StoredPeer::new(
                pending.from.id.clone(),
                pending.from.name.clone(),
                pending.from.platform,
                &pending.keys.device_key,
                Some(pending.addr),
            );
            shared.save_peer(stored).await?;
            let device = shared.device(&pending.from.id);
            shared.emit(move |events| {
                events.pairing_changed(None);
                if let Some(device) = device {
                    events.paired(device);
                }
                events.devices_changed();
            });
            Ok(Json(PairConfirmReply { mac: b64(&reply) }))
        }
        Confirmed::WrongPin { rotated } => {
            if let Some(code) = rotated {
                shared.emit(move |events| events.pairing_changed(Some(code)));
            }
            Err(Error::WrongPin)
        }
    }
}

async fn send(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): Peer,
    Json(request): Json<SendRequest>,
) -> Result<StatusCode, Error> {
    shared.limit(peer.ip(), false)?;
    check_recipient(&shared, &request.to)?;
    let stored = shared.store.get(&request.from).ok_or(Error::NotPaired)?;
    let key = stored.key().ok_or(Error::NotPaired)?;
    let nonce: [u8; NONCE_LEN] = unb64(&request.nonce)
        .and_then(|n| n.try_into().ok())
        .ok_or_else(|| Error::Invalid("bad nonce".into()))?;
    let data = unb64(&request.data).ok_or_else(|| Error::Invalid("bad payload".into()))?;
    let plaintext = crypto::open(&key, &send_aad(&request.from, &request.to), &nonce, &data)
        // Wrong key: the sender holds a pairing this device has replaced.
        .ok_or(Error::Rejected)?;
    let message: Message = serde_json::from_slice(&plaintext)
        .map_err(|e| Error::Invalid(format!("bad message: {e}")))?;
    if (now_secs() - message.sent_at).abs() > MAX_SKEW_SECS {
        return Err(Error::Invalid(
            "the link was sent too long ago; check that both devices' clocks are right".into(),
        ));
    }
    if !shared.replay_check(&request.from, nonce) {
        return Err(Error::Invalid("this link was already received".into()));
    }
    match message.kind {
        MessageKind::Magnet => validate_link(&message.url)?,
    }

    let addr = SocketAddr::new(peer.ip(), message.port);
    if stored.last_addr != Some(addr) {
        let shared2 = Arc::clone(&shared);
        let id = request.from.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = shared2.store.set_last_addr(&id, addr) {
                tracing::warn!(error = %e, "could not remember a device's address");
            }
        });
    }
    let Some(from) = shared.device(&request.from) else {
        return Err(Error::NotPaired);
    };
    let received = Received {
        from,
        url: message.url,
        title: clean_title(&message.title),
    };
    tracing::info!(from = %received.from.name, "received a link over the LAN");
    shared.emit(move |events| events.received(received));
    Ok(StatusCode::NO_CONTENT)
}

impl Shared {
    /// Applies the per-address request limits.
    pub(crate) fn limit(&self, ip: IpAddr, pairing: bool) -> Result<(), Error> {
        let ok = self.limiter.check_key(&ip).is_ok()
            && (!pairing || self.pair_limiter.check_key(&ip).is_ok());
        if ok { Ok(()) } else { Err(Error::RateLimited) }
    }
}
