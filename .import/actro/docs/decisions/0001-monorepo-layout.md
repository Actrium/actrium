# Decision 0001: Monorepo Layout

## Status

Accepted.

## Context

Actro needs to contain multiple client projects: the original macOS Actro
client first, followed by Windows, Android, and iOS clients. Actro already
contains native app code, XPC services, Rust FFI libraries, React Native runtime
host code, and guest app examples.

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

Actro is imported intact at `apps/macos/actro/` first. Follow-up migrations
may move code into shared modules, but those moves must happen in separate
commits so Git history remains auditable.

## Consequences

- Initial macOS paths are historical and not final product paths.
- Root workspaces only include packages that are ready for monorepo dependency
  management.
- Actro-local package manager and Xcode behavior remain isolated until the
  migration removes local `file:` assumptions.
