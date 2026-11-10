# 0016. Send results to other devices over the local network

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

People often search on their phone while the torrent client runs on their computer, or the other way round. Both apps run the whole core themselves (ADR 0003), so the phone has no server it could tell to download something.

Constraints:

- **No cloud relay or accounts.** Nothing may leave the local network (ADR 0010).
- **Shared Wi-Fi isn't trusted.** In a flat, dorm or café, anyone on the network could otherwise push downloads into someone's torrent client or read what they send.
- **The app's HTTP API stays on loopback.** It is guarded against DNS rebinding (ADR 0004), so it must not be exposed to the network.
- **Discovery may be blocked.** Guest networks often block multicast, and Windows asks before an app may accept connections.
- **Downloads go to the OS handler** on whichever device receives them (ADR 0008).

## Decision

**A separate crate, `hashlark-lan`.** The desktop app uses it directly and the Android app uses it through `hashlark-ffi`, with JSON payloads as elsewhere at that boundary (ADR 0015). There is one implementation of the protocol, so the two apps can't drift apart.

**Discovery.** Devices announce themselves with mDNS as `_hashlark._tcp.local.`, with a TXT record holding the device id, protocol version, platform and name. Where multicast doesn't work, the user can type an address (`ip` or `ip:port`). A typed address is used exactly as entered, so port forwarding and NAT work too. A device counts as "on this network" only while it is announcing itself.

**Listener.** Each device runs its own small HTTP server on `0.0.0.0:47821`, or on another port if that one is taken; mDNS announces the real port. It has four routes under `/lan/v1`: `hello`, `pair/start`, `pair/confirm` and `send`. Limits:

- request bodies of at most 48 KiB;
- 120 requests a minute per address, and 10 a minute for pairing;
- no proxy or Tor on outgoing calls.

**Trust: pair once with a PIN.**

- The receiving device opens pairing on request and shows a 6-digit PIN for 2 minutes.
- The two devices run SPAKE2 over Ed25519, keyed by the PIN. HKDF-SHA256 over the exchange gives a long-term device key and two confirmation keys, and each side proves with an HMAC that it holds the same key.
- 3 wrong PINs replace the PIN with a new one.
- Pairing is mutual: afterwards either device can send to the other.

**Sending.**

- A link is sealed with ChaCha20-Poly1305 under the device key, with both device ids authenticated alongside it.
- Each message carries a timestamp that must be within 5 minutes of the receiver's clock, and a nonce the receiver remembers for 10 minutes, so a message can't be replayed.
- Only `magnet:` links are carried in this version.

**Storage.** This device's id and each pairing's key are kept in the platform's secret store: the OS keychain on desktop, and the Android Keystore-backed store on Android. They are never stored in SQLite.

**Receiving.** The receiver opens the link with the OS handler (ADR 0008), and its UI says which device sent it.

- On the desktop, sharing is off until the user turns it on.
- On Android, sharing is on by default but runs only while the app is in the foreground. There is no background service, and the app holds a multicast lock only while sharing runs.

## Consequences

- No accounts, servers or internet access are involved, and nothing leaves the local network.
- **What others on the network can see:** that Hashlark devices are there and what they're called (mDNS and `hello`), and that they talk to each other. They can't see the links, forge a send or replay one.
  - Guessing the PIN is limited to online attempts, which the rate limits and PIN rotation restrict.
  - Recording a pairing doesn't let anyone test PINs offline, because SPAKE2 is a password-authenticated key exchange.
- **Setting it up can take more than one step:**
  - Windows asks to allow Hashlark through the firewall the first time, and on a network marked *Public*, Allow has to cover public networks too.
  - Networks that block multicast need a typed address, so both apps show theirs.
- **Android receives only while Hashlark is open.** Android doesn't let apps open other apps from the background. Receiving in the background would need a foreground service and a notification to tap, which is possible later.
- **Limits of this version:**
  - Results that only have a `.torrent` file can't be sent yet.
  - The headless server isn't a target.
  - Forgetting a device only removes it locally. The other device finds out on its next send, which is refused with a "pair again" message.
- **New dependencies:** `spake2` 0.4, which brings in `curve25519-dalek` 4 and `sha2` 0.10 next to the 0.11 used elsewhere, plus `chacha20poly1305`, `mdns-sd` and `if-addrs`. All are MIT or Apache-2.0.
- **Testing:**
  - The protocol is covered by loopback tests, including a port-forwarding case.
  - Real multicast is checked by an opt-in test (`cargo test -p hashlark-lan -- --ignored`).
  - `cargo run -p hashlark-lan --example lan-send` and `--example lan-receive` stand in for the other device during manual testing.
