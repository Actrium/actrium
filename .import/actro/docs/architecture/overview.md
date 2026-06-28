# Actro Architecture Overview

Actro is a multi-platform browser client for the Actrium ecosystem. The client
repo owns product shells, native integrations, runtime host packages, and
client-specific services. The core Actrium runtime remains in the `actr`
repository and is consumed through released or local SDKs.

## Product Layers

1. Platform apps live in `apps/`.
2. Shared platform adapters live in `platform/`.
3. JavaScript and React Native runtime packages live in `packages/`.
4. Native Rust client libraries live in `crates/`.
5. Cross-language contracts live in `schemas/`.

## Initial Platform Strategy

- macOS starts from the imported Canvas codebase.
- iOS should share Apple packages where WebKit, account, and Actr SDK behavior
  can be common.
- Android consumes the Actr Kotlin package and keeps Android lifecycle behavior
  in `apps/android` and `platform/android`.
- Windows consumes the TypeScript or native Actr SDK path, with the browser
  host expected to use WebView2.

## Repository Boundary

Actro should not vendor the full `actr` runtime. It should depend on:

- SwiftPM package `Actr` for Apple targets.
- Maven package `io.actrium:actr` for Android.
- npm package `@actrium/actr` where Node or desktop tooling needs it.
- Local overrides only for active development against `/Users/example-dev-2/Projects/example-app-2/actr`.
