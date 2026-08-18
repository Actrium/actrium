# Actro Migration

The macOS client history is imported into Actro at:

```text
apps/macos/actro/
```

## History Rule

The macOS client Git history must remain intact. Do not migrate it by copying
files from another checkout. Use Git operations that preserve the original
commits as repository history.

The initial import used:

```text
git remote add actro-local /Users/example-dev-2/Projects/example-app-2/actro-reference
git fetch actro-local main
git merge -s ours --no-commit --allow-unrelated-histories actro-local/main
git read-tree --prefix=apps/macos/actro/ -u actro-local/main
git commit -m "Import Actro macOS client history"
```

This keeps the Actro commits reachable from Actro while placing the imported
tree under the macOS app directory.

## Current Mapping

```text
Actro/                    -> apps/macos/actro/Actro/
ActroViewService/         -> apps/macos/actro/ActroViewService/
ActroTests/               -> apps/macos/actro/ActroTests/
ActroUITests/             -> apps/macos/actro/ActroUITests/
Packages/ActroIPC/        -> platform/apple/packages/ActroIPC/
Shared/IPC/                -> apps/macos/actro/Shared/IPC/
legacy runtime directory  -> packages/actro-runtime/
docs/                      -> apps/macos/actro/docs/
rill/                      -> packages/rill/
askit/                     -> packages/askit/
```

macOS now references these monorepo locations directly. The legacy runtime and
ActroIPC compatibility symlinks under `apps/macos/actro/` have been removed.

## Migration Phases

1. Anchor the macOS client history in Actro.
2. Verify the imported macOS project builds in place.
3. Replace local path assumptions for Rill and Askit.
4. Extract shared Apple packages into `platform/apple/packages/`.
5. Move Rust service libraries into `crates/`.
6. Move runtime host and system apps into `packages/`.
7. Rename visible product identifiers to Actro.

Each move should be a small commit. Prefer `git mv` for file moves inside this
repository.

## Verified Build

After the first split, the macOS app still builds through the original
workspace:

```text
xcodebuild -workspace Actro.xcworkspace -scheme Actro -configuration Debug -destination platform=macOS CODE_SIGNING_ALLOWED=NO build
```

## Remaining Risks

- Windows has a WebView2 browser shell project at `apps/windows/actro/`, plus
  native tab strip/navigation chrome, generated local history/bookmark pages,
  provider sync settings, and a `window.actroHost` JSON bridge for core status,
  tab lifecycle, tab selection, tab enumeration, and tab-scoped navigation. It
  owns explicit WebView2 profile storage plus page and browser full-screen shell
  transitions; browser-feature parity, background sync, packaging, and
  non-local cloud provider adapters remain in progress. Root `verify:windows`
  validates project metadata on every host and builds whenever a .NET SDK is
  available; Windows hosts require the SDK.
