# Actrium Linux CI Runner

This directory contains the runner image build sources and the declarative
configuration for the active `actrium-linux` fleet on the KubeSphere
kubekey cluster. `autoscaling-runner-set.yaml` records the deployed
AutoscalingRunnerSet and the controller's namespace Role/RoleBinding.

## Deployed configuration

| Setting | Value |
|---|---|
| Namespace | `arc-runners-actrium` |
| Scale set | `actrium-linux` |
| ARC controller/chart version | `0.14.2` |
| Minimum / maximum runners | 10 / 10 |
| Runner image | `harbor.zlibs.com/platform/actrium-runner:20261013-cache` |
| Init container image | `harbor.zlibs.com/platform/actrium-runner:20260901` |
| Pod requests | CPU 2, memory 4Gi |
| Pod limits | CPU 12, memory 12Gi |
| Scheduling | Nodes labeled `arc-runner=true` |
| Workspace | Per-pod `emptyDir`, 100Gi limit |
| Cargo registry storage | Node-local `/var/openebs/cargo-registry` hostPath |
| Container startup | Image entrypoint `runner-entrypoint.sh`; the manifest carries no command/args |

The fleet handles `runs-on: [self-hosted, Linux, X64]` jobs. ARC injects
`ACTIONS_RUNNER_INPUT_JITCONFIG` and reaps each runner Pod after its job.
The YAML is authoritative for image tags, resources, environment, and the
init container.

## What the image provides

The image is the single source of the per-Pod bootstrap, so a runner Pod
starts with every tool already in place:

| Provided by the image | Why it used to be a per-Pod download |
|---|---|
| `wasm-opt` (Binaryen `version_117`) | `wasm-pack` resolves it through a code path (ureq) that ignores `HTTP(S)_PROXY`, so the job-time download was unreliable on proxied egress |
| `sccache` `0.18.0` | the toolchain-era image shipped `0.10.0`, which cannot start its daemon against the fleet's MinIO 2025-07 S3 endpoint; the startup script installed `0.18.0` on every Pod |
| Python `3.11.16` toolcache bin on `PATH` | the system `python3` is 3.10 while CI scripts need 3.11+ (stdlib `tomllib`) |
| Actions cache worker patch | see "Actions cache integration" |

`entrypoint.sh` therefore only performs runtime work:

1. pre-start the sccache daemon while the Pod is still idle. `RUSTC_WRAPPER`
   would start it lazily, and a cold start inside a compile burst exceeded the
   client's server-startup timeout under fleet load. A failure is tolerated
   (`|| true`): a broken cache must never keep a job from running;
2. `exec /home/runner/actions-runner/run.sh`. `exec` keeps the agent as PID 1,
   so Kubernetes `SIGTERM`/`SIGINT` reach it for a graceful shutdown
   (`RUNNER_MANUALLY_TRAP_SIG=1`). An explicit command - for example the
   standalone `container:` workflow mode - is forwarded instead.

The startup path performs no network downloads.

## Cache layers

| Layer | Backend | Configuration |
|---|---|---|
| Cargo crate downloads | Node-local hostPath | `/home/runner/.cargo/registry` |
| Rust compilation | sccache with in-cluster MinIO | `SCCACHE_ENDPOINT=sccache-minio.arc-sccache.svc:9000`, bucket `sccache` |
| Actions file caches | gha-cache-server with MinIO storage | `ACTIONS_RESULTS_URL=http://gha-cache-server.arc-sccache.svc:3000/`, bucket `gha-cache` |

### Cargo registry layout

The fleet shares one node-local Cargo registry between every Pod on that node
(`/var/openebs/cargo-registry` hostPath). That is what makes crate downloads
free, but only part of the registry may actually be shared:

| Registry path | Backing | Why |
|---|---|---|
| `registry/cache` | node hostPath | Holds the `.crate` archives; cargo downloads to a temporary file and renames it into place, so concurrent Pods are safe and no Pod re-downloads |
| `registry/index` | node hostPath | Sparse index cache, protected by cargo's own advisory locks |
| `registry/src` | per-Pod `emptyDir` | Extraction target. Cargo unpacks each `.crate` here and marks a finished unpack with a `.cargo-ok` file; a shared directory lets two Pods extracting the same crate collide on that marker (`failed to unpack ... File exists (os error 17)`), which fails the losing job |

Only the extraction directory is therefore Pod-local. Each Pod re-extracts
crates from the shared archive cache - CPU only, never network - and the volume
carries a 20 GiB guard (measured extraction for this workspace is ~1.3 GiB).
This matters most when many Rust jobs run concurrently on one node, which is
exactly when the shared-directory race used to fire.

The stale shared `registry/src` left on each node by the previous layout is
inert (it is shadowed by the per-Pod mount). It can be reclaimed node-side with
`rm -rf /var/openebs/cargo-registry/src` once no runner uses the old layout.

### Actions cache integration

The fleet's `gha-cache-server` (`falcondev-oss/github-actions-cache-server`)
implements the v1 Actions cache API. `actions/cache` addresses the v2 cache
service through `ACTIONS_RESULTS_URL` and the v1 service through
`ACTIONS_CACHE_URL`, falling back to `ACTIONS_RESULTS_URL`. The cache-enabled
image therefore ships a `Runner.Worker.dll` whose `ACTIONS_RESULTS_URL`
literal is renamed to `ACTIONS_RESULTS_ORL` by `patch-runner-dll.py`: the
worker no longer binds job cache traffic to the hosted v2 results endpoint, so
cache requests resolve to the in-cluster v1 server.

The patch is one in-place UTF-16LE substitution of equal byte length, and it
asserts that exactly one occurrence exists. A runner version bump that changes
the string therefore fails the image build loudly instead of shipping an
unpatched runner. The runtime effect is what the cache restore/save check in
"Deploy and verify" confirms.

Reproducing the published patch byte-for-byte:

| DLL | SHA-256 |
|---|---|
| `20260901` (plain) | `0c7be2904004e4e43a0f00c508c6904ef2b73eebd89c0971645efca667c81d88` |
| `20260901-cache` (patched) | `1b99af8831f187de16c597d180b3596d63c530c106cf02e90fd8f2c583d6a659` |

Applying `patch-runner-dll.py` to the plain DLL reproduces the patched DLL
exactly, which is how the integration was verified.

## Image build

`Dockerfile` is the canonical, self-contained definition: it starts from
`ubuntu:22.04` and installs the whole toolchain. Building it requires Docker
Hub access and re-resolves every apt/tool download.

Production updates on the internal build host use `Dockerfile.incremental`,
which layers the bootstrap changes on top of a published image pinned by
digest. That keeps every inherited layer byte-identical, which matters for two
reasons: the internal network cannot reach Docker Hub, and a full rebuild
would pull newer apt packages than the running fleet was built with (an
unintended toolchain upgrade). Keep both files in sync - they must produce the
same additions.

```bash
# Canonical full build (needs Docker Hub; validates the whole toolchain).
docker build -t actrium-runner:dev \
  -f tools/ci/runner/Dockerfile tools/ci/runner/

# Incremental production rebuild (internal network; github.com via corp proxy).
docker build \
  -f tools/ci/runner/Dockerfile.incremental \
  --build-arg GITHUB_PROXY=http://calm.util.zlibs.com:8128 \
  -t harbor.zlibs.com/platform/actrium-runner:<YYYYMMDD>-cache \
  tools/ci/runner/
```

Pinned artifacts, verified by both Dockerfiles:

| Artifact | Version | SHA-256 |
|---|---|---|
| Binaryen (`wasm-opt`) | `version_117` | `3dc677006555b355ea2da5e82602065a161d5e83eaefd3f759afa00b96e83212` |
| sccache | `0.18.0` | `45f1447fbe231e3037bde351ef70677dd212216c8d62ae7ca409fecc4d6acc89` |

Base image pinned by digest for the incremental build:

| Tag | Digest |
|---|---|
| `harbor.zlibs.com/platform/actrium-runner:20260901` | `sha256:5d90ceb14253b8a08d4ffea6ddce7efe6e2e80a8179d99f8ef145ef48fdd3d3c` |

The incremental build starts from the plain tag (not the cache tag) and
applies the cache patch itself, so the cache-enabled image is reproducible
from published inputs: a clean `--no-cache` rebuild produces identical file
contents, identical config metadata (`User`, `Entrypoint`, `Cmd`, `Env`) and
the same 21 inherited layers. The image *digest* is not bit-identical across
rebuilds, because files created by `RUN` steps carry the build timestamp.
Verified: two clean rebuilds differ only in the layers above the inherited
ones, and `--build-arg SOURCE_DATE_EPOCH=<epoch>` normalises the image config
timestamp but not those file mtimes. Pin the published digest when an exact
artifact matters. The build fails if `sccache` is not exactly `0.18.0`, if `python3`
cannot `import tomllib`, if `wasm-opt` is missing, or if the cache patch did
not apply.

### Publishing

Push a new tag and record its digest; never overwrite a tag the fleet runs:

```bash
docker tag <local-image> harbor.zlibs.com/platform/actrium-runner:<tag>
docker push harbor.zlibs.com/platform/actrium-runner:<tag>
docker image inspect --format '{{index .RepoDigests 0}}' \
  harbor.zlibs.com/platform/actrium-runner:<tag>
```

Use the registry credentials already configured on the build host. Do not
commit them, and do not echo them into build logs.

## Prerequisites

1. Install the ARC controller in `arc-systems` at the version used by the
   manifest:
   ```bash
   helm upgrade --install arc \
     oci://ghcr.io/actions/actions-runner-controller-charts/gha-runner-scale-set-controller \
     --version 0.14.2 -n arc-systems --create-namespace
   ```
2. Create namespace `arc-runners-actrium`. Create
   `actrium-arc-github-app` there with `github_app_id`,
   `github_app_installation_id`, and `github_app_private_key`. Use
   `--from-file=github_app_private_key=...` for the private key.
3. Provision MinIO in `arc-sccache` with buckets `sccache` and `gha-cache`.
   The runner namespace needs Secret `sccache-s3` with
   `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` for sccache. Deploy
   `gha-cache-server` with its storage credentials and make the service
   reachable at the `ACTIONS_RESULTS_URL` above. These backing services
   and Secret values are managed separately from this runner manifest.
4. Ensure nodes can pull both pinned images from `harbor.zlibs.com`, reach the
   corporate proxy and cache services for job-time traffic, and have space for
   the node-local registry cache and per-pod workspaces. Label the approved
   nodes:
   ```bash
   kubectl label node <node-name> arc-runner=true
   ```

The bundled `arc-manager` Role and RoleBinding authorize
`arc-systems/arc-gha-rs-controller` in the runner namespace. The controller
chart alone does not provide these namespace permissions. If its service
account name changes, update the RoleBinding subject accordingly.

## Deploy and verify

Use the KubeSphere cluster context. Check active jobs before applying a
reviewed change: updating the runner template rolls the fleet and can
interrupt running jobs.

```bash
kubectl --context=kubesphere -n arc-runners-actrium get ephemeralrunners
kubectl --context=kubesphere apply --dry-run=server \
  -f tools/ci/runner/autoscaling-runner-set.yaml
kubectl --context=kubesphere apply \
  -f tools/ci/runner/autoscaling-runner-set.yaml
kubectl --context=kubesphere -n arc-runners-actrium \
  get autoscalingrunnerset,ephemeralrunner,pods
```

Verify, in this order:

1. **Image and startup** - new Pods run the pinned tag and the container uses
   the image entrypoint:
   ```bash
   kubectl --context=kubesphere -n arc-runners-actrium get pods \
     -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.spec.containers[0].image}{"\n"}{end}'
   kubectl --context=kubesphere -n arc-runners-actrium logs <pod> -c runner | head -20
   ```
   The startup log must not contain a Binaryen or `sccache-v0.18.0` download.
2. **Registration** - the Pod registers under `actrium-linux` and a CI job
   completes end to end.
3. **Rust cache** - sccache reports a running server in the job log and the
   S3-backed counters move.
4. **Actions cache** - a job using `actions/cache` logs a successful restore
   and/or save.

Then compare Pod startup cost with the same commit and a similar cache state
against the previous image, and report the phases separately: image pull, Pod
initialization, and job execution. Do not attribute cache-speed effects that
were already delivered by other changes to this image update.

## Rollback

Rolling back is a manifest revert; the previous image tag is never deleted, so
the fleet can be returned to `20260901-cache` with its startup script.

**Capture the rollback manifest before switching.** Upstream `main` carries a
stale template (`ghcr.io/actrium/runner:latest`, `minRunners: 0`, no startup
script) that does not describe what the fleet actually runs, so it is *not* a
valid rollback target - and the pre-change revision is not reachable on this
branch either. Snapshot the deployed revision first:

```bash
kubectl --context=kubesphere -n arc-runners-actrium get autoscalingrunnerset \
  actrium-linux -o yaml > /tmp/runner-set-previous.yaml

# Strip runtime metadata so the snapshot can be re-applied verbatim.
python3 -c "
import yaml
doc = yaml.safe_load(open('/tmp/runner-set-previous.yaml'))
for key in ('resourceVersion', 'uid', 'creationTimestamp', 'generation', 'managedFields'):
    doc['metadata'].pop(key, None)
doc['metadata'].get('annotations', {}).pop('kubectl.kubernetes.io/last-applied-configuration', None)
doc.pop('status', None)
yaml.safe_dump(doc, open('/tmp/runner-set-rollback.yaml', 'w'), sort_keys=False)
"
```

Then roll back:

```bash
kubectl --context=kubesphere apply --dry-run=server -f /tmp/runner-set-rollback.yaml
kubectl --context=kubesphere apply -f /tmp/runner-set-rollback.yaml
kubectl --context=kubesphere -n arc-runners-actrium get pods \
  -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.spec.containers[0].image}{"\n"}{end}'
```

Roll back when registration, caching, or job execution regresses. A snapshot of
the pre-change revision (image `20260901-cache` plus the 39-line startup
script) is recorded with the operational notes for this change, so the fleet
can be restored even if the live snapshot was missed.

## Image maintenance

- Bump the pinned tool versions and their checksums together; the build
  verifies checksums, so a mismatch fails the build rather than the fleet.
- When the runner agent version changes, re-run the cache patch verification:
  `patch-runner-dll.py` fails if the `ACTIONS_RESULTS_URL` literal is no longer
  present exactly once.
- Keep `Dockerfile` and `Dockerfile.incremental` in sync; a change to one that
  is not mirrored in the other silently splits the canonical and production
  image contents.
- The init container image is pinned separately and only creates the registry
  cache directory, so it does not need to track the runner image.

## Operational notes

- Jobs run as uid 1001 with passwordless sudo. Each job gets a separate
  Pod and workspace; the node-local Cargo registry cache is shared.
- The init container creates the registry cache directory with uid/gid
  1001 ownership before the runner starts.
- Crates.io and Rust toolchain downloads use the corporate proxy. The runner
  keeps this route rather than probing a mirror (mass cold downloads through a
  mirror were rate-limited: USTC 429, seen 2026-09-29).
- Keep sccache pre-started so the first parallel compiler calls connect
  to an existing daemon; that is the one piece of startup the entrypoint
  still performs.