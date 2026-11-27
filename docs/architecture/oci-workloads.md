# OCI workloads

An `actr.oci/1` package carries one complete OCI Image Layout for
`linux/amd64` or `linux/arm64`. Hyper executes a native Guest that matches
the Linux host architecture in a rootless Syd OCI container.
The executable is a static PIE binary built with the `actr-framework`
`oci-guest` feature. Its entry point awaits
`actr_framework::guest::oci::run(workload)`.

Build the CLI and supervisor with the `oci-engine` feature on Linux x86_64
or aarch64.
Install the `actr-oci-supervisor` binary from the same build as `actr` beside
the configured runtime tools. The OCI workflow uses Syd OCI 3.60.0. Syd declares GPL-3.0-only
and is installed separately from Actr. The runtime also needs umoci 0.4.7 or
an explicitly validated replacement, and
bubblewrap with unprivileged user namespace support.

## Build and sign

Prepare an OCI Image Layout with a single image manifest and a complete set of
config and layer blobs. The image can carry OCI tar or gzip layer descriptors.
The pack manifest uses these fields:

```toml
[workload]
profile = "actr.oci/1"
entry = "image/index.json"
rpc-contract = "contracts/echo.proto"

[workload.oci]
digest = "sha256:IMAGE_MANIFEST_DIGEST"
platform = "linux/amd64"
ipc = "actr.oci-ipc/1"
args = ["/app/echo"]

[host]
profile = "actr.oci-host/1"
api = "1.0.0"
required-capabilities = ["actor.receive"]
```

Set `platform = "linux/arm64"` for an AArch64 Guest. The image config and
any index descriptor platform must agree with the signed platform.

The entry is a `workload` file. Declare `oci-layout` and each referenced blob
with role `image`, and set each blob's media type to its OCI descriptor type.
Declare the signed RPC Proto under role `contract`. The normal
`actr-package/2` actor and signature tables are also required. The packer
computes sizes, hashes, and the publisher key ID.

```sh
actr pkg pack --manifest-path manifest.toml --payload-root . --key publisher-key.json --output service.actr
actr pkg verify --package service.actr --pubkey publisher-public.json
```

Verification uses a private archive snapshot and checks signed file hashes,
OCI descriptors, and layer DiffIDs. It can run without Syd or a registry.

## Deploy

Use an Issuer that understands the OCI workload profile. Provision a non-root
service user with a writable delegated cgroup v2 subtree containing the
`cpu`, `memory`, and `pids` controllers. On AppArmor-restricted systems, grant
the selected Syd and extraction tools user namespace access through their
deployed executable profiles.

Provision a private state directory with space for short Unix socket paths.
Use a separate private cache volume with hard byte and inode limits. When
granting writable application data, provision a dedicated, bounded data volume
owned by the same service user. A finite volume is required for extraction
and writable data.

Set a static publisher anchor for offline package verification. Configure the
runtime separately from the package:

```toml
[package]
path = "service.actr"

[[trust]]
kind = "static"
pubkey_file = "publisher-public.json"

[hyper.oci]
syd-oci = "/opt/actr/bin/syd-oci"
umoci = "/usr/bin/umoci"
bubblewrap = "/usr/bin/bwrap"
supervisor = "/opt/actr/bin/actr-oci-supervisor"
state-dir = "/run/user/1000/actr-oci"
image-dir = "/var/lib/actr/oci-images"
policy = "/etc/actr/guest.syd-3"
cgroup-parent = "/sys/fs/cgroup/example-delegated-subtree"
package-sha256 = "WHOLE_SIGNED_ARCHIVE_SHA256"
allowed-capabilities = ["actor.receive"]
```

The file also needs the usual Issuer, signaling, and Realm configuration.
Set `memory-limit-bytes` under `[hyper.oci]` to a positive hard memory limit
for the Guest. The default is 256 MiB. Use `swap-limit-bytes = 0` to disable
Guest swap, or set a positive byte limit. Omitting it leaves swap governed
by the delegated cgroup. These settings apply to Guest execution. Image
extraction uses its own bounded cgroup.

The Syd policy is a host-owned file, tailored to the Guest, and must grant the
Guest socket connection with `allow/net/connect+/run/actr/guest.sock`. When
Landlock is enabled, also add `allow/lock/connect+/run/actr/guest.sock` for
kernels supporting pathname Unix socket restrictions (Landlock ABI 9+). For
the Tokio Guest,
grant creation of Syd's `!eventpoll` and `!eventfd` pseudo-descriptors when
the policy confines descriptor creation. The Tokio signal driver also uses
an unnamed Unix socket pair, which Syd checks under the `!unnamed` bind ACL.
The Linux test policy also uses Landlock with scoped rules for the executable
and data directory. End the policy with `lock:on`. Hyper verifies the policy
annotation before starting the Guest. Syd filters environment variable names
such as those matching `*TOKEN*` by default. Keep filtering enabled and
allow only the exact name of a required non-secret variable with
`passenv+NAME` in the host policy before `lock:on`. Its value comes from
the signed `[workload.oci.env]` table. A Guest with fixed model paths can
read `/data/model.gguf` and `/data/tokenizer.json` directly from a read-only
mount.

```sh
actr oci doctor --config actr.toml
actr run --config actr.toml
actr oci gc --config actr.toml
```

Use the SHA-256 of the whole signed archive for the host grant. The
`[workload.oci].digest` field holds the OCI image manifest digest. Open Join
uses the same verified archive digest as the host grant.

For a Service serving an Actrium Front with Actor channel authentication
disabled, set `[service_authorization].front_package` to the signed Front
archive. The runtime verifies the Front and checks its Service binding against
the same verified OCI archive digest.

## Capabilities

| Package declaration | Host grant |
| --- | --- |
| `fs.data.read-only` | A private directory mounted read-only at `/data`. |
| `fs.data.read-write` | The same mount with explicitly granted write access and hard byte and inode limits. |
| `net.https` | Signed terminal aliases and matching host grants for exact URLs, methods, and data classes. |
| `actor.receive` | Signed local unary routes and existing Hyper inbound ACL checks. |
| `actor.call` | Signed dependency aliases, routes and locks, plus host grants for exact Actor identities. |

The two filesystem access modes are mutually exclusive. A data grant has
`path`, `access`, `max-bytes`, and `max-inodes` under `[hyper.oci.data]`.
Writable data persists on this volume across orderly restarts.

For HTTPS, add entries under `[[hyper.oci.https]]` with `alias`, `url`,
`method`, and `data-classes`. Guest code calls
`Context::https_request(alias, body)`. Hyper enforces the existing public HTTPS
broker policy, including TLS validation, bounded requests and responses, and
one fixed URL per terminal.

For Actor RPC, add entries under `[[hyper.oci.actors]]` with `alias`, exact
`target` Actor ID, `actor-type`, target `package-sha256`,
`contract-fingerprint`, `routes`, and `data-classes`. Each tuple is asserted
by the deploying host and must match the signed dependency lock.
Set `allow-tell = true` to allow one-way sends on the granted routes. The OCI v1
Guest handles RETURN unary RPC and TELL on the reliable lane.

An SDK host can construct a Hyper with an explicit trust provider and use
`Node::attach_oci`, or verify once with `Hyper::verify_oci_package` and pass
that handle to `Node::attach_verified_oci` on the same Hyper instance.

## Troubleshooting

- Missing publisher trust, grant mismatches, unsupported protocol metadata, and
  failed OCI layer checks surface before Guest execution.
- An AMD64 userspace under OrbStack on Apple Silicon may report x86_64 while the
  Syd binary still returns `ENOSYS`. Qualify a Linux x86_64 kernel by testing the
  actual runtime. On Apple Silicon, use an OrbStack ARM64 Linux machine and
  a `linux/arm64` package.
- A caller deadline cancels that invocation's host operations. Hyper lets the
  Guest finish the in-flight IPC exchange within its own execution deadline so
  the next RPC can reuse the session. A transport failure or host deadline can
  still stop the Guest. If the caller loses the result after dispatch, use the
  operation ID in `DeliveryUncertain` to reconcile the outcome. When available,
  the supervisor's OOM report is included as the cause.
- If the supervisor cannot clean up an owned container, its private bundle
  remains available for inspection and reconciliation with `actr oci gc`.
  GC also reclaims abandoned unpack cgroups recorded in the private state
  directory after their processes stop.
- For systemd deployment, direct planned shutdown through Hyper before
  systemd's cgroup-wide termination. Keep cgroup-wide termination as the
  fallback for an unresponsive service.


## Run the Linux validation

The [OCI CI workflow](../../.github/workflows/ci-oci.yml) builds the Guest and
provides the procedure for signed local-package and capability tests. The test
launcher needs permission to create a transient delegated service and bounded
temporary volumes. It runs the workload as a non-root user. On ARM64, build
the static Guest for `aarch64-unknown-linux-gnu` and pass `--architecture arm64`
to `tests/e2e/actr/oci/make-layout.py`.

To compare OCI with Linked and DynClib, also enable the `dynclib-engine`
feature for the `oci_runtime_e2e` test. Build
`tests/e2e/actr/oci/bench-dynclib` and pass the resulting shared library as
`ACTR_OCI_BENCH_DYLIB` to the test launcher.
