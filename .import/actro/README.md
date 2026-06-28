# Actro

Actro is the Actrium browser client monorepo.

The first client targets are:

- macOS
- Windows
- Android
- iOS

The original macOS client, Canvas, is imported under
`apps/macos/canvas/` with its Git history preserved. Treat that directory as
the migration source of record until the macOS app is renamed and split into
shared modules.

## Repository Layout

```text
apps/          Platform app entrypoints.
platform/      Platform-specific shared libraries and adapters.
packages/      TypeScript/React Native runtime packages and system apps.
crates/        Rust client-side native libraries.
schemas/       IPC, browser, and service contracts.
examples/      Example Actro apps and guest packages.
docs/          Architecture decisions and migration notes.
scripts/       Repository automation.
tools/         Developer tooling.
tests/         Cross-platform test fixtures and e2e suites.
```

## Canvas Import

Canvas was imported with a history-preserving merge/read-tree flow:

```text
apps/macos/canvas/
```

Do not copy Canvas files into new locations when history matters. Move code in
small, reviewable follow-up commits after the imported history is anchored in
this repository.

See `docs/migration/canvas.md` for the migration rules.
