# SPDX-License-Identifier: GPL-3.0-or-later
# Hashlark headless server: API, web UI and Torznab endpoint.
#   docker build -t hashlark .
#   docker run -d -p 8787:8787 -v hashlark-data:/data --name hashlark hashlark
#   docker logs hashlark   # shows the first API key

# --- Web UI -------------------------------------------------------------------
FROM node:24-bookworm-slim AS ui
WORKDIR /src/apps/desktop
RUN corepack enable
COPY apps/desktop/package.json apps/desktop/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY apps/desktop/ ./
RUN pnpm build

# --- Server -------------------------------------------------------------------
FROM rust:1-bookworm AS server
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY definitions/ definitions/
COPY fixtures/ fixtures/
COPY docs/deploy/hashlark.example.toml docs/deploy/hashlark.example.toml
# The desktop crate is part of the workspace but not built here; keep a stub
# manifest so Cargo can load the workspace without the Tauri sources.
COPY apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.toml
RUN mkdir -p apps/desktop/src-tauri/src && echo 'fn main() {}' > apps/desktop/src-tauri/src/main.rs \
 && echo 'fn main() {}' > apps/desktop/src-tauri/build.rs
# The web UI is embedded into the binary.
COPY --from=ui /src/apps/desktop/build apps/desktop/build
RUN cargo build --release --locked -p hashlark-server \
 && cp target/release/hashlark-server /usr/local/bin/

# --- Runtime ------------------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data hashlark \
 && mkdir -p /data && chown hashlark /data
COPY --from=server /usr/local/bin/hashlark-server /usr/local/bin/hashlark-server
USER hashlark
ENV HASHLARK_DATA_DIR=/data \
    HASHLARK_BIND=0.0.0.0:8787
VOLUME ["/data"]
EXPOSE 8787
HEALTHCHECK --interval=60s --timeout=5s \
  CMD ["/bin/sh", "-c", "exec 3<>/dev/tcp/127.0.0.1/8787 && printf 'GET /api/v1/health HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q '\"ok\"' <&3"]
ENTRYPOINT ["/usr/bin/tini", "--", "hashlark-server"]
