# Actrium Migration

The macOS client history is imported into Actrium at:

```text
macos/
```

## History Rule

The macOS client Git history must remain intact. Do not migrate it by copying
files from another checkout. Use Git operations that preserve the original
commits as repository history.

The initial import used:

```text
git remote add actrium-local /Users/leo/actrium/actrium-reference
git fetch actrium-local main
git merge -s ours --no-commit --allow-unrelated-histories actrium-local/main
git read-tree --prefix=macos/ -u actrium-local/main
git commit -m "Import Actrium macOS client history"
```

This keeps the Actrium commits reachable from Actrium while placing the imported
tree under the macOS app directory.

## Current Mapping

```text
Actrium/                    -> macos/Actrium/
ActriumViewService/         -> macos/ActriumViewService/
ActriumTests/               -> macos/ActriumTests/
ActriumUITests/             -> macos/ActriumUITests/
Packages/ActriumIPC/        -> packages/actrium-platform/apple/ActriumIPC/
Shared/IPC/                -> macos/Shared/IPC/
legacy runtime directory  -> packages/actrium/
docs/                      -> macos/docs/
keel/                      -> packages/keel/
askit/                     -> packages/actrium-app-sdk/
```

macOS now references these monorepo locations directly. The legacy runtime and
ActriumIPC compatibility symlinks under `macos/` have been removed.

## Migration Phases

1. Anchor the macOS client history in Actrium.
2. Verify the imported macOS project builds in place.
3. Replace local path assumptions for Keel and Askit.
4. Extract shared Apple packages into `packages/actrium-platform/apple/`.
5. Move Rust service libraries into `crates/`.
6. Move runtime host and system apps into `packages/`.
7. Rename visible product identifiers to Actrium.

Each move should be a small commit. Prefer `git mv` for file moves inside this
repository.

## Verified Build

After the first split, the macOS app still builds through the original
workspace:

```text
xcodebuild -workspace Actrium.xcworkspace -scheme Actrium -configuration Debug -destination platform=macOS CODE_SIGNING_ALLOWED=NO build
```

## Remaining Risks

- Windows has a WebView2 browser shell project at `windows/`, plus
  native tab strip/navigation chrome, generated local history/bookmark pages,
  provider sync settings, and a `window.actriumHost` JSON bridge for core status,
  tab lifecycle, tab selection, tab enumeration, and tab-scoped navigation. It
  owns explicit WebView2 profile storage plus page and browser full-screen shell
  transitions; browser-feature parity, background sync, packaging, and
  non-local cloud provider adapters remain in progress. Root `verify:windows`
  validates project metadata on every host and builds whenever a .NET SDK is
  available; Windows hosts require the SDK.
