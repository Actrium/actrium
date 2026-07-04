# Crates

Rust client-side libraries live here.

- `actro-core`: cross-platform domain models, package manifest validation,
  native bridge payload contracts, canonical persistence schema, and a UniFFI
  boundary for thin platform adapters.
- `actro-vault`: vault cryptography, password verification, encrypted field
  serialization, category templates, password strength scoring, and UniFFI
  bindings for secure platform adapters.
- `actro-terminal`: SSH, terminal grid, and SFTP engine exported with UniFFI.
- `actro-remote`: remote desktop protocol, video/input plumbing, and ACTR
  transport integration exported with UniFFI.

UI shells remain in `apps/*/actro`. Rust crates should own shared non-UI logic
only; platform code should own rendering, OS permissions, windows, native view
managers, and vendor SDK entrypoints.

Apple bindings for UniFFI crates are generated with each crate's
`build-xcframework.sh` script. `actro-core` and `actro-vault` are consumed by
macOS and iOS through shared local Swift packages under
`platform/apple/packages/*` to keep generated UniFFI symbols isolated per
module. `actro-terminal` still syncs generated Swift into the macOS terminal and
SFTP targets until that older integration is moved behind a package wrapper.

Android bindings are generated with each crate's `build-android.sh` script and
collected in `packages/actro-native-kotlin`, which is included by the Android
app as `:actro-native-kotlin`.

For a fresh checkout or CI worker, run `scripts/build-actro-rust-bindings.sh`
from the repository root. The generated Swift/Kotlin binding sources are small
enough to review, while XCFramework static libraries and Android `.so` files are
local build artifacts and are ignored by git.
