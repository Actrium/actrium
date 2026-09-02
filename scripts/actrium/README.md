# Scripts

Repository automation lives here.

Keep platform-specific scripts close to the app or package that owns them unless
the script is used across multiple top-level areas.

- `build-actrium-rust-bindings.sh` rebuilds local native outputs for Actrium Rust
  client crates. Use `apple`, `android`, or `all` to regenerate XCFrameworks,
  Swift binding sources, Kotlin bindings, Android JNI libraries, and
  Rust-generated compile-time contracts. Use `windows` separately on a Windows
  Rust host to build and copy the ActriumCore DLL used by the Windows shell. Heavy
  binary slices are intentionally ignored and should be regenerated locally or in
  CI.
- `PrepareAndroidBinaryAssets.java` is the JDK-only downloader used by Android
  Gradle `preBuild` tasks. It reads the pinned Android asset lock, verifies
  archive SHA-256 values, safely extracts through a staging directory, and
  atomically generates the offline Vosk assets and Actr JNI libraries. Verified
  archives remain available to Gradle `--offline` builds through the Gradle user
  cache.
- `check-large-files.sh` rejects new tracked files over 1,000,000 bytes unless
  they have an explicit reviewed budget. `verify-vendored-assets.sh` verifies
  the remaining large vendored source bundles.
- `check-migration-hygiene.sh` verifies migration invariants that should stay
  true after the client imports: old product/repository names are absent from
  first-party files, old app/client names are absent from first-party paths,
  tracked app files stay under the `android/`, `ios/`, `macos/`, and `windows/`
  platform roots, platform READMEs describe their current app roots, mobile app
  runtime links point at `packages/actrium`, the macOS compatibility layout
  points at monorepo package/runtime locations, reserved package boundaries
  have README files, `internal/` remains git-ignored, and temporary import
  trees under `.import/actrium/` are absent.
- `check-retired-protocol-names.sh` rejects identifiers from the superseded
  package split in the current tree and history reachable from `HEAD`. CI
  checks out the full history before invoking it, but unrelated branches and
  unreachable objects do not affect the result. The audit checks text blobs,
  tree entry names, and commit messages, and reports object IDs and locations.
  Binary blobs and raw tree object IDs are excluded. Use `--all-refs` to audit
  history reachable from every local ref during an explicit repository audit.
- `rewrite-retired-protocol-history.sh` performs the corresponding one-time,
  confirmation-gated rewrite after an external backup has been created.
- `check-generated-contracts.sh` regenerates the lightweight Rust-owned
  TypeScript and Android compile-time contract sources into a temporary
  directory and verifies that the checked-in copies are current. It also checks
  that React Native runtime source imports the TypeScript contract through the
  stable `runtime-host/coreContract.ts` adapter instead of directly importing the
  generated file.
- `check-rust-boundary-hygiene.sh` protects the native RN/UI boundary files that
  have already been moved behind local Swift/Kotlin adapters. It intentionally
  does not ban `ActriumCore` imports at real package-loading, database, model,
  ACTR, ASR, TTS, or Email native boundaries. It also keeps Windows P/Invoke
  access centralized in `windows/src/ActriumCore.cs`.
- `check-windows-layout.sh` verifies the current Windows project contract:
  `windows/` exists, WebView2 remains the expected shell host,
  the browser application context owns multi-window
  lifecycle including private-window creation/cleanup, startup options expose
  private-window mode plus positional/repeated initial addresses, the browser
  tab strip/lifecycle is present, duplicate-tab creation is present, the browser
  tab keyboard shortcuts and closed-tab reopening state are present, the browser
  toolbar/navigation loop is present, address bar search fallback is present,
  navigation failure error pages reload the original address, WebView2
  link-hover status text is
  synchronized, the desktop menu command surface is present, native file opening
  is exposed, page address copy/default-browser actions are present, WebView2
  save-as-PDF and print UI are exposed, WebView2 browsing-data clearing is
  present, the WebView2 download manager, download policy,
  open-downloads-folder command, and recent-download open/reveal commands are
  present, the external-protocol manager and policy are present, the WebView2
  permission manager and permission policy are present, the WebView2 page-find
  bar, find policy, and repeated-find shortcuts are present, the WebView2 zoom
  policy is present, WebView2 new-window requests are handled as tab openings,
  WebView2 page full-screen and F11 browser full-screen are handled by the
  Windows shell, the WebView2 normal/private profile user-data policy is
  explicit, the WebView2
  host message bridge delegates to the local host bridge protocol,
  browser host methods are present, page-initiated unsafe navigations are
  intercepted, the Windows-local navigation policy, browser session store,
  primary-window state store, and lightweight tests are present, the ActriumCore C
  ABI adapter is present, and `verify:windows` is wired to the Windows verifier.

## Root Verification Shortcuts

The repository root `package.json` exposes the main migration checks:

- `npm run verify:all` runs the full migration verification chain: migration
  hygiene, Rust core/vault tests, Apple ActriumCore Swift package tests, generated
  contract checks, Rust boundary hygiene, Windows layout checks, shared runtime
  typecheck, React Native bundles, and Android/iOS/macOS debug builds.
- `npm run check:generated` verifies that Rust-generated lightweight contract
  sources are present, up to date, and consumed through stable local adapters.
- `npm run check:rust-boundary` verifies that adapter-protected RN/UI native
  files do not import Rust bindings directly and that Windows native core access
  stays centralized.
- `npm run check:windows-layout` verifies the current Windows app layout and
  project metadata wiring.
- `npm run check:bundles` builds the iOS, Android, and macOS React Native bundles.
  macOS covers both Actrium runtime entries: system apps and ACTR host.
- `npm run check:migration` runs repository migration hygiene checks.
- `npm run check:repository-assets` validates the Android artifact lock, large
  file budgets, and vendored source checksums.
- `npm run verify:rust` runs the core Rust contract/vault tests that back the
  shared native boundary.
- `npm run verify:apple-core` runs the ActriumCore Swift package tests, including
  Apple browser storage v1/v2/v3-to-v6 SQLite migration fixtures.
- `npm run verify:runtime` runs the shared React Native runtime TypeScript
  check used by `verify:all`.
- `npm run verify:android` builds the Android debug app and instrumentation
  test APK, including browser storage v1/v2/v3-to-v6 migration fixtures.
- `npm run verify:android-device` runs the Android instrumentation tests on a
  connected device or emulator.
- `npm run verify:ios` builds the iOS app for a generic device with code signing
  disabled.
- `npm run verify:macos` builds the macOS app for the current machine architecture
  with code signing disabled.
- `npm run verify:windows` validates the Windows WebView2 project metadata on
  every host and runs the lightweight Windows navigation/session/bridge behavior
  tests plus `dotnet build` whenever a .NET SDK is available. Windows hosts
  require the SDK.

Use targeted platform commands while iterating, then prefer these root
shortcuts for final migration verification.

Use `npm run ...` for these root shortcuts during migration; `pnpm ...` can
trigger workspace install and lifecycle hooks before running the script in a
mixed-manager checkout.
