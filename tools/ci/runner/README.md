# Actrium Linux CI Runner

This directory contains the Linux runner image sources and ARC fleet manifest.
The image prepares pinned tools before jobs start. The fleet manifest provides
per-job workspaces and isolates Cargo source extraction between Pods.

## External environment configuration

Provision these resources in the runner namespace before applying the manifest:

- ConfigMap `runner-env`: compiler settings, proxy configuration, Cargo download
  settings, and cache configuration.
- Secret `sccache-s3`: only `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
- The GitHub App Secret referenced by `githubConfigSecret` in the manifest.

The runner imports the first two resources through `envFrom`. Their values are
managed outside this repository; do not commit the populated ConfigMap or Secret.
The namespace, image, scheduling, and storage fields remain in the fleet manifest.

The ConfigMap supplies the following keys:

| Purpose | Keys |
|---|---|
| Compiler | `RUSTFLAGS`, `RUSTC_WRAPPER`, `CARGO_INCREMENTAL` |
| Proxy | `HTTP_PROXY`, `HTTPS_PROXY`, `http_proxy`, `https_proxy`, `NO_PROXY`, `no_proxy` |
| Cargo downloads | `CARGO_NET_RETRY`, `CARGO_HTTP_TIMEOUT` |
| Compilation cache | `SCCACHE_CACHE_SIZE`, `SCCACHE_CACHE_DIR`, `SCCACHE_ENDPOINT`, `SCCACHE_BUCKET`, `SCCACHE_REGION`, `SCCACHE_S3_USE_SSL`, `SCCACHE_IDLE_TIMEOUT` |
| Actions cache | `ACTIONS_RESULTS_URL` |

`envFrom` imports every key in a referenced resource. Keep the Secret restricted
to the two required cache credential keys, and do not duplicate those keys in the
ConfigMap. A missing required resource prevents the runner container from starting.
Environment updates affect new Pods; existing Pods require replacement.

External storage removes configuration values from the manifest. Credentials
injected into the runner container remain accessible to code executing there.
Use only trusted build code on that fleet, or keep storage credentials behind a
separate cache service when stronger credential isolation is required.

## Image contents

The image includes the GitHub Actions agent, Rust and WebAssembly tools, hosted
Python/Node/Java toolcaches, sccache, Binaryen, and zstd. Both image build paths
verify pinned versions and artifact checksums.

| Tool | Version |
|---|---|
| wasm-bindgen and wasm-bindgen-test-runner | `0.2.126` |
| wasm-pack | `0.15.0` |
| wasm-component-ld | `0.5.26` |
| wasm-tools | `1.253.0` |
| Binaryen | `version_117` |
| sccache | `0.18.0` |
| zstd | `1.5.7` |

The installer pins must match the workflows and the web workspace lockfile.
The zstd installer also provides `zstdmt`, `unzstd`, and `zstdcat` for Actions Cache.

`entrypoint.sh` pre-starts sccache and then executes the runner agent as PID 1.
An explicit container command is forwarded. Startup performs no tool downloads.

## Build and publish

`Dockerfile` builds the complete toolchain from the base operating system.
`Dockerfile.incremental` requires a previously published plain runner image,
pinned by digest, through `RUNNER_BASE_IMAGE`. It preserves the inherited
layers and adds the same tool, entrypoint, and cache-worker changes.

Supply image references and any network configuration from the build environment:

```bash
docker build -t actrium-runner:dev \
  -f tools/ci/runner/Dockerfile tools/ci/runner/

docker buildx build \
  -f tools/ci/runner/Dockerfile.incremental \
  --build-arg RUNNER_BASE_IMAGE \
  --build-arg HTTP_PROXY --build-arg HTTPS_PROXY \
  --provenance=false --load \
  -t "$RUNNER_IMAGE" tools/ci/runner/

docker push "$RUNNER_IMAGE"
```

Set `RUNNER_BASE_IMAGE` to an immutable image reference before the incremental
build. `GITHUB_PROXY`, when used, is a release-URL prefix rather than an HTTP proxy.
Use configured registry authentication and never print credentials in build logs.

Keep both Dockerfiles synchronized. Do not overwrite a deployed tag; verify a new
image can be pulled from the fleet before changing the manifest.

## Cache and build reuse

Cargo archives and the registry index share node-local storage. Each Pod extracts
sources into its own `registry/src` volume to avoid concurrent unpack-marker races.

Rust compilation uses sccache. Actions file caches use a v2 cache service selected
by `ACTIONS_RESULTS_URL`. The worker patch renames the single UTF-16LE literal to
`ACTIONS_RESULTS_ORL`, preserving the configured endpoint. Both builds assert that
the patch applied exactly once; revalidate it when upgrading the runner agent.

CI Gate generates Web runtime artifacts once. Rust, MSRV, both test shards,
Package Runtime, and Kotlin Codegen consume `cli-web-runtime`; Web consumes the
full `sw-host-runtime` output. Only the rest shard saves the shared dependency
cache. Fleet jobs preserve preinstalled tools; hosted binary cache keys include
the required versions.

The rest shard compiles one workspace feature graph and reuses nextest metadata
for inventory and execution. Its RFC-0507 acceptance test must be selected exactly
once and keeps its zero-retry override. Doctests remain a separate gate.

Windows verification uploads the DLL exercised by its harness. Portable packaging
reuses that artifact and verifies that the DLL inside the zip has the same SHA-256.

## Deploy, verify, and roll back

1. Confirm that the externally managed ConfigMap and Secrets exist and contain the
   required keys. Avoid commands that display credential values.
2. Confirm that no jobs are assigned before rolling the runner template.
3. Run a server-side dry run, then apply the reviewed fleet manifest using the
   operator's configured cluster context.
4. Check that new runners become ready, register, and complete a CI job. Verify
   successful Rust and Actions cache restores/saves.

Capture the existing fleet configuration before a rollout. Restore that snapshot
for rollback, including the corresponding externally managed environment values.
Do not change capacity, scheduling, or storage as part of a configuration-only
rollout. Coordinate Pod replacement after updating environment resources.
