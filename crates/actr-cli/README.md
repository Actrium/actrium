# actr CLI

Command-line tool for Actor-RTC projects. Handles project scaffolding, codegen,
packaging (`.actr`), signing, installation of service dependencies, and runtime
instance management.

## Quick reference (21 top-level subcommands)

```
Development (flat, 5):  actr init | gen | build | check | doc
Runtime (flat, 7):      actr run | ps | logs | start | stop | restart | rm
Resources (grouped, 6): actr deps | pkg | registry | dlq | realm | publisher
Meta (3):               actr config | version | completion
```

Full help: `actr --help`, then drill down with `actr <command> --help`.

## Publisher Profile keys and signing

The Publisher Profile signing key signs the domain's public metadata. The
package signing key signs `.actr` packages. Generate them separately:

```sh
actr publisher profile keygen --key-id publisher-root-1 \
  --output publisher-profile-key.json > publisher-profile-public.json
actr pkg keygen --publisher apps.example.com --output package-key.json

actr publisher profile sign --key publisher-profile-key.json \
  --domain apps.example.com --package-key package-key.json \
  --security-contact mailto:security@apps.example.com > publisher.json
```

`keygen` writes the private key with owner-only permissions on Unix and prints
only its public `profile_signing_keys` entry. It refuses to replace an existing
file unless `--force` is given and leaves the configured package keychain alone.

`sign` prints the complete signed `publisher` member. Insert that object into
the authority descriptor served at `https://apps.example.com/.well-known/actr`,
keeping the descriptor's `authority` and service links. The command uses the
protocol's JCS and detached Ed25519 implementation, derives every package key ID
from its public key, and checks the result with the consumer's acceptance path.
Both commands work offline outside a project.

The default sequence is `1`, valid for 30 days. For an update with the same
Profile signing key, increase `--sequence` and renew before expiry; change the
validity with `--valid-for-days`. The command constructs a Profile with one
Profile signing key; rotation chains for that key remain a separate tool.
Repeat `--package-key` to list package keys. Each file may be a keychain or
public-only JSON with `public_key` and optional `status` (`active` or `retired`),
`not_before` and `not_after`. Keep retired package keys listed while their
packages must remain verifiable. The protocol allows at most eight package
keys, including at most two active keys. Private material never enters the
printed Profile.

The Profile key file also works with the existing Publisher registration tool:

```sh
actr realm register-publisher --key publisher-profile-key.json \
  --publisher apps.example.com --provider infra.example.com
```

---

## Realm package selection

Use a JSON file with `primary_package`, `front_package` and the complete `packages` map. The [signed Echo example](../../actr/protocols/examples/realm-profile.json) shows the two references and archive fields. Replace its example addresses and digests with the ones supplied by your Publishers.

```sh
actr realm create-request --key realm-key.json --realm echo@realm.example \
  --provider afp.example --selection selection.json > create-request.json
# POST the request to the AFP's realm-management endpoint and save the reply.
actr realm sign-profile --key realm-key.json --selection selection.json draft-reply.json > publish-request.json
# POST publish-request.json to the same endpoint.
actr realm fetch-packages --realm echo@realm.example --output ./echo-cache
```

`fetch-packages` verifies the signed Profile, both archives and their RPC contract. Its JSON output gives the local Service and Front paths. Reuse the same directory to retain trust and version pins. Add `--package <reference>` for a selected auxiliary. For Provider-domain Realms, an owner can replace the complete selection with `actr realm update --key client.json --selection selection.json`.

For Front releases under the Provider's domain, use the same output directory for the update and rollback.

```sh
actr realm update --key client.json --selection selection.json --output ./echo-cache
actr realm update --rollback --key client.json --output ./echo-cache
```

The CLI compares the complete selection with the signed Profile. It verifies changed archives before publishing and keeps the current sequence when the selection is identical. Rollback republishes the previous selection at a higher Profile sequence. If publication times out, retry the same command and selection to recover the recorded result.

For another Realm domain, the signing key holder runs `check-selection` before `sign-profile --front-operation <32-lowercase-hex-digits> --key realm-key.json --selection selection.json current-profile.json`. Submit the resulting `publish_front_profile` envelope to the Provider. Keep its `previous_selection` for rollback. When hosting the public Profile externally, confirm the receipt's sequence is still current with the Provider, then publish its signed `profile`.

For a stateless Service in a Provider-domain Realm, start the target with `actr run` using the verified archive and an owner Realm key. Keep the original running for rollback. Use a separate runtime config for the probe Front with a Realm member key, trust for the Front package, and permission to dial the target Service.

```sh
actr realm update --key owner.json --selection next-selection.json --output ./realm-cache \
  --probe-config probe-next.toml --probe-route example.Health.Ready
actr realm update --rollback --key owner.json --output ./realm-cache \
  --probe-config probe-original.toml --probe-route example.Health.Ready
```

The selection JSON can also carry a `readiness` object with `config` (relative to the JSON file), `route`, and optional `request_base64`. The readiness fields stay local to the command. Use a read-only RPC that returns success when the Service is ready. Pass `--probe-request` if it expects an encoded request body. The CLI requires one discoverable Actor of the exact Service package type, including its version. The Profile selects that package type. Include a compatible Front if the current Front locks the old Service version. The original and candidate runtime lifecycles remain under operator control.

An external Realm signing tool can run the same readiness check before signing its next Profile with `realm check-selection --probe-realm <realm> --probe-config <file> --probe-route <rpc>` plus the usual `--selection` and `--output`.

## Browser Host Realm client key

Run `actr run --web -c actr.toml`, open its local page, choose the authorized
Realm client key JSON file written by `actr realm join`, then select **Start
Actor**. The file must name the configured Realm and Issuer. The page passes
the key directly to its Service Worker before initialization; the worker
validates it and stores it in its scoped browser secret store. The page shows
the Actor as ready only after the worker confirms registration.

Select the key when opening the bundled page. Host-side
`deployment.realm_client_key` paths are not served to the browser, and private
key material is not included in `/actr-runtime-config.json` or uploaded to the
local static server. Package verification still uses public trust anchors.

## Organization principles

These rules govern where a new command lives. Add commands that fit; if a new
command doesn't fit, update this document **before** adding code.

### 1. Audience determines grouping

Commands serve three audiences. Keep their paths short in proportion to how
often the audience types them.

| Audience | Typical commands | Path depth |
|---|---|---|
| **Developer** (writes actor code) | `init`, `gen`, `build`, `check`, `doc` | flat |
| **Operator** (runs workloads) | `run`, `ps`, `logs`, `start`, `stop`, `restart`, `rm` | flat |
| **Integrator / publisher** (manages deps, packages, registry) | `deps ...`, `pkg ...`, `registry ...`, `dlq ...` | grouped |

**High-frequency stays flat** (Docker/Cargo precedent). **Low-frequency or
fine-grained stays grouped** so the top level stays scannable.

### 2. Group by resource, not by phase

Groups are named after **what the subcommand operates on**:

- `deps` — local dependency manifest / lockfile
- `pkg` — a local `.actr` package file (sign, verify, keygen)
- `registry` — the remote registry / Issuer (discover, fingerprint)
- `dlq` — the Dead Letter Queue
- `realm` — a Realm signing key (keygen, and signed Realm management requests),
  and a member's Realm client key (join, invite, members, …)
- `publisher profile` — a Publisher Profile signing key and the signed
  `publisher` member of a domain's authority descriptor

**Do not** group by workflow phase (`build`, `release`, `ops`, etc.) — those
names rot as the workflow evolves.

### 3. Full-flow commands vs fine-grained operations

- `actr build` — full source-to-package pipeline (compile → hash → sign → zip).
  The default entry point for developers.
- `actr pkg sign / verify / keygen` — single-step operations against an existing
  manifest or package. Intended for debugging, CI glue, or signed-build splits.

The rule: **one top-level command per full workflow**; put individual steps in
a group. Never expose both `foo` and `group foo` with overlapping semantics.

### 4. Parameter conventions

- Manifest path: `--manifest-path` / `-m` (follows Cargo). Default
  `manifest.toml`.
- Runtime config (`actr.toml`): `--config` / `-c`.
- Output file: `--output` / `-o`.
- Signing key: `--key` / `-k`.
- Never reuse `-f`; reserve single-letter short flags for obvious mappings.

### 5. Internal flags stay hidden

Flags that only another subcommand or a spawned child should set MUST use
`#[arg(long = "internal-...", hide = true)]`. Users browsing `--help` shouldn't
see implementation plumbing.

### 6. Every subcommand implements `core::Command`

All command types implement [`actr_cli::core::Command`][trait]:

```rust
async fn execute(&self, ctx: &CommandContext) -> anyhow::Result<CommandResult>;
fn required_components(&self) -> Vec<ComponentType>;
fn name(&self) -> &str;
fn description(&self) -> &str;
```

Dispatch is a single call in `src/cli.rs::run`; the service container is only
built when `required_components()` is non-empty. This keeps commands that don't
need the network (e.g. `keygen`, `version`, `pkg sign`) cheap.

[trait]: ./src/core/container.rs

### 7. Output style

Return a structured `CommandResult` (variants: `Success`, `Install`,
`Validation`, `Generation`, `Error`). The dispatcher in `cli.rs` renders it.
Avoid `println!` inside command logic when the result should flow through the
standard renderer.

### 8. No backwards-compat aliases (pre-1.0)

Until the CLI is released, break freely. Renames go in a single PR with
everything — call sites, e2e tests, scaffold templates, docs — updated atomically.
Do **not** ship transitional aliases.

---

## Adding a new command — checklist

1. Decide the audience (Dev / Operator / Integrator) → determines whether it
   sits at the top level or inside a group.
2. Pick a resource-oriented noun if grouping is required; extend an existing
   group before creating a new one.
3. Add the `#[derive(Args)]` struct in `src/commands/<name>.rs` and implement
   `core::Command`.
4. Register the variant in `src/cli.rs::Commands` and `as_command()`.
5. Re-export the type from `src/commands/mod.rs`.
6. Add a smoke assertion in `crates/actr-cli/tests/cli_shape.rs`.
7. Document the command in this README if it changes the top-level shape.

---

## Runtime config vs manifest

Two TOML files, easily confused:

| File | Owned by | Purpose |
|---|---|---|
| `manifest.toml` | developer | Project metadata, service `[package]`, proto exports, dependencies. Signed as part of `.actr`. |
| `actr.toml` | operator | Runtime config: package path, signaling URL, realm, data dir. Consumed by `actr run`. |

### WASM workload data

For an `actr-package/2` Component, declare the required capability in the signed
package manifest:

```toml
[host]
profile = "actr.host/1"
api = "^1.0.0"
required-capabilities = ["wasi.data.read-only"]
```

Use `wasi.data.read-write` instead if the workload needs to modify data. A
package can declare at most one of these capabilities.

The deployer grants access to a specific, existing directory in `actr.toml`:

```toml
[hyper]
wasi_data_dir = "./workload-data"
wasi_data_access = "read-only"
```

```sh
mkdir -p "$PWD/workload-data"
actr run --config actr.toml
```

Use a dedicated directory per workload. The Host exposes it at `/data` with
the access requested by the signed package. The deployment setting is a
ceiling: a read-only package gets a read-only mount even when the ceiling is
`"read-write"` (the default). A package that requires write access fails to
attach under a read-only grant. Without a signed data capability, the
directory is not mounted.
For `actr run`, setting `wasi_data_access` alone does not grant a directory;
configure `wasi_data_dir` as well.

When upgrading a workload that used `wasi_data_dir` before capability
declarations were enforced, add the required data capability to its package
manifest, then repack and re-sign it before starting it with the upgraded Host.
Legacy packages must first be rebuilt as `actr-package/2`. When a configured
directory is ignored because the package lacks a declaration, the Host logs a
warning and does not mount it.

`actr build` produces a legacy archive. For a Service with a built WASM
Component, create a separate `actr-package/2` manifest for `actr pkg pack`.
The `[host]` declaration belongs in this package manifest:

```toml
format = "actr-package/2"

[actor]
publisher = "example.com"
name = "my-service"
version = "1.0.0"

[workload]
profile = "actr.component/1"
entry = "workload/service.wasm"

[host]
profile = "actr.host/1"
api = "^1.0.0"
required-capabilities = ["wasi.data.read-write"]

[signature]
algorithm = "ed25519"
key-id = "placeholder"

[[files]]
path = "workload/service.wasm"
role = "workload"
media-type = "application/wasm"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
```

Save it as `package-v2.toml`, set the actor identity to the existing publisher
and Service name with a new release version, and put the built Component at
`payload/workload/service.wasm`. The publisher must match the signing key.
Pack and verify the archive with the Publisher key:

```sh
actr pkg pack --manifest-path package-v2.toml --payload-root payload \
  --key publisher-key.json --output service.actr
actr pkg verify --package service.actr --pubkey public-key.json
```

The packer recalculates the file size, SHA-256 and signing key ID. Add any
required RPC contracts, dependencies and lock file to the manifest and payload
for your Service. Update `actr.toml` to point to the new archive and retain its
trust anchor and data directory grant.

Relative paths resolve against the config file's directory. The directory
cannot be a filesystem root. On macOS, the Host process needs read and
search permissions on each ancestor of the resolved directory to validate the mount.
On Linux, the Host requires access to `/proc/self/fdinfo` and
`/proc/self/mountinfo` to verify the directory is not a filesystem root.
Unknown keys in `[hyper]` are rejected so a misspelled access limit fails
configuration loading.

Embedded hosts using `Node::from_hyper` inherit the directory and access limit
from the runtime configuration by default. `HyperConfig` can deny the grant
with `WasiDataDir::Disabled`, override the path with `WasiDataDir::Directory`
while keeping the deployment access limit, or use
`WasiDataDir::ReadOnlyDirectory` to supply an explicitly read-only absolute
directory. Restart or reattach the workload to apply a grant change.

Commands that touch `manifest.toml` use `--manifest-path` / `-m`.
Commands that touch `actr.toml` use `--config` / `-c`.

---

## See also

- `src/cli.rs` — top-level `Cli` struct and dispatcher.
- `src/core/container.rs` — `Command` trait and `ServiceContainer`.
- `src/commands/` — one file per subcommand.
