# Canvas Migration

Canvas is the original macOS client and is imported into Actro at:

```text
apps/macos/canvas/
```

## History Rule

The Canvas Git history must remain intact. Do not migrate Canvas by copying
files from another checkout. Use Git operations that preserve the original
commits as repository history.

The initial import used:

```text
git remote add canvas-local /Users/example-dev-2/Projects/example-app-2/canvas-reference
git fetch canvas-local main
git merge -s ours --no-commit --allow-unrelated-histories canvas-local/main
git read-tree --prefix=apps/macos/canvas/ -u canvas-local/main
git commit -m "Import Canvas macOS client history"
```

This keeps the Canvas commits reachable from Actro while placing the imported
tree under the macOS app directory.

## Current Mapping

```text
Canvas/                    -> apps/macos/canvas/Canvas/
Canvas*Service/            -> apps/macos/canvas/Canvas*Service/
Packages/CanvasIPC/        -> apps/macos/canvas/Packages/CanvasIPC/
Shared/IPC/                -> apps/macos/canvas/Shared/IPC/
canvas-terminal/           -> apps/macos/canvas/canvas-terminal/
canvas-remote/             -> apps/macos/canvas/canvas-remote/
RCTNApps/                  -> apps/macos/canvas/RCTNApps/
docs/                      -> apps/macos/canvas/docs/
```

## Migration Phases

1. Anchor Canvas history in Actro.
2. Verify the imported macOS project builds in place.
3. Replace local path assumptions for Rill and Askit.
4. Extract shared Apple packages into `platform/apple/Packages/`.
5. Move Rust service libraries into `crates/`.
6. Move runtime host and system apps into `packages/`.
7. Rename visible Canvas product identifiers to Actro.

Each move should be a small commit. Prefer `git mv` for file moves inside this
repository.

## Current Risks

- `RCTNApps/package.json` still references historical local dependencies:
  `file:../../rill` and `file:../../askit`.
- `canvas-remote` references external local paths from its original development
  environment.
- macOS Xcode schemes and bundle identifiers still use Canvas names.
