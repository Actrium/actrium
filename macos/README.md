# Actrium for macOS

This directory contains the native macOS browser host, its view service,
shared IPC sources, Xcode projects, and platform-specific tests.

Shared React Native runtime code lives in `../packages/actrium`; Keel lives in
`../packages/keel`; shared Apple packages live in
`../packages/actrium-platform/apple`. The Xcode project and bundle scripts
reference those monorepo paths directly.

From the repository root, run the unsigned debug build with:

```sh
npm run verify:macos
```

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
