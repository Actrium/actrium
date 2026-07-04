# Decision 0001: Monorepo Layout

## Status

Accepted.

## Context

Actro needs to contain multiple client projects: macOS first, then the iOS and
Android first versions, followed by Windows. Actro already contains native app
code, XPC services, Rust FFI libraries, React Native runtime host code, Android
Kotlin code, and guest app examples.

Putting all of that under a single flat app directory would make later platform
work depend on Actro-specific paths.

## Decision

Use a product monorepo with explicit top-level ownership:

- `apps/` for platform app entrypoints.
- `platform/` for shared platform-specific libraries.
- `packages/` for JavaScript and React Native packages.
- `crates/` for Rust client libraries.
- `schemas/` for contracts and generated-code inputs.
- `examples/` for guest apps and sample packages.
- `docs/` for architecture and migration records.

The macOS app is imported intact at `apps/macos/actro/` first. The iOS and
Android clients are imported intact at `apps/ios/actro/` and
`apps/android/actro/` with history-preserving subtree merges. Windows starts at
`apps/windows/actro/` with an initial WebView2 shell scaffold. Follow-up
migrations may move code into shared modules, but those moves must happen after
imported histories are anchored so Git history remains auditable.

## Consequences

- Initial imported paths are historical and not always final product paths.
- Root workspaces only include packages that are ready for monorepo dependency
  management.
- App-local package manager, Gradle, CocoaPods, and Xcode behavior remain
  isolated until the migration removes local `file:` assumptions.
