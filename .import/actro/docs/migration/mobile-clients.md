# Mobile Client Migration

The iOS and Android first versions were imported as platform clients with
history-preserving subtree merges, then reshaped into the Actro monorepo
layout.

## Source Imports

The import commits are part of the current repository history:

```text
0b338aed279f450f62d0f11ef863722c50e01748
Platform: iOS client history
git-subtree-dir: apps/ios/actro
git-subtree-split: 0dcd8bfd2c01f9138d67460e96bc7a996681514b

9e68d8e41f9dc8c84f2ced7dd8aa2bfe8e52c8e4
Platform: Android client history
git-subtree-dir: apps/android/actro
git-subtree-split: a8421376e213c99673b38c4e36b3d5b62c9a356b
```

Keep those histories reachable. Do not replace the imported clients by copying
files from outside checkouts.

## Current Mapping

```text
apps/ios/actro/                 iOS native app, Xcode project, CocoaPods setup.
apps/ios/actro/ActroRuntime     Symlink to ../../../packages/actro-runtime.
apps/android/actro/             Android React Native app shell and Gradle app.
apps/android/actro/actro-runtime
                                Symlink to ../../../packages/actro-runtime.
packages/actro-runtime/         Shared React Native runtime and system apps.
packages/actr-kotlin/           Android Actr Kotlin package.
packages/askit/                 Shared app APIs and UI helpers.
packages/rill/                  Shared host/guest runtime helpers.
packages/supramark/             Shared markdown, diagram, web, and RN native packages.
```

Platform-specific native code stays in its app directory unless it is clearly a
shared platform adapter. Shared JavaScript, React Native, Supramark, Rill, and
Askit code belongs under `packages/`.

The imported app-local `.github` workflows were removed from the current tree.
They were written for standalone repositories and would not run from nested
monorepo paths. New CI should live under the repository root `.github/`.

## Dedup Rules

- Keep one shared React Native runtime in `packages/actro-runtime/`.
- Keep the iOS `ActroRuntime` and Android `actro-runtime` symlinks pointed at
  the shared runtime package.
- Resolve `rill`, `askit`, and Supramark packages from workspace sources during
  Metro bundling so local edits are not shadowed by stale copied packages.
- Keep React and React Native singletons pinned to the app/runtime dependency
  set. Do not add separate React copies inside shared package source folders.
- Do not commit generated Supramark native binaries. Regenerate them locally
  with the root `npm run prepare:supramark-native` script.

## Fresh Checkout Prep

From the repository root:

```text
npm run prepare:supramark-native

cd packages/rill && bun install
cd ../askit && bun install
cd ../actro-runtime && bun install
```

For iOS, return to the repository root and run:

```text
cd apps/ios/actro
./Scripts/ensure-actro-runtime.sh
pod install
```

For Android, return to the repository root and run:

```text
cd apps/android/actro
yarn install
yarn actro-runtime
```

Android also needs `apps/android/actro/android/local.properties` to point at the
local Android SDK and the intended Node executable.

## Verification Commands

The current migration was verified with:

```text
npm run verify:all
```

`npm run verify:all` covers migration hygiene, the Rust core/vault test surface,
Rust-generated contract freshness, Rust/native boundary hygiene, Windows layout
checks, Windows project verification, shared runtime typecheck, the shared
runtime bundle for iOS, the Android React Native entry, both macOS Actro runtime
entries, and Android/iOS/macOS debug builds. Keep platform-specific manual
commands in app-level READMEs only when debugging a single client.
