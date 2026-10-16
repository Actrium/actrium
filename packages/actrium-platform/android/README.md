# actrium-native-kotlin

Android/Kotlin bindings for Actrium client-side Rust crates.

This module intentionally contains Actrium client primitives, not the ACTR SDK
itself. It is consumed by `android/android`.

Current Rust crates:

- `actrium-core`: ACTR manifest/domain/schema/bridge contracts.
- `actrium-vault`: vault crypto, field serialization, categories, and password
  strength scoring.

Generated Kotlin files live under `src/main/kotlin/uniffi/*`. Android native
libraries live under `src/main/jniLibs/*` and are produced with each crate's
Android build path:

- `crates/actrium-core/build-android.sh`
- `crates/actrium-vault/build-android.sh`

The scripts currently build `arm64-v8a` and `x86_64` and copy UniFFI loader-name
aliases (`libuniffi_actrium_core.so`, `libuniffi_actrium_vault.so`) next to the
canonical Cargo shared libraries.

App code should prefer the stable facade package `io.actrium.app.rust`
instead of importing `uniffi.*` directly.
