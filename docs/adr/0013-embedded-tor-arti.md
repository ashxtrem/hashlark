# 0013. Built-in Tor with Arti behind a local SOCKS bridge

- **Status:** Accepted
- **Date:** 2026-09-29

## Context

v1.1 adds Tor without asking users to install it (PLAN §8.4). Every HTTP client in Hashlark is reqwest, which already supports SOCKS5 proxies.

## Decision

- Embed **Arti** (`arti-client`, rustls, onion-service client) behind the `tor` cargo feature. The feature is on in the desktop app and the server, and off in the CLI.
- Arti runs in-process. A minimal **local SOCKS5 server** (`net/socks.rs`, CONNECT only, no auth, bound to 127.0.0.1 on a random port) forwards each connection to `TorClient::connect`. Tor is then just another `socks5h://` proxy for every client, including per-provider routes.
- Tor starts only when needed: Tor turned on globally, or a provider routed through it. It bootstraps in the background.
- Users with their own Tor or Orbot can choose **external** mode with a SOCKS address instead.

## Consequences

- One network code path for proxies and Tor, and the bridge is tested with a real reqwest client.
- Larger binaries and longer builds, which is why the feature is optional.
- Two advisories come in through Arti's dependency tree and are documented in `deny.toml`:
  - RUSTSEC-2023-0071 (`rsa`): not exploitable here, because a Tor client performs no RSA private-key operations;
  - RUSTSEC-2024-0436 (`paste`): an unmaintained build-time macro.
- Networks that block Tor directly need **bridges** (pluggable transports). This isn't supported yet (follow-up). During development, Arti started correctly but couldn't fetch a consensus from the test network.
