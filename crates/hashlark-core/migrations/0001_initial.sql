-- SPDX-License-Identifier: GPL-3.0-or-later
-- Initial schema. All timestamps are Unix epoch milliseconds (INTEGER).
-- Credentials are never stored here; they live in the OS keychain.

CREATE TABLE settings (
    key         TEXT PRIMARY KEY NOT NULL,
    value_json  TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE definition_repos (
    id            TEXT PRIMARY KEY NOT NULL,
    url           TEXT NOT NULL UNIQUE,
    name          TEXT,
    public_key    TEXT,
    last_sync_at  INTEGER,
    last_version  INTEGER,
    created_at    INTEGER NOT NULL
);

CREATE TABLE definitions (
    id          TEXT PRIMARY KEY NOT NULL,
    repo_id     TEXT REFERENCES definition_repos(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    sha256      TEXT NOT NULL,
    yaml        TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE providers (
    id                   TEXT PRIMARY KEY NOT NULL,
    kind                 TEXT NOT NULL CHECK (kind IN ('native', 'definition', 'torznab')),
    name                 TEXT NOT NULL,
    enabled              INTEGER NOT NULL DEFAULT 1,
    definition_id        TEXT REFERENCES definitions(id) ON DELETE SET NULL,
    config_json          TEXT NOT NULL DEFAULT '{}',
    network_policy_json  TEXT NOT NULL DEFAULT '{}',
    created_at           INTEGER NOT NULL
);

CREATE TABLE provider_mirrors (
    provider_id  TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    url          TEXT NOT NULL,
    position     INTEGER NOT NULL,
    last_ok_at   INTEGER,
    last_error   TEXT,
    PRIMARY KEY (provider_id, url)
);

CREATE TABLE provider_health (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id  TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    ts           INTEGER NOT NULL,
    ok           INTEGER NOT NULL,
    latency_ms   INTEGER,
    error_kind   TEXT
);
CREATE INDEX idx_provider_health_provider_ts ON provider_health (provider_id, ts DESC);

CREATE TABLE provider_sessions (
    provider_id   TEXT PRIMARY KEY NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    cookies_json  TEXT NOT NULL,
    user_agent    TEXT,
    expires_at    INTEGER
);

CREATE TABLE trackers (
    url       TEXT PRIMARY KEY NOT NULL,
    source    TEXT NOT NULL,
    added_at  INTEGER NOT NULL
);

CREATE TABLE search_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    query_json    TEXT NOT NULL,
    ts            INTEGER NOT NULL,
    result_count  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE favorites (
    result_id      TEXT PRIMARY KEY NOT NULL,
    snapshot_json  TEXT NOT NULL,
    ts             INTEGER NOT NULL
);

CREATE TABLE result_cache (
    cache_key   TEXT PRIMARY KEY NOT NULL,
    payload     TEXT NOT NULL,
    expires_at  INTEGER NOT NULL
);
CREATE INDEX idx_result_cache_expires ON result_cache (expires_at);
