# Mobile Client Migration

The iOS and Android first versions were imported as platform clients with
history-preserving subtree merges, then reshaped into the Actro monorepo
layout.

## Source Imports

The import commits are part of the current repository history:

```text
0b338aed279f450f62d0f11ef863722c50e01748
Platform: iOS client history
git-subtree-dir: ios
git-subtree-split: 0dcd8bfd2c01f9138d67460e96bc7a996681514b

9e68d8e41f9dc8c84f2ced7dd8aa2bfe8e52c8e4
Platform: Android client history
git-subtree-dir: android
git-subtree-split: a8421376e213c99673b38c4e36b3d5b62c9a356b
```

Keep those histories reachable. Do not replace the imported clients by copying
files from outside checkouts.

## Current Mapping

```text
ios/                 iOS native app, Xcode project, CocoaPods setup.
ios/ActroRuntime     Symlink to ../packages/actrium.
android/             Android React Native app shell and Gradle app.
android/actro-runtime
                     Symlink to ../packages/actrium.
packages/actrium/          Shared React Native runtime and system apps.
packages/actr-kotlin/           Android Actr Kotlin package.
packages/actrium-app-sdk/       Shared app APIs and UI helpers.
packages/keel/                  Shared host/guest runtime helpers.
```

Platform-specific native code stays in its app directory unless it is clearly a
shared platform adapter. Shared JavaScript, React Native, Keel, and app SDK code
belongs under `packages/`. Supramark is intentionally not part of Actrium and
the imported browser no longer has a direct or transitive workspace dependency
on it.

The imported app-local `.github` workflows were removed from the current tree.
They were written for standalone repositories and would not run from nested
monorepo paths. New CI should live under the repository root `.github/`.

## Dedup Rules

- Keep one shared React Native runtime in `packages/actrium/`.
- Keep the iOS `ActroRuntime` and Android `actro-runtime` symlinks pointed at
  the shared runtime package.
- Resolve `keel` and the Actrium app SDK from workspace sources during Metro
  bundling so local edits are not shadowed by stale copied packages.
- Keep React and React Native singletons pinned to the app/runtime dependency
  set. Do not add separate React copies inside shared package source folders.

## Fresh Checkout Prep

From the repository root:

```text
pnpm install
```

For iOS, return to the repository root and run:

```text
cd ios
./Scripts/ensure-actro-runtime.sh
pod install
```

For Android, return to the repository root and run:

```text
cd android
yarn install
yarn actro-runtime
```

Android also needs `android/android/local.properties` to point at the
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
