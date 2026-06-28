# Actro

Actro is the Actrium browser client monorepo.

The first client targets are:

- macOS
- Windows
- Android
- iOS

The original macOS client, Canvas, is imported under
`apps/macos/canvas/` with its Git history preserved. Its first Actro-shaped
source layout now lives behind compatibility symlinks so existing Xcode
references can keep building during the migration.

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

## Canvas Import

Canvas was imported with a history-preserving merge/read-tree flow:

```text
apps/macos/canvas/
```

Do not copy Canvas files into new locations when history matters. Move code in
small, reviewable follow-up commits after the imported history is anchored in
this repository.

The split keeps compatibility symlinks under `apps/macos/canvas/` so the
existing Xcode project can keep building while code moves into `Actro*/`,
`packages/`, `crates/`, and `platform/apple/`.

See `docs/migration/canvas.md` for the migration rules.
