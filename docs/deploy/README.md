# Running the Hashlark headless server

`hashlark-server` is the same search engine as the desktop app. It runs without a window, so it suits a NAS, a home server or a VPS. It provides three things:

- **the web UI** at `http://<host>:8787/`, the same interface as the desktop app;
- **the HTTP API** at `/api/v1` (the OpenAPI document is at `/api/v1/openapi.json`);
- **a Torznab endpoint** at `/torznab/api`, for Sonarr, Radarr and Prowlarr.

## First start

On first start the server creates an **admin API key** and prints it **once** in its log:

```text
  Created the first API key (shown only once):

      hlk_3f0c…
```

Sign in to the web UI with it. You can then create more keys under **Settings → API keys**, one per device or app, so each can be revoked separately. If you lose all keys:

```bash
hashlark-server key create admin      # prints a new key
hashlark-server key list
hashlark-server key revoke <id>
```

The `key` commands work while the server is stopped, with the same `--data-dir` or config.

## Docker

```bash
docker run -d --name hashlark -p 8787:8787 -v hashlark-data:/data ghcr.io/ashxtrem/hashlark:latest
docker logs hashlark          # the first API key
docker exec hashlark hashlark-server key create phone
```

There's also a [`docker-compose.yml`](docker-compose.yml). The image is multi-arch (amd64, arm64) and runs as an unprivileged user. Data lives in `/data`: the database, `secrets.json` (provider passwords, readable only by the server's user) and an optional `hashlark.toml`.

To build the image yourself: `docker build -t hashlark .` from the repository root.

## Linux service (systemd)

See [`hashlark.service`](hashlark.service). It runs as a dedicated user with a hardened sandbox, and `journalctl -u hashlark` shows the first key.

## Windows service

Use [NSSM](https://nssm.cc/) or `sc.exe`:

```powershell
nssm install Hashlark "C:\Program Files\Hashlark\hashlark-server.exe"
nssm set Hashlark AppEnvironmentExtra HASHLARK_BIND=0.0.0.0:8787 HASHLARK_DATA_DIR=C:\ProgramData\Hashlark
nssm set Hashlark AppStderr C:\ProgramData\Hashlark\server.log
nssm start Hashlark
```

## Configuration

Settings come from, highest priority first:

1. command-line flags (`hashlark-server --help`);
2. `HASHLARK_*` environment variables;
3. `hashlark.toml` in the data directory (or `--config <file>`); see [`hashlark.example.toml`](hashlark.example.toml).

| Setting | Flag / variable | Default |
|---|---|---|
| Listen address | `--bind` / `HASHLARK_BIND` | `127.0.0.1:8787` (`0.0.0.0:8787` in Docker) |
| Data directory | `--data-dir` / `HASHLARK_DATA_DIR` | OS app-data folder (`/data` in Docker) |
| TLS | `--tls-cert`, `--tls-key` or `[tls]` | off |
| Web UI | `--no-web-ui` or `web_ui = false` | on |
| Provider secrets | `secrets = "file" \| "keychain"` | `file` |
| Definitions dev folder | `--definitions-dir` | none |
| Log level | `RUST_LOG` or `log =` | `info` |

Search, network (DoH, proxy, Tor), provider and repository settings are made in the web UI, as on the desktop.

## Security

- **Always use HTTPS** when the server is reachable beyond your own machine. API keys are sent with every request, so over plain HTTP anyone on the network can read them. The server logs a warning if it listens on a network address without TLS.
  - Either set `[tls]` with a certificate, or put Hashlark behind a reverse proxy.
  - Example Caddyfile, which gets certificates automatically:

    ```text
    hashlark.example.org {
        reverse_proxy 127.0.0.1:8787
    }
    ```

    With a reverse proxy, keep Hashlark bound to `127.0.0.1`.
- Failed sign-ins are limited to 20 per minute per client address.
- Keys are 256-bit random values, and only their SHA-256 is stored.
- Don't expose the server to the internet without a reason. A VPN such as Tailscale or WireGuard to your home network is the safer way to reach it from outside.

## Sonarr, Radarr and Prowlarr

Add a **Torznab** indexer:

- **URL:** `https://<host>/torznab` (Sonarr/Radarr add `/api` themselves; Prowlarr: use the full `https://<host>/torznab/api`)
- **API key:** create one named after the app under **Settings → API keys**.
- **Categories:** 2000 (Movies), 5000 (TV), 5070 (Anime), 3000 (Audio), 7000 (Books), 4000 (PC), 1000 (Console).

Searches go to every provider enabled in Hashlark.
