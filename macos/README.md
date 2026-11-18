# Actrium for macOS

This directory contains the native macOS browser host, its view service,
shared IPC sources, Xcode projects, and platform-specific tests.

Shared React Native runtime code lives in `../packages/actrium`; Keel lives in
`../packages/keel`; shared Apple packages live in
`../packages/actrium-platform/apple`. The Xcode project and bundle scripts
reference those monorepo paths directly.

From the repository root, compile an unsigned debug build for verification:

```sh
npm run verify:macos
```

For an interactive build that uses Realm credentials, keep a fixed signing
identity across rebuilds:

```sh
IDENTITY_NAME="Apple Development: Your Name (TEAMID)" npm run build:macos
```

The local build and DMG packager both use `scripts/sign-app.sh`: nested
libraries and frameworks, the renderer helper, then the main app. Both process
entitlement files are preserved, and signing must pass
`codesign --verify --deep --strict`. `KEYCHAIN_PATH` can select the keychain
containing the signing identity. Local builds disable secure timestamps;
distribution builds retain them.

`verify:macos` only checks compilation. Opening its unsigned build can cause
repeated Realm Keychain prompts after recompilation: ad hoc code identities
are tied to a single binary. The signed build command rejects missing or ad
hoc identities rather than silently producing an interactive unsigned app.
Changing from an old ad hoc build to a signed build can still require one-time
authorization of the existing Realm credential.

CI Gate selects the macOS Host checks for changes under `macos/` and the
shared build entry points. `bash macos/scripts/test-host.sh` runs the native
focus/blur/resize regression and signs disposable nested bundle fixtures.
These checks do not read Realm private keys or require signing certificates.

Runtime bundles can be generated independently with
`npm run bundle:macos:system`.

## Package a DMG

`scripts/package-dmg.sh` builds a Release app and packages it into a DMG.
Without signing credentials it produces an unsigned DMG:

```sh
# From the repository root
npm run package:macos:dmg -- --unsigned
```

To sign and notarize, provide a Developer ID identity and notarization
credentials (see `scripts/package-dmg.sh --help`):

```sh
IDENTITY_NAME="Developer ID Application: Example (TEAMID)" \
USER_NAME="apple-id@example.com" \
TEAM_ID="TEAMID" \
ACCOUNT_PASSWORD="app-specific-password" \
  macos/scripts/package-dmg.sh
```

Artifacts are written to `macos/dist/` (`Actrium_<tag>.dmg`).
