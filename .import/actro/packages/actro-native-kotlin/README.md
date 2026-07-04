# actro-native-kotlin

Android/Kotlin bindings for Actro client-side Rust crates.

This module intentionally contains Actro client primitives, not the ACTR SDK
itself. It is consumed by `apps/android/actro/android`.

Current Rust crates:

- `actro-core`: ACTR manifest/domain/schema/bridge contracts.
- `actro-vault`: vault crypto, field serialization, categories, and password
  strength scoring.

Generated Kotlin files live under `src/main/kotlin/uniffi/*`. Android native
libraries live under `src/main/jniLibs/*` and are produced with each crate's
Android build path:

- `crates/actro-core/build-android.sh`
- `crates/actro-vault/build-android.sh`

The scripts currently build `arm64-v8a` and `x86_64` and copy UniFFI loader-name
aliases (`libuniffi_actro_core.so`, `libuniffi_actro_vault.so`) next to the
canonical Cargo shared libraries.

App code should prefer the stable facade package `io.actrium.actro.rust`
instead of importing `uniffi.*` directly.
