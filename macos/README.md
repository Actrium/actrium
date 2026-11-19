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

To enable the local Guest `Supramark` component, point
`ACTRIUM_SUPRAMARK_DIR` to a Supramark checkout with its workspace dependencies
and macOS parser dylib prepared. Use `react-native-svg` 15.15.3 in that checkout;
its example's 15.12.1 pin fails to compile with macOS Fabric.

```sh
export ACTRIUM_SUPRAMARK_DIR=/path/to/supramark
(cd macos && pod install)
npm run verify:macos
```

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

### Publish a snapshot from a fork

Sync the fork's `main`, enable Actions, then select **Release Actrium macOS →
Run workflow** on `main`. The workflow builds the selected commit from scratch
and uploads the unsigned Apple Silicon DMG and its SHA-256 file to **this
repository's Releases**. It uses the workflow token, not Apple credentials.
Downloads require macOS 26.1 or newer. The app is not notarized; macOS may
block opening it by default.

For workspace version `0.10.7`, the first snapshot is `0.10.7-alpha.1`.
Each new commit gets the next numeric suffix for that version. Rebuilding a
previously tagged commit reuses its tag and replaces only its two assets.
Snapshots are prereleases and never marked Latest. On failure, rerun all jobs.
Build metadata is updated in a marked block in the snapshot release notes.
New snapshots stay in draft until both assets are uploaded. Rerunning a failed
upload resumes the snapshot draft.

For stable releases from `main`, Release Train automatically builds the same
unsigned DMG from the release tag and attaches it and its SHA-256 file to the
existing `vX.Y.Z` Release. A Release Train dry run saves the DMG and checksum
as an Actions artifact. The release report includes the macOS result.
