# Actro

Actro is the Actrium browser client monorepo.

The initial client targets are:

- macOS
- iOS
- Android
- Windows

The first imported clients live in their platform app directories with their
source histories preserved:

- `apps/macos/actro/` - macOS app.
- `apps/ios/actro/` - iOS app.
- `apps/android/actro/` - Android app.
- `apps/windows/actro/` - Windows WebView2 browser shell with native chrome,
  local browser storage, provider sync settings, and a centralized Rust
  ActroCore C ABI adapter.

iOS and Android share the React Native runtime in `packages/actro-runtime/`.
The platform apps keep small local entrypoints and point to the shared runtime
through local symlinks.

## Repository Layout

```text
apps/          Platform app entrypoints.
platform/      Platform-specific shared libraries and adapters.
packages/      TypeScript/React Native runtime packages and system apps.
crates/        Rust client-side native libraries.
third_party/   Pinned legacy source dependencies needed for migration.
schemas/       IPC, browser, and service contracts.
examples/      Example Actro apps and guest packages.
docs/          Architecture decisions and migration notes.
scripts/       Repository automation.
tools/         Developer tooling.
tests/         Cross-platform test fixtures and e2e suites.
```

## Shared Runtime

```text
packages/actro-runtime/       Shared React Native runtime and system apps.
packages/actr-kotlin/         Android Actr Kotlin package.
packages/rill/                Shared host/guest runtime helpers.
packages/askit/               Shared app kit APIs and UI helpers.
packages/supramark/           Markdown, diagram, and native rendering packages.
```

Supramark React Native native artifacts are generated locally because the
staged binaries are large and intentionally ignored by the source packages.
After a fresh checkout, run:

```text
npm run prepare:supramark-native
```

## History-Preserved Imports

Client source must not be copied when history matters. Use history-preserving
Git imports first, then move code with normal Git moves after the imported
commits are anchored in this repository.

```text
apps/macos/actro/
apps/ios/actro/
apps/android/actro/
```

See the migration notes for the current mapping and verification commands:

- `docs/migration/actro.md`
- `docs/migration/mobile-clients.md`
- `docs/migration/windows-client.md`

The full migration verification entrypoint is:

```text
npm run verify:all
```

This includes migration hygiene, Rust core/vault tests, Rust-generated contract
checks, Rust/native boundary hygiene, Windows layout checks, Windows project
verification, shared runtime typecheck, React Native bundles, and
Android/iOS/macOS debug builds.

See `docs/architecture/rust-core.md` for the Rust/native boundary. In short,
shared non-UI protocol, persistence, crypto, parsing, and deterministic data
rules can move into Rust; platform shell routing, UI lifecycle, OS SDK
orchestration, and Windows WebView2 policy stay platform-owned.

`apps/windows/actro/` is the Windows app root. It contains the .NET Windows
Forms + WebView2 browser shell plus `src/ActroCore.cs`, the centralized C#
adapter for the Rust ActroCore C ABI. `npm run verify:windows` validates the
project metadata on every host and runs `dotnet build` whenever a .NET SDK is
available; Windows hosts require the SDK.
