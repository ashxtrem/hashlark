-- Torrents managed by the built-in client. Times are epoch milliseconds (ADR 0006).
CREATE TABLE torrents (
  info_hash      TEXT PRIMARY KEY,           -- 40 lowercase hex
  name           TEXT NOT NULL,
  save_dir       TEXT NOT NULL,
  source         TEXT NOT NULL,              -- magnet | torrent | result | created | watch
  result_id      TEXT,
  added_at       INTEGER NOT NULL,           -- epoch ms (ADR 0006)
  completed_at   INTEGER,
  seeding_since  INTEGER,
  run_mode       TEXT NOT NULL DEFAULT 'auto', -- auto | paused | stopped | forced
  queue_pos      INTEGER NOT NULL,
  peer_limit     INTEGER,                    -- NULL = global per-torrent default
  seed_ratio     REAL,                       -- NULL = global
  seed_minutes   INTEGER,
  down_kib       INTEGER,
  up_kib         INTEGER,
  downloaded_total INTEGER NOT NULL DEFAULT 0,
  uploaded_total   INTEGER NOT NULL DEFAULT 0,
  done           INTEGER NOT NULL DEFAULT 0  -- seed limit reached
);
