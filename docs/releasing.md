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

### Android signing key (required for the Android APKs)

Every Android release is signed with one key. **If it is lost, existing installs can no longer be updated**: users would have to uninstall (and lose their data) to move to a new key. Treat it like the updater key above.

1. Generate the keystore once, on your own machine:

   ```bash
   keytool -genkeypair -v -keystore hashlark-release.jks -alias hashlark \
     -keyalg RSA -keysize 4096 -validity 12000
   ```

   (12000 days is over 30 years.) Choose a strong password. You can use the same one for the store and the key.

2. **Back it up**: keep `hashlark-release.jks` and its passwords in a password manager **and** in an offline copy (an encrypted USB stick, for example). Never commit it.
3. Add these repository secrets (Settings → Secrets and variables → Actions):
   - `ANDROID_KEYSTORE_BASE64`: the keystore, base64-encoded (`base64 -w0 hashlark-release.jks`);
   - `ANDROID_KEYSTORE_PASSWORD`: the store password;
   - `ANDROID_KEY_ALIAS`: `hashlark` (or the alias you chose);
   - `ANDROID_KEY_PASSWORD`: the key password.
4. Record the **SHA-256 fingerprint of the signing certificate** and publish it, so users can check an APK before installing it (for example with AppVerifier):

   ```bash
   keytool -list -v -keystore hashlark-release.jks -alias hashlark | grep "SHA256:"
   ```

   The release workflow also prints the fingerprint of every build in its job summary; check that it matches. Put the fingerprint in the release notes of each release.

**Current signing certificate (SHA-256):**

```
5A:90:D8:64:96:45:43:EB:51:43:70:22:99:D7:85:E2:53:A7:D4:98:5E:81:56:AE:CF:2D:DD:3E:B2:C4:64:3D
```

For a local release build, create `apps/android/keystore.properties` (git-ignored):

```properties
storeFile=/absolute/path/to/hashlark-release.jks
storePassword=...
keyAlias=hashlark
keyPassword=...
```

APK Signature Scheme v2 and v3 are used (v1 is not needed from Android 7). v3 lets a later release rotate to a new key with a signing lineage, if that is ever necessary.

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
   - the signed Android APKs: `hashlark-<version>-arm64-v8a.apk`, `-armeabi-v7a.apk`, `-x86_64.apk` and `-universal.apk` (see [Android](#android) below);
   - the multi-arch Docker image;
   - `SHA256SUMS`, the checksums of every file attached to the release.
6. **Android check on a real device**: install the arm64 APK over the previous release (this proves the update path and the signature), then run the manual checklist in [android-plan.md §6](android-plan.md#6-testing) on the Galaxy Z Fold7.
7. Review the **draft** release on GitHub, put the Android signing-certificate fingerprint in the release notes, then publish it. Publishing makes `latest.json` live, so installed apps see the update. The Android app's update notice also only sees **published** releases.

## Android

The Android version (`versionName`) is read from `Cargo.toml`, so it always matches the desktop version, and the release workflow refuses a tag that doesn't match it. `versionCode` is `major·1000000 + minor·10000 + patch·100 + abi` (armeabi-v7a 1, arm64-v8a 2, x86_64 3, universal 9), so an update always has a higher code whichever APK was installed before.

Build locally (needs the Android SDK, NDK 29, JDK 17 and `cargo install cargo-ndk`):

```bash
cd apps/android
./gradlew assembleDebug -Phashlark.abis=arm64-v8a      # only what your phone needs; faster
./gradlew assembleRelease                              # every ABI plus the universal APK; needs keystore.properties
```

Users install by downloading the APK for their phone from the release page (the universal APK works everywhere and is larger) and allowing their browser or file manager to install apps. Obtainium users can add the GitHub repository to get updates.

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
