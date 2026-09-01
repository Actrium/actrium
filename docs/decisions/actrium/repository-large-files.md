# Repository large-file policy

Migration completed: 2026-07-29.

The source Actro repository initially contained 15 tracked blobs over 1,000,000
bytes, totalling 206,485,114 bytes (196.92 MiB). The Android Vosk models and
Actr JNI libraries moved to checksum-verified build-time generation before the
monorepo import. After the Actrium integration and the exclusion of Supramark,
the current tree contains three reviewed blobs over 1,000,000 bytes, totalling
4,608,896 bytes (4.40 MiB).

| Category | Baseline bytes | Current delivery |
| --- | ---: | --- |
| Android Vosk model blobs over 1 MiB | 138,752,455 (139,191,238 for both complete model trees) | Generated into app assets from two official, pinned archives |
| `libactr.so` for two Android ABIs | 60,162,760 (61,212,896 for all four JNI libraries including `libopus.so`) | Generated into `actr-kotlin` JNI inputs from the pinned `v0.4.4` release asset |
| Vendored QuickJS source | 1,780,706 | Keep vendored; enforce license, version, and checksum provenance |
| `browser_data.rs` | 1,827,956 | Split for maintainability, not repository-size savings |
| iOS icon SVG | 1,000,242 | Keep as a reviewed source asset |

## Enforcement

`.github/actrium-large-files-allowlist.txt` is a reviewed exception list with a
byte budget per path. `scripts/actrium/check-large-files.sh` rejects any new tracked blob
over 1,000,000 bytes and rejects growth beyond an existing budget. Use
`scripts/actrium/check-large-files.sh --staged` before committing.

External Android inputs are pinned in
`android/android/binary-assets.lock.properties`; the JDK downloader
verifies archives before staging and atomically replacing generated outputs.
Large vendored sources remain pinned in `.github/actrium-vendored-assets.sha256`.
CI validates the lock and runs both repository checks.

## Android generated inputs

`prepareAsrModels` and `prepareActrNativeLibraries` run before their modules'
`preBuild` tasks. Verified archives are cached in
`$GRADLE_USER_HOME/caches/actro-binary-assets`; a warm cache supports Gradle
`--offline`. Clean build outputs are generated under each module's `build/`
directory and remain ignored by Git.

The app still bundles both ASR models in the APK, so first-run speech
recognition remains offline. This change reduces repository and clone payload,
not APK or installed-app size.

## Remaining work

1. **`browser_data.rs`.** Extract the 14,545-line test module first, then split
   production code by schema/SQL, domain policies, cloud-store adapters,
   planning/execution, and serialization. Preserve the public module exports
   and FFI payloads. This improves ownership and reviewability but does not
   materially shrink Git.
2. **Vendored source.** Keep QuickJS as source because it is compiled and
   locally patched. Package-manager generation is an optional build-system
   change, not an automatic repository improvement.

## Source repository history cleanup

History cleanup completed on 2026-07-29 after a full mirror backup and
`git fsck` verification.

`main` was rewritten with exact path filters for the former Android binary
inputs:

- `android/app/src/main/assets/model/asr/`
- `android/android/app/src/main/assets/model/asr/`
- `packages/actr-kotlin/src/main/jniLibs/`

The first path is the model location before the Android project moved under
`android`; filtering both locations prevents the same model blobs
from surviving under an earlier name. The rewritten tip kept the exact same
tree as the pre-rewrite tip.

A malformed Dependabot branch based on a separate legacy root history was also
deleted. That branch, rather than `main`, retained the 45-90 MiB
`CanvasTerminal.xcframework` libraries, `default.profraw` files, Rust `target/`
outputs, and Swift `.build/` outputs. Its pull request showed 10,989 unrelated
changed files and could not be reused safely.

After expiring the backed-up local reflogs and pruning unreachable objects,
the local Git object pack fell from 265.70 MiB at audit time to 46.06 MiB.
Reachable `main` history fell from 25 blobs over 1,000,000 bytes totalling
212.11 MiB to 15 historical versions totalling 22.41 MiB. The current tree
contains only the three reviewed large files described above.

Because commit IDs changed, clones made before the rewrite must be replaced
with fresh clones. Do not merge or push an old local branch into the rewritten
history.
