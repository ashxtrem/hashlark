// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates the Kotlin bindings of `hashlark-ffi`; the Android Gradle build
//! runs this (`cargo run -p uniffi-bindgen -- generate --library ...`).

fn main() {
    uniffi::uniffi_bindgen_main()
}
