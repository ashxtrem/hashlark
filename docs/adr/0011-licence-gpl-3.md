# 0011. Licence: GPL-3.0-or-later

- **Status:** Accepted
- **Date:** 2026-09-28

## Context

We wanted a copyleft licence so that forks stay open. We compared GPL-2.0 and GPL-3.0.

## Decision

License all Hashlark code under **GPL-3.0-or-later**. Every source file carries `SPDX-License-Identifier: GPL-3.0-or-later`, and `cargo deny` enforces a GPL-3.0-compatible allow-list of dependency licences (`deny.toml`).

We rejected GPL-2.0 because:

- Apache-2.0 is incompatible with GPL-2.0. Several dependencies require Apache-2.0: `ring` and `aws-lc-sys` (HTTPS), `minijinja`, and `librqbit`.
- GPL-3.0 adds a patent licence, anti-tivoization terms, and a 30-day grace period for fixing accidental violations.

## Consequences

- Provider definitions are data loaded at runtime, not linked code. Definition repos may therefore carry their own licence, e.g. a community repo of Jackett-derived definitions can stay GPL-2.0.
- The Cardigann importer converts Jackett definitions on the user's machine; we don't redistribute them.
- GPL-3.0 doesn't require sharing source with users of a network service (the headless server). AGPL-3.0 would; we chose not to use it.
