# Hashlark for Android

A standalone Android app: the Rust core runs inside the app (through UniFFI), and the UI is Kotlin with Jetpack Compose and Material 3. Plan and decisions: [docs/android-plan.md](../../docs/android-plan.md).

```
app/    the app: Compose screens, adaptive layouts, Android integrations (intents, shortcuts, WorkManager, WebView)
core/   Kotlin wrapper of the engine: models, Flow of search events, Keystore secret store, update check
ffi/    builds the Rust core with cargo-ndk, generates the Kotlin bindings, packages the .so files
```

The native library and the bindings are built by Gradle (`:ffi:cargoNdk`, `:ffi:uniffiBindgen`), so `./gradlew assembleDebug` is the only command needed.

## Requirements

- JDK 17 and the Android SDK with platform 36, build-tools 36 and **NDK 29.0.13599879** (`sdkmanager "ndk;29.0.13599879"`).
- Rust stable with the Android targets, and `cargo-ndk`:

  ```bash
  rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
  cargo install cargo-ndk
  ```

- `ANDROID_HOME` (or `sdk.dir` in `local.properties`). `ANDROID_NDK_HOME` is optional; it defaults to the NDK above.

## Everyday commands

```bash
./gradlew assembleDebug -Phashlark.abis=arm64-v8a     # build only the ABI your device needs (much faster)
./gradlew installDebug  -Phashlark.abis=x86_64        # install on a running emulator
./gradlew testDebugUnitTest                           # Kotlin unit tests and the layout tests (no device needed)
./gradlew lint
./gradlew connectedDebugAndroidTest -Phashlark.abis=x86_64   # instrumented tests on a device or emulator
```

The first build compiles Arti and the rest of the core for Android and takes several minutes; later builds only rebuild what changed. The layout tests save a picture of the search screen for each window shape in `app/build/screenshots/`.

## Trying the foldable layouts

Create an emulator from the **Pixel Fold** hardware profile (API 34 or newer), then change its fold state from a shell:

```bash
adb shell cmd device_state print-states
adb shell cmd device_state state 0   # closed: the cover screen
adb shell cmd device_state state 1   # half-open: book posture, or tabletop after rotating to portrait
adb shell cmd device_state state 2   # open: the inner screen
adb shell settings put system accelerometer_rotation 0
adb shell settings put system user_rotation 1
```

## Release builds

See [docs/releasing.md](../../docs/releasing.md#android). Release APKs are signed with the key in `keystore.properties` (git-ignored) or the `ANDROID_KEYSTORE_*` environment variables; without either the release APK is unsigned.

Dependency versions are pinned in `gradle/libs.versions.toml`.
