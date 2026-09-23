# Actrium for macOS

This directory contains the native macOS browser host, its view service,
shared IPC sources, Xcode projects, and platform-specific tests.

Shared React Native runtime code lives in `../packages/actrium`; Keel lives in
`../packages/keel`; shared Apple packages live in
`../packages/actrium-platform/apple`. The Xcode project and bundle scripts
reference those monorepo paths directly.

## Fresh checkout prep

The Apple SwiftPM packages (`ActriumCore`, `ActriumVault`) reference local
XCFrameworks whose `.a` binaries are intentionally not committed (see
`.gitignore`). A clean checkout must generate them once before Xcode or
`swift build`/`swift test` can succeed:

```sh
# 1. JavaScript workspace dependencies (react-native-macos et al.)
pnpm install

# 2. CocoaPods for the app project
brew install cocoapods
(cd macos && pod install)

# 3. Rust XCFramework libraries + Swift bindings (required step;
#    without it the ActriumCore/ActriumVault packages cannot build)
#    apple-macos builds the macOS slices only (fast) but strips the
#    committed iOS headers from the working tree; use `apple` for the
#    full iOS+macOS build, or restore headers with
#    `git checkout -- crates/actrium-*/*.xcframework` after apple-macos.
scripts/actrium/build-actrium-rust-bindings.sh apple-macos
```

Re-run step 3 whenever the Rust crates change or the generated Swift
bindings drift.

## Build and test

From the repository root, run the unsigned debug build with:

```sh
npm run verify:macos
```

Runtime bundles can be generated independently with
`npm run bundle:macos:all`.
