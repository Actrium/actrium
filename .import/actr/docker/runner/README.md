# Actrium Runner Image (Linux)

Self-hosted GitHub Actions runner image for the Actrium Linux CI fleet.
Bundles the GitHub Actions runner agent, the Rust toolchain, protoc, and
the wasm tools the CI workflows need, so a job starts without spending
minutes on `apt-get install` / `rustup` every run.

## When to use this image

| Mode | How | Notes |
|---|---|---|
| ARC (gha-runner-scale-set) | ARC starts a Pod from this image per queued job; the agent self-registers via injected JIT config | Recommended for k8s-hosted runners; see "ARC mode" below |
| Workflow `container:` | `jobs.<id>.container: ghcr.io/actrium/runner:<tag>` | Lighter weight, no operator needed |
| Standalone runner agent | Run `./config.sh` inside a container from this image | Closest to a directly installed agent on a host |

## Build

Build context is this directory (`docker/runner/`):

```bash
docker build -t actrium-runner:dev -f docker/runner/Dockerfile docker/runner/
```

All versions are pinned with `ARG`; override at build time to upgrade
(e.g. `--build-arg RUNNER_VERSION=2.337.0`). Optional `APT_MIRROR` /
`GITHUB_PROXY` / `RUSTUP_DIST_SERVER` build args redirect downloads to
mirrors where the defaults are slow — see the Dockerfile header.

## Pinned versions

| Component | Version | Notes |
|---|---|---|
| Ubuntu | 22.04 | `ubuntu:22.04` base |
| GitHub Actions runner agent | 2.336.0 | actions/runner release |
| Rust toolchain | stable + nightly | nightly for `ci-coverage` `coverage-web-wasm` |
| Rust components | clippy, rustfmt, rust-src, llvm-tools-preview | |
| Rust targets | wasm32-unknown-unknown, wasm32-wasip2 | Android targets NOT included — `kotlin-echo-app-e2e` needs JDK/Android SDK/KVM and stays on GitHub-hosted runners |
| protoc | 29.3 | matches `vars.PROTOC_VERSION` |
| wasm-pack / wasm-component-ld / wasm-tools | 0.15.0 / 0.5.26 / 1.253.0 | sha256-pinned; sha values match `ci-gate.yml`, so a tampered upstream release fails the build loudly |

`rust_msrv` jobs still pin `dtolnay/rust-toolchain@1.95.0` (historical
toolchain, not preinstalled). Installed apt packages mirror the
`apt-get install` steps across `.github/workflows/ci-*.yml` — see the
`base` stage of the Dockerfile for the authoritative list.

## User

Jobs run as `runner` (uid 1001) with passwordless sudo, matching the
GitHub Actions convention. Trade-off: any job code (e.g. from fork PRs)
runs as passwordless root inside the pod. One-job-per-pod under ARC
contains but does not eliminate the blast radius.

## ARC mode (gha-runner-scale-set)

`autoscaling-runner-set.yaml` deploys this image as an
`AutoscalingRunnerSet` (the current, maintained ARC generation; the
legacy `actions.summerwind.dev` RunnerDeployment is deprecated). Each
queued job gets its own ephemeral Pod: the controller injects the JIT
runner config as `ACTIONS_RUNNER_INPUT_JITCONFIG`, the stock agent
`run.sh` consumes it, and the Pod is reaped when the job ends.

### Prerequisites (out of band, need cluster admin / org admin)

1. **ARC controller** (once per cluster):
   ```bash
   helm install arc \
     oci://ghcr.io/actions/actions-runner-controller-charts/gha-runner-scale-set-controller \
     -n arc-systems --create-namespace
   ```
2. **GitHub App** (preferred) or PAT with organization runner
   permissions. The App's ID, Installation ID, and private key go into a
   secret (below).
3. **Image pushed** to GHCR (below).

### Create the GitHub App secret

In the same namespace as the scale set:

```bash
kubectl create namespace arc-runners
kubectl -n arc-runners create secret generic actrium-arc-github-app \
  --from-literal=github_app_id=<APP_ID> \
  --from-literal=github_app_installation_id=<INSTALLATION_ID> \
  --from-file=github_app_private_key=app-private-key.pem
```

Use `--from-file` for the private key (not `--from-literal="$(cat ...)"`)
so the PEM survives byte-for-byte.

### Build and push the image

```bash
docker build -t ghcr.io/actrium/runner:latest -f docker/runner/Dockerfile docker/runner/
docker push ghcr.io/actrium/runner:latest
```

**Visibility matters:** a GHCR package linked to a private repo defaults
to private, and pods fail with `ImagePullBackOff` unless you make the
package public or create a pull secret and uncomment `imagePullSecrets`
in `autoscaling-runner-set.yaml`:

```bash
kubectl -n arc-runners create secret docker-registry ghcr-pull \
  --docker-server=ghcr.io --docker-username=<user> \
  --docker-password=<read:packages PAT>
```

### Apply the manifest

```bash
kubectl apply -f docker/runner/autoscaling-runner-set.yaml
kubectl -n arc-runners get autoscalingrunnerset,ephemeralrunner -w
```

The scale set registers runners with the scale-set name (`actrium-linux`)
plus the labels in `runnerScaleSetLabels` (`self-hosted`, `Linux`,
`X64`), matching the `runs-on: [self-hosted, Linux, X64]` selectors once
the routing change (PR #557) lands. Until then the runners simply see no
jobs — expected, not a misconfiguration.

## Windows runners

Not covered. Windows containers add version-coupling and size overhead
without proportional benefit; Windows runners stay on their existing
directly-installed host setup.

## Updating this image

1. Bump the relevant `ARG` default in `Dockerfile`.
2. If a CI workflow gains an `apt-get install` dependency, add it to the
   `base` stage in the same PR.
3. If a wasm tool version changes in `ci-gate.yml`, update both the URL
   and the sha256 in the `toolchain` stage.
4. Rebuild and push.

## Verification

The final stage sanity-checks the toolchain (`rustc`, `cargo`,
`rustfmt`, `clippy`, `protoc`, `nightly`, `llvm-tools`, wasm targets) at
build time, so a missing binary fails the build instead of a CI job.
