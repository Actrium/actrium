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
git remote add actro-local /Users/leo/actrium/actro-reference
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
Actro*Service/            -> apps/macos/actro/Actro*Service/
ActroTests/               -> apps/macos/actro/ActroTests/
ActroUITests/             -> apps/macos/actro/ActroUITests/
Packages/ActroIPC/        -> platform/apple/packages/ActroIPC/
Shared/IPC/                -> apps/macos/actro/Shared/IPC/
actro-terminal/           -> crates/actro-terminal/
actro-remote/             -> crates/actro-remote/
legacy runtime directory  -> packages/actro-runtime/
docs/                      -> apps/macos/actro/docs/
rill/                      -> packages/rill/
askit/                     -> packages/askit/
rustdesk-actr/             -> third_party/rustdesk-actr/
actor-rtc/actr pinned rev  -> third_party/actor-rtc-actr/
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

The Rust FFI crates also rebuild from their new locations:

```text
crates/actro-terminal/build-xcframework.sh
crates/actro-remote/build-xcframework.sh
```

`actro-remote` requires the native libraries installed by vcpkg
(`libyuv`, `libvpx`, `aom`) through `VCPKG_ROOT`. The repo declares these in
`vcpkg.json` and provides `scripts/bootstrap-native-deps.sh`, but vcpkg itself
and installed artifacts remain outside the repo.

## Remaining Risks

- The pinned `actor-rtc/actr` source is a legacy compatibility dependency for
  Actro remote desktop only; it is not the intended long-term Actro runtime
  dependency model.
- Windows has an initial WebView2 browser shell project at
  `apps/windows/actro/`, plus native tab strip/navigation chrome and a
  `window.actroHost` JSON bridge for core status, tab lifecycle, tab selection,
  tab enumeration, and tab-scoped navigation. It now also owns explicit WebView2
  profile storage plus page and browser full-screen shell transitions, but is
  still not a production Windows browser client yet. Root `verify:windows`
  validates project metadata on every host and builds whenever a .NET SDK is
  available; Windows hosts require the SDK.
