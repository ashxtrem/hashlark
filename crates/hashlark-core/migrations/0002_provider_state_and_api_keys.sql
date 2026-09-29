-- SPDX-License-Identifier: GPL-3.0-or-later
-- Provider health state (auto-disable with backoff) and API keys for
-- headless mode.

ALTER TABLE providers ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;
-- Set when the provider is auto-disabled; it is retried after this time.
ALTER TABLE providers ADD COLUMN disabled_until INTEGER;
-- How many times in a row it has been auto-disabled (drives the backoff).
ALTER TABLE providers ADD COLUMN disable_count INTEGER NOT NULL DEFAULT 0;

CREATE TABLE api_keys (
    id            TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    -- SHA-256 of the key, hex. Keys are 256-bit random, so no slow hash is needed.
    key_hash      TEXT NOT NULL UNIQUE,
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER
);
