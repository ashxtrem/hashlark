# 0012. Our own DNS-over-HTTPS resolver

- **Status:** Accepted
- **Date:** 2026-09-29

## Context

DoH is on by default (PLAN §8.1). hickory-resolver 0.26 reworked its DoH configuration API, and custom DoH URLs weren't clearly supported.

## Decision

Implement RFC 8484 ourselves in `net/doh.rs`: DNS wire-format queries POSTed over HTTPS with reqwest, answers parsed with support for name compression and CNAMEs, and TTL-based caching. Plugged into reqwest as a `dns::Resolve`. The well-known providers are contacted by fixed IP addresses (bootstrap), so no plain DNS lookup ever happens. A custom URL is bootstrapped once through the system resolver.

## Consequences

- Any RFC 8484 endpoint works, and the resolver is small (~300 lines) and fully unit-tested.
- We own the wire-format parser. It only reads A/AAAA answers and is bounds-checked; malformed answers are errors, never panics.
- Verified live with the system-DNS fallback switched off: every provider resolved through Cloudflare DoH.
