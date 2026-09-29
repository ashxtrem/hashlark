# Releasing Hashlark

## One-time setup

### Updater signing key (required)

The desktop app installs updates only if they're signed with the key whose public half is in `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).

- A key pair was generated in `~/.tauri/hashlark-updater.key` (private) and `.pub` (public). The private key has no password.
- **Back it up somewhere safe.** If it's lost, existing installs can't be updated automatically any more.
- Add these repository secrets (Settings → Secrets and variables → Actions):
  - `TAURI_SIGNING_PRIVATE_KEY`: the contents of `hashlark-updater.key`;
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: empty, or the password if you regenerate the key with one (`pnpm tauri signer generate -w <file>`).

If you regenerate the key, put the new public key into `tauri.conf.json` before the next release.

### Windows code signing (strongly recommended)

Unsigned installers trigger SmartScreen warnings. The options are:

- **Azure Trusted Signing** (cheapest current option): see the Tauri guide "Windows code signing" and add the Azure secrets and a `signCommand` in `tauri.conf.json`.
- **An OV/EV certificate**: set `bundle.windows.certificateThumbprint` and import the certificate in the workflow.

### macOS signing and notarization (optional)

This needs an Apple Developer account. Add these secrets: `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific password) and `APPLE_TEAM_ID`. Then pass them as environment variables on the `tauri-action` step in `.github/workflows/release.yml`. Until those variables are set, macOS builds are unsigned and users must right-click → Open.

### Container registry

The Docker image is pushed to `ghcr.io/<owner>/<repo>` with the built-in `GITHUB_TOKEN`; nothing to set up.

## Cutting a release

1. Update `CHANGELOG.md`.
2. Bump the version in `Cargo.toml` (`workspace.package.version`), `apps/desktop/package.json` and `apps/desktop/src-tauri/tauri.conf.json`.
3. Check everything passes:

   ```bash
   cargo test --workspace --exclude hashlark-desktop
   cargo clippy --workspace --all-targets -- -D warnings
   cargo run -p hashlark-cli -- defs lint definitions/builtin/*.yml
   cd apps/desktop && pnpm check && pnpm test
   ```

4. Tag and push: `git tag v1.0.0 && git push origin v1.0.0`.
5. The **Release** workflow builds:
   - desktop installers (Windows NSIS/MSI, macOS DMG for both architectures, Linux AppImage/deb) and `latest.json` for the updater;
   - `hashlark-server` archives for Linux (x86_64, arm64), Windows and macOS;
   - the multi-arch Docker image.
6. Review the **draft** release on GitHub, then publish it. Publishing makes `latest.json` live, so installed apps see the update.

## Building locally

```bash
cd apps/desktop
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/hashlark-updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" pnpm tauri build
# Installers: target/release/bundle/{nsis,msi}/
```

## First-party definitions

The definitions in `definitions/builtin/` are compiled into the app. A newer version reaches users with the next app release (see `install_first_party` in `crates/hashlark-core/src/engine/providers.rs`).

To ship definition fixes between releases, publish them as a signed repository:

```bash
cargo run -p hashlark-cli -- repo build definitions/builtin --key <signing key> --name "Hashlark" --version <n>
```
