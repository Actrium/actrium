# Actro Architecture Overview

Actro is a multi-platform browser client for the Actrium ecosystem. The client
repo owns product shells, native integrations, runtime host packages, and
client-specific services. The core ACTR runtime remains in the `actr`
repository and is consumed through released or local SDKs.

## Product Layers

1. Platform apps live in `apps/`.
2. Shared platform adapters live in `platform/`.
3. JavaScript and React Native runtime packages live in `packages/`.
4. Native Rust client libraries and reference executors live in `crates/`.
5. Cross-language contracts live in `schemas/`.

Browser sync uses user-owned cloud storage as the product sync medium, not an
Actro-operated sync service. See
[`browser-sync-cloud-drive.md`](browser-sync-cloud-drive.md) for the cloud drive
provider architecture.

## Initial Platform Strategy

- macOS starts from the imported macOS app codebase.
- iOS starts from the imported iOS client under `apps/ios/actro/` and
  should share Apple packages where WebKit, account, and Actr SDK behavior can
  be common.
- Android starts from the imported Android client under `apps/android/actro/`,
  consumes `packages/actr-kotlin/`, and keeps Android lifecycle behavior in
  `apps/android` and `platform/android`.
- iOS and Android share the React Native runtime and system apps from
  `packages/actro-runtime/`.
- Windows has a WebView2 browser shell at `apps/windows/actro/`, with native
  tab/navigation chrome, explicit WebView2 profile storage, generated local
  history/bookmark pages, provider sync settings, page and browser full-screen
  shell handling, and a `window.actroHost` browser bridge. The browser host
  consumes shared non-UI behavior through the Actro Rust/native boundary while
  browser-feature parity and packaging continue.

## Repository Boundary

Actro should not vendor the ACTR runtime. It should depend on:

- SwiftPM package `Actr` for Apple targets.
- Maven package `io.actrium:actr` for Android.
- npm package `@actrium/actr` where Node or desktop tooling needs it.
- Local overrides only for active development against `/Users/example-dev-2/Projects/example-app-2/actr`.
