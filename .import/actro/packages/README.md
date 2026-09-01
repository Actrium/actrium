# Packages

JavaScript and React Native packages owned by Actro live here.

## Active Packages

- `actro-runtime`: shared React Native runtime, ACTR host, native bridge
  TypeScript adapters, system apps, and shared assets for iOS, Android, and
  macOS bundling.
- `actr-kotlin`: Android ACTR SDK binding imported from the upstream ACTR
  Kotlin package release.
- `actro-native-kotlin`: Android/Kotlin bindings and stable facade for Actro
  client Rust crates such as `actro-core` and `actro-vault`.
- `askit`: shared guest/host app API and UI helper package.
- `rill`: shared host/guest runtime helpers and native sandbox package.
- `supramark`: shared markdown, diagram, web, and React Native rendering stack.

## Reserved Splits

These directories are tracked as future package boundaries. Do not move
production code into them until the extraction removes real duplication or
matches an agreed architecture step:

- `runtime-host`: host runtime for Actro dynamic apps.
- `system-apps`: built-in Actro apps such as start page, history, bookmarks,
  settings, and global search if they are split out of `actro-runtime`.
- `app-sdk`: guest app SDK surface.
- `browser-model`: shared browser state, workspace, tab, history, bookmark, and
  settings model.

Until those splits happen, keep the working implementation in
`packages/actro-runtime/` and platform-owned shell behavior in `apps/`.
