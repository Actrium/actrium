# GitHub-hosted CI

All workflows select GitHub-hosted runners. Linux build jobs use Ubuntu 24.04,
Apple ARM64 jobs use macOS 15, and existing hosted Windows and release matrices
retain their supported architecture labels.

## Runner policy

CI Gate validates every `.yml` and `.yaml` workflow with
`.github/scripts/check_workflow_runners.py`. The policy resolves static matrix
axes, `include`/`exclude` entries, and local reusable workflow calls. It rejects
custom labels, runner groups, retired images, and runner expressions or external
reusable workflows that cannot be verified statically.

```sh
python3 -m pip install -r .github/scripts/runner-policy-requirements.txt
python3 .github/scripts/check_workflow_runners_test.py
python3 .github/scripts/check_workflow_runners.py
```

Keep the explicit label allowlist aligned with GitHub's supported images.
The retired `macos-14` release targets use ARM64 `macos-15`; see the
[retirement announcement](https://github.com/actions/runner-images/issues/13518).

## WebAssembly CLI installer

`wasm-bindgen/install.sh` installs the Linux x86_64 musl binaries matching the
web workspace lockfile. It validates the pinned SHA-256 before installing either
`wasm-bindgen` or `wasm-bindgen-test-runner`. Keep `wasm-bindgen/versions.env`
aligned with `packages/actr-web/Cargo.lock`.

CI Gate and the Service Worker build use the same installer:

```sh
sh tools/ci/wasm-bindgen/install.sh packages/actr-web/Cargo.lock
```

The optional `RUNNER_DOWNLOAD_CACHE` stores checksum-verified download archives;
`GITHUB_PROXY` optionally prefixes release URLs. Neither setting is required on
GitHub-hosted runners. The download checks run locally without network requests
and require GNU coreutils `sha256sum` on `PATH`:

```sh
sh tools/ci/wasm-bindgen/test-download.sh
```

## Build storage and artifacts

Large Linux Rust jobs use `.github/actions/prepare-linux-build` to reclaim unused
SDK and container storage before restoring caches. Android jobs retain their SDK.
Rust and Gradle caches use GitHub's normal cache endpoints.
Rust cache paths follow each Cargo workspace's configured target directory.
The protoc setup steps use `PROTOC_VERSION` when configured and otherwise
install version `29.3`.
Every sccache consumer runs its setup action to export GitHub cache credentials,
even when the executable is already present. Kotlin uses canonical Maven and
Gradle plugin repositories by default. Build and packaging scripts use Cargo's
standard configuration and `CARGO_TARGET_DIR` directly.

CI Gate builds the Web runtime once. Rust, MSRV, test shards, Package Runtime,
and Kotlin Codegen consume `cli-web-runtime`; Web consumes `sw-host-runtime`.
The producer checks the cached `packages/actr-web/dist/sw` files before syncing
them into the CLI. A missing or empty required file triggers a rebuild.
Kotlin installs wasm-pack only when it must generate these artifacts itself.
