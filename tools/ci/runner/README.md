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
The build-pool manifest retains its existing deployment configuration. The light
pool uses the environment-neutral template and private renderer described below.

The ConfigMap supplies the following keys:

| Purpose           | Keys                                                                                                                                            |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Compiler          | `RUSTFLAGS`, `RUSTC_WRAPPER`, `CARGO_INCREMENTAL`                                                                                               |
| Proxy             | `HTTP_PROXY`, `HTTPS_PROXY`, `http_proxy`, `https_proxy`, `NO_PROXY`, `no_proxy`                                                                |
| Cargo downloads   | `CARGO_NET_RETRY`, `CARGO_HTTP_TIMEOUT`                                                                                                         |
| Compilation cache | `SCCACHE_CACHE_SIZE`, `SCCACHE_CACHE_DIR`, `SCCACHE_ENDPOINT`, `SCCACHE_BUCKET`, `SCCACHE_REGION`, `SCCACHE_S3_USE_SSL`, `SCCACHE_IDLE_TIMEOUT` |
| Actions cache     | `ACTIONS_RESULTS_URL`                                                                                                                           |

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

| Tool                                      | Version       |
| ----------------------------------------- | ------------- |
| wasm-bindgen and wasm-bindgen-test-runner | `0.2.126`     |
| wasm-pack                                 | `0.15.0`      |
| wasm-component-ld                         | `0.5.26`      |
| wasm-tools                                | `1.253.0`     |
| Binaryen                                  | `version_117` |
| sccache                                   | `0.18.0`      |
| zstd                                      | `1.5.7`       |

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

Both Dockerfiles invoke `check-runner-bootstrap.sh` for the same tool, cache-worker,
Clippy, mold, and hosted-toolcache marker checks. The incremental build preserves
its immutable base rather than repeating the full build's OS installation.
Do not overwrite a deployed tag. Pin both runner and initContainer images by digest
and verify that the fleet can pull the image before changing the manifest.

## Cache and build reuse

Cargo archives and the registry index share node-local storage. Each Pod extracts
sources into its own `registry/src` volume to avoid concurrent unpack-marker races.
The initContainer prepares the shared directories; hostPath ownership is not
repaired recursively by `fsGroup`. If a privileged job leaves root-owned cache or
index entries, drain assigned jobs before repairing only those two directories on
the host:

```bash
for directory in "$RUNNER_REGISTRY_HOST_PATH/cache" "$RUNNER_REGISTRY_HOST_PATH/index"; do
  sudo find "$directory" -xdev -exec chown -h 1001:1001 {} +
done
```

Do not remove the shared `registry/src` mount point while Pods are running.

The image seeds Gradle 9.2.1 under the canonical `services.gradle.org` wrapper
URL's cache key and creates its `.zip.ok` marker. Both Kotlin wrappers use that URL
and keep the canonical archive SHA-256. `prepare-gradle-cache.py` can reuse the
trusted image's existing same-version distribution when its URL slot or marker
needs preparation; it never downloads an archive. The CI preparation step also
supports the previous image's missing marker. Normal wrapper downloads still
verify `distributionSha256Sum`. Additional Gradle versions used by independent
examples or generated projects have their own distribution caches.

Rust compilation uses sccache. Actions file caches use an externally configured cache service selected
by the inherited `ACTIONS_RESULTS_URL`. The worker normally writes GitHub's
endpoint into that environment key. Renaming its UTF-16LE literal to
`ACTIONS_RESULTS_ORL` moves that write to a separate key; it does not rename the
ConfigMap key or the cache client's input. Both builds assert that
the patch applied exactly once; revalidate it when upgrading the runner agent.

Generated Web runtime artifacts are a shared dependency. A producer failure intentionally
blocks consumers that require those bytes. Output-cache misses use a separate
Rust dependency/build cache; audit policy changes do not invalidate WASM outputs.
Future Rust toolchain override files remain cache inputs.

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

## Registry access and ARC compatibility

A registry may permit anonymous pulls or require authentication. For authenticated
registries, provision a namespace-scoped pull Secret from an operator-managed
Docker config without putting credentials in commands or this repository:

```bash
kubectl -n "$RUNNER_NAMESPACE" create secret generic runner-registry \
  --type=kubernetes.io/dockerconfigjson \
  --from-file=.dockerconfigjson="$RUNNER_REGISTRY_AUTH_FILE"
```

Add `imagePullSecrets: [{name: runner-registry}]` to `spec.template.spec` when
required. Keep credentials external; existing node authentication is not a
portable deployment prerequisite.

Before applying a manifest or upgrading ARC, check the installed controller chart:

```bash
helm -n "$ARC_CONTROLLER_NAMESPACE" list --filter "^${ARC_CONTROLLER_RELEASE}$"
```

Keep `app.kubernetes.io/version` aligned with the installed ARC controller's
compatible major/minor version. Incompatible scale-set versions can be removed by
the controller. After a coordinated controller upgrade, apply the reviewed manifest
with the matching label and verify the listener and runners register again.

## Capacity and scheduling

Set min/max counts, requests, limits, node affinity, and storage for the destination
pool before deployment. The example reserves at least 20 CPU/40 GiB for ten runners
and 32 CPU/64 GiB at sixteen runners, in addition to other workloads. CPU limits
allow bursts; they do not reserve node capacity. Check allocatable resources,
existing requests, Pod Pending/OOM events, and observed CPU throttling before
raising concurrency. An overcommit ratio alone does not establish throttling.

### Light CI pool

`autoscaling-runner-set-light.yaml.tmpl` defines `actrium-linux-light` alongside
the build pool. Deployment-specific values are supplied privately at render time;
do not apply the template directly or commit a rendered manifest. The pool reuses
the operator-selected GitHub App Secret and controller RBAC. Its ConfigMap contains
only the required proxy settings. It imports no compilation-cache credentials or
node-local Cargo storage, and starts the runner agent without the cache entrypoint.

Supply these seven string settings in an operator-managed JSON file outside the
checkout, or through environment variables when `--config` is omitted:

| Setting                       | Purpose                                           |
| ----------------------------- | ------------------------------------------------- |
| `RUNNER_IMAGE`                | Published image reference pinned with `@sha256:`  |
| `RUNNER_NAMESPACE`            | Existing runner namespace                         |
| `RUNNER_GITHUB_ORGANIZATION`  | GitHub organization label                         |
| `RUNNER_GITHUB_CONFIG_URL`    | GitHub organization configuration URL             |
| `RUNNER_GITHUB_CONFIG_SECRET` | Existing GitHub App Secret name                   |
| `RUNNER_LIGHT_ENV_CONFIGMAP`  | Proxy-only ConfigMap name                         |
| `ARC_CONTROLLER_VERSION`      | Installed compatible ARC controller chart version |

```bash
python3 tools/ci/runner/render-light-runner.py \
  --config "$RUNNER_LIGHT_CONFIG_FILE" \
  --output "$RUNNER_LIGHT_MANIFEST"

kubectl --context "$RUNNER_CONTEXT" apply --dry-run=server \
  -f "$RUNNER_LIGHT_MANIFEST"
```

The renderer requires every setting, rejects mutable image tags and unexpected
keys, writes atomically with owner-only permissions, and prints no deployment
values. Protect the external settings file with the same permissions. Compare the
rendered specification with the intended deployment before applying it. Converting
the source to this template does not require changing an existing scale set or
restarting its Pods.

Select exactly four eligible nodes with the `actrium-linux-light=true` node
label. Storage nodes may be selected only for an explicitly authorized light-only
validation, with storage health monitored during the check. The pool has four warm
runners and a maximum of sixteen. Hard hostname spreading with `maxSkew: 1` and
`minDomains: 4` bounds each node to four runners; if a node becomes unavailable,
the scheduler leaves excess runners Pending rather than concentrating them on
the remaining nodes. Do not raise `maxRunners` above sixteen without revisiting
this bound. Existing Pods are not redistributed by topology spreading.

Each runner requests and limits 2 CPUs and 4 GiB of memory, with a 10 GiB
workspace and 12 GiB ephemeral-storage limit. At four runners, a node commits
8 CPUs and 16 GiB of memory to this pool. These limits do not isolate disk I/O.

Use `runs-on: actrium-linux-light` for policy checks, formatting, shell-script
checks, repository hygiene, bounded formal checks, version metadata, and release
tag validation. Python codegen, TypeScript workload authoring, and Keel's
VitePress docs build have also been validated within the 2 CPU / 4 GiB limit.
Native builds, native dependency compilation, and integration suites stay on
the build pool.
The light set intentionally omits `self-hosted`, `Linux`, and `X64` aliases, so
existing jobs selecting that generic label combination cannot run on it.

After checkout, jobs using dependency caches or artifacts must invoke
`./.github/actions/setup-light-runner` before their cache or artifact actions.
The shared image's runner patch exposes GitHub's original results endpoint as
`ACTIONS_RESULTS_ORL`; the composite action restores `ACTIONS_RESULTS_URL` for
the light pool without injecting a deployment-specific cache endpoint. Keep
this setup local to light jobs; the ordinary build pool retains its cache setup.
Keel Docs explicitly selects Node.js 24 instead of relying on hosted-runner tools.

Validate a new deployment by temporarily setting `minRunners: 16`, checking four
registered runners per selected node and none on other nodes, then restoring
`minRunners: 4`. Complete a routed CI job and inspect control-plane latency and
node pressure before considering the migration verified.

## Deploy, verify, and roll back

1. Confirm that the externally managed ConfigMap and Secrets exist and contain the
   required keys. Avoid commands that display credential values.
2. Confirm that no jobs are assigned before rolling the runner template.
3. Run a server-side dry run, then apply the reviewed fleet manifest using the
   operator's configured cluster context.
4. Check that new runners become ready, register, and complete a CI job. Verify
   successful Rust and Actions cache restores/saves.

For a complete fleet change, capture and restore the full prior configuration:
capacity, scheduling, resource requests/limits, storage, image references, and the
corresponding external environment resources. This manifest includes fleet changes
relative to the repository baseline; a full rollback must not restore only envFrom.

For a configuration-only update to an already deployed fleet, preserve its existing
capacity, scheduling, resources, storage, and image. Snapshot the ConfigMap and
Secret privately, change only their environment references/values, and coordinate
Pod replacement with assigned jobs drained. Do not commit populated backups.
