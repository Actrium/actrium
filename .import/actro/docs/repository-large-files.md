# Repository large-file policy

Migration completed: 2026-07-29.

The baseline `HEAD` contained 15 tracked blobs over 1,000,000 bytes, totalling
206,485,114 bytes (196.92 MiB). The Android Vosk models and Actr JNI libraries
have now moved to checksum-verified build-time generation. The resulting tree
contains five tracked blobs over 1,000,000 bytes, totalling 7,569,899 bytes
(7.22 MiB).

| Category | Baseline bytes | Current delivery |
| --- | ---: | --- |
| Android Vosk model blobs over 1 MiB | 138,752,455 (139,191,238 for both complete model trees) | Generated into app assets from two official, pinned archives |
| `libactr.so` for two Android ABIs | 60,162,760 (61,212,896 for all four JNI libraries including `libopus.so`) | Generated into `actr-kotlin` JNI inputs from the pinned `v0.4.4` release asset |
| Vendored MathJax, Cytoscape, and QuickJS sources | 4,741,701 for the three files over 1 MiB | Keep vendored; enforce license, version, and checksum provenance |
| `browser_data.rs` | 1,827,956 | Split for maintainability, not repository-size savings |
| iOS icon SVG | 1,000,242 | Keep as a reviewed source asset |

## Enforcement

`.github/large-files-allowlist.txt` is a reviewed exception list with a byte
budget per path. `scripts/check-large-files.sh` rejects any new tracked blob
over 1,000,000 bytes and rejects growth beyond an existing budget. Use
`scripts/check-large-files.sh --staged` before committing.

External Android inputs are pinned in
`apps/android/actro/android/binary-assets.lock.properties`; the JDK downloader
verifies archives before staging and atomically replacing generated outputs.
Large vendored sources remain pinned in `.github/vendored-assets.sha256`.
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
2. **Vendored sources.** Keep QuickJS as source because it is compiled and
   locally patched. Keep the MathJax and Cytoscape bundles while byte-stable,
   offline embedding is a requirement. Package-manager generation is an
   optional build-system change, not an automatic repository improvement.

## History is separate

The local `.git` directory was 278 MiB at the audit point. Reachable history
still contains deleted build products, including several 45-90 MiB
`CanvasTerminal.xcframework` archives, profiling output, and Rust `target`
artifacts. Removing files from the current tree does not remove those blobs
from existing clones. If clone size remains a goal after changing current
distribution, perform a separately coordinated `git filter-repo` rewrite,
publish a backup ref/bundle, and require fresh clones.
