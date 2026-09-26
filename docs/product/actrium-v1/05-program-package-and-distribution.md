# 05. Program Package and HTTPS Distribution

## 1. Three layers of identity

The distribution design must keep three things separate:

| Layer | Identifier | What it proves |
|---|---|---|
| Publisher | Normalized domain name | Who currently controls the publishing entry point |
| Program Release | `publisher/program@version` + digest | Which immutable set of bytes to run |
| Service | ActorId + Actor credential | Which running instance is currently being talked to |

HTTPS cannot prove a program is not malicious; Marketplace review cannot prove an Actor's identity on the network; an Actor credential cannot prove that the Guest loaded locally is the correct version either.

## 2. Publisher identity

### 2.1 The domain is the Publisher ID

It is reasonable for a Publisher to use a domain name as its convention, because it directly reuses:

- DNS global uniqueness;
- domain ownership management;
- CA/TLS certificates;
- HTTPS;
- certificate transparency;
- CDN, cache, proxy and enterprise network facilities;
- an organization's existing security-response capability.

Actrium does not maintain a separate, global Publisher ID registry.

### 2.2 Publisher Profile

The Publisher Profile is published as the optional `publisher` member of the authority descriptor, in the domain's single bootstrap resource:

```text
GET https://<publisher>/.well-known/actr
```

The `publisher` member is defined in Actr Authority Discovery §4.5; per the authority-discovery rules, no sub-paths are defined under `/.well-known/actr`, so Publishers do not get a separate well-known file. The same descriptor may also carry `realm_signing_keys` (if the domain also has Realms) and `services`.

The bootstrap URL is derived directly from the Publisher's identity domain and must use standard HTTPS on port 443.

Fetching the descriptor follows the redirect rules of authority discovery: at most 5 hops, each over HTTPS; a redirect target never becomes a new authority, and the `authority` binding is still validated against the original Publisher domain. The validity of the `publisher` member depends only on its own signature and the pinned root/key history, not on the origin it was fetched from.

The `publisher` member's fields use snake_case and contain at least:

- `schema`: `actr.publisher/v1`;
- `domain`: the normalized Publisher domain, which must match the hostname of the descriptor's `authority`;
- current and historical Publisher Profile signing keys (`profile_signing_keys`);
- current and historical package signing keys;
- key id, algorithm, validity period and revocation status;
- a security contact address;
- optional Marketplace/transparency-log statements.

On first retrieval, trust comes from the Web PKI of the URL hostname, or from an enterprise-provisioned Publisher root pin; a root carried inside the Publisher Profile cannot vouch for itself in a circular way. After accepting the first Publisher Profile, Actrium pins the root/key history, and every later Publisher Profile must:

- be signed over a payload normalized per RFC 8785 JCS (the `publisher` member with its `signature` field removed), then verified as a detached Ed25519 signature;
- carry a monotonically increasing `sequence`, an issue time and an expiry time;
- introduce any new root only through a rotation chain signed by the old root;
- be rejected on rollback, expiry or unauthorized root change.

Parts of the descriptor outside `publisher` are unsigned and play no part in Publisher trust decisions.

An offline root is a strongly recommended Publisher operating practice, but whether hardware/offline storage is required can be raised further by Marketplace or enterprise policy; the wire protocol must support it.

### 2.3 Domain transfer

Control of a domain only determines the future publishing entry point; it must not silently rewrite history:

- an already-installed `Program Release + digest + signing key` can keep running offline;
- when a new key has no verifiable rotation chain from the historical key, treat it as a Publisher ownership change;
- an already-installed version must not change just because the current domain now returns a different package;
- a new install or update must display the identity change;
- high-risk policy may require re-approval by the Marketplace or an organization admin.

## 3. Where a package comes from

The platform is not responsible for "find the package by program name." A package has only two sources:

- **Opening an instance**: the Realm Profile's `package` gives the download address, SHA-256 and Actor type ([Actr Realm Profile](../../../actr/protocols/realm-profile.md));
- **Creating an instance**: the user selects one from the Program Catalog, the Marketplace, a local file, or a package address. Discovering programs and checking for new versions is the Marketplace's and the Publisher's own channel.

Whatever the source, verification is the same: check the SHA-256, then verify the package signature with the Publisher Profile's package signing key (§8). Where the download address is hosted has no bearing on security.

## 4. One Program Release, one `.actr` package

### 4.1 What the package is responsible for

Actrium's Program Release directly reuses the Actr `.actr` package:

- the executable body is the Service workload;
- Keel Guest is a signed package resource;
- the Front–Service proto/contract is a signed package resource;
- icons, static assets and default configuration go into the resource area;
- Actrium extension fields declare the front-end entry point, permissions, components, Workspace and compatibility range.

Actrium does not invent a second installer layer wrapped around `.actr`.

### 4.2 Recommended layout

```text
package.actr
├── manifest.toml
├── manifest.sig
├── manifest.lock.toml
├── bin/
│   └── service.wasm
├── ui/
│   ├── guest.bundle.js
│   └── capability-manifest.json
├── proto/
│   └── service.proto
└── assets/
    ├── icon.png
    └── ...
```

The manifest signature must bind the hashes of all resources, so replacing the Guest, the contract or the icon all change the package digest.

### 4.3 What a package must not contain

A formal third-party package must not contain:

- a Front Host binary;
- an alternative Keel Host implementation;
- a custom Host Module implementation;
- React Native native plugins;
- a Front dynamic library;
- executable scripts not listed in the manifest;
- a Guest entry point that is swapped in from the network at runtime.

A Publisher may only supply the Guest and its declarations; the Actr Host implementation always ships with the Actrium client.

## 5. Service workload policy

### 5.1 Marketplace default tier

In v1, a third-party Service workload on the Marketplace must be:

- WebAssembly Component Model;
- targeting an ABI on Actrium's supported list;
- passing the import/export allowlist;
- subject to fuel, wall-clock timeout, memory, table, stream and I/O quotas;
- brokered by the Service Host for all Workspace, network and Actor capability.

### 5.2 Native Service

Actr already has native cdylib/linked workload capability, but that does not mean the Marketplace should allow it by default:

- a native workload carries higher risk together with the Actrium process;
- platforms such as iOS forbid arbitrarily downloading and executing native code;
- dynamic libraries have ABI, signing and supply-chain differences.

So Native Service applies only to:

- Actrium's own first-party system programs;
- enterprise-managed distribution;
- desktop development mode;
- an elevated trust tier the user has explicitly accepted.

Native Service does not change the rule that Front only ever allows a Keel Guest.

Native workloads must be further distinguished:

- `NativeCdylib`: loaded into the Actr Host's address space; it cannot prove a closed egress boundary, and third-party instances must not be shown green;
- `NativeCapsule`: an independent, restricted process; it can only be assessed as `CONTROLLED` when OS sandboxing, no raw network/file access, sealed libraries and brokered IPC are all in effect;
- `NativeTrusted`: TCB shipped by Actrium itself or preset by an enterprise; not part of the open Marketplace trust tier.

The v1 Marketplace still opens only WASM Services. Native Capsule is a desktop-first follow-on capability; see [Privacy Marks and Aggregation Protocol](13-privacy-state-and-aggregation.md) for the constraints.

### 5.3 UI-only programs

v1 does not introduce a special case for "a page app with no Service." The toolchain generates a platform-defined, minimal WASM Service Component for a UI-only program:

- it still has the same ActrType and an independent ActorId as the Program Release;
- it has no Workspace or outbound network capability by default;
- it implements only session, health and an optional minimal key-value contract;
- it still goes through package signing, resource limits and the Service lifecycle the same way;
- a Publisher cannot use it to bypass the Host API or inject native code.

So every app Tab still uniformly Attaches to a Service; the only difference is whether that Service contains custom logic.

## 6. Actrium Manifest extensions

The Actr base manifest continues to provide:

- publisher;
- name/program;
- version;
- binary target/kind/hash;
- Ed25519 signature and key id;
- resources;
- proto files;
- dependency lock.

Actrium adds:

```toml
[actrium]
schema_version = 1
min_actrium_version = "1.0.0"
front_host_api = "^1.0"
service_host_api = "^1.0"

[actrium.front]
guest_runtime = "keel-js"
guest_entry = "ui/guest.bundle.js"
keel_api = "^1.0"
contract_manifest = "ui/capability-manifest.json"
contract_digest = "sha256:..."

[actrium.service]
contract = "proto/service.proto"
protocol_fingerprint = "sha256:..."

[actrium.service.workspace]
requirement = "required"
provisioning = "user-selected"
access = "read-write"
providers = ["managed-store", "local-folder", "cloud-drive"]
lease = "exclusive-writer"

[actrium.session]
attachment_policy = "exclusive"

[actrium.privacy]
schema_version = 1
data_classes = ["public", "private", "secret"]
default_unknown_action = "block-private-and-secret"

[[actrium.privacy.actor_egress]]
actr_type = "notes.example/index-worker@1.4.2"
required = false
data_classes = ["private"]
```

See the full example in [Reference Specification](12-reference-spec.md).

## 7. Permission, component and quota declarations

What a package declares is a "request," not an actual grant:

```text
effective capabilities =
  package requested
  ∩ Actrium Host supported
  ∩ user/admin granted
  ∩ current platform allowed
  ∩ current session scope
```

The manifest must declare at least:

- Front capability;
- Service capability;
- the set of Keel components;
- outbound network domains;
- camera/microphone/clipboard/notification/location;
- Workspace access mode;
- Guest heap, single-execution time, render ops;
- Service memory/fuel/storage/network;
- minimum Actr Host/SDK/contract version;
- data classes, expected Actor egress, terminal egress, required/optional, and the minimum node conclusion.

Adding a new high-risk permission requires a new version, and reconfirmation at upgrade time.

The Privacy Declaration is a static allowlist and installation notice, not the authoritative source of the runtime privacy graph. The Actr Host must check WASM imports and actual capability against the actual edges against the declaration; a Publisher marking a dependency as optional cannot override the closure judgment the Actr Host makes from the real data flow.

## 8. Download and verification pipeline

```text
Realm Profile package / user-selected package
  → HTTPS download
  → package digest
  → package signature/key history
  → ZIP structural limits
  → manifest identity
  → every file hash
  → ABI/contract/import validation
  → policy + permission review
  → content-addressed cache
  → Service Runtime / verified Guest bytes
```

### 8.1 Check order

1. verify the Publisher's DNS/IDNA;
2. TLS and hostname;
3. download size limit;
4. package SHA-256;
5. the Publisher Profile of the publisher declared in the manifest;
6. Ed25519 key id and signature;
7. ZIP path, file count, compression ratio and total decompressed size;
8. the manifest's publisher/program/version;
9. binary kind/target;
10. every resource/proto/lock hash;
11. the Guest's static dependencies and capability manifest;
12. the Service WASM imports;
13. Host API, Keel API and contract compatibility;
14. local/organizational security policy.

If any step fails, no part of the content may be handed to the Keel or Service runtime.

### 8.2 Keel loading boundary

Keel's `Engine.loadBundle(URL)` is only a loading mechanism, not an installation trust mechanism. Actrium must:

1. download and verify the package itself;
2. read the already-verified Guest bytes from the content-addressed cache;
3. hand the local bytes/code to Keel;
4. forbid the Guest from specifying an alternate URL at startup.

## 9. Version immutability

Actrium stores locally under the following composite key:

```text
(publisher, program, version, package_digest)
```

Rules:

- the digest is fixed after the first acceptance;
- the same Program Release appearing with a different digest is `PACKAGE_EQUIVOCATION`;
- "the Publisher updated the file" cannot be used to overwrite the cache;
- an update must use a new SemVer;
- a Service instance is fixed to its package digest;
- cache cleanup must not delete a digest still referenced by a Service;
- re-downloading must be re-verified and compared against the fixed digest.

A local first-seen pin only protects the current install; a Marketplace can help multiple clients compare digests they have seen. Providing global non-equivocation across all clients requires introducing a public, append-only transparency log later. v1 does not overstate a single-machine pin into global transparency.

## 10. The division of labor between signing and HTTPS

### What HTTPS provides

- proof of current domain control;
- transport confidentiality and integrity;
- the origin of the Publisher Profile;
- standard proxy/CDN/cache capability.

### What package signing and digests provide

- integrity across CDNs, mirrors and after being written to disk;
- offline verification;
- key-rotation history;
- exact version immutability;
- continued operation while the domain is temporarily unavailable;
- detection of same-name-different-package.

Both must be used together; neither substitutes for the other.

## 11. Marketplace

A Marketplace may:

- search, categorize and recommend;
- record Publisher verification;
- do static/dynamic review;
- give a risk rating;
- record Program Releases/digests it has seen;
- publish revocations and malware intelligence;
- provide a cache or mirror.

A Marketplace may not:

- squat a domain on a third party's behalf;
- silently replace a Publisher's package;
- substitute a marketplace login identity for the Publisher domain;
- bypass Actrium's permissions because something is "reviewed";
- equate delisting with automatically deleting the Service/Workspace.

## 12. Revocation policy

Revocation tiers:

| Tier | Behavior |
|---|---|
| Advisory | show the risk, allow continuing |
| Block new | block new installs/creations, prompt on existing instances |
| Quarantine | stop and isolate existing instances by default, user/admin can audit and act |
| Critical block | block loading the Guest/Service, keep a data-recovery entry point |

A revocation record must contain:

- Program Release/digest/key;
- the reason;
- the publication time;
- the publisher's or marketplace's signature;
- a recommended action;
- whether it affects only the Guest, the Service, or the signing key.

Revocation does not automatically delete the Workspace.

## 13. Update and upgrade

The Program Catalog may prompt about a new version, but a Service does not automatically upgrade in place.

v1 upgrade:

1. install the new Program Release;
2. create a new Service in the same Realm, obtaining a new ActorId;
3. the old Service produces a migration snapshot;
4. explicitly hand off the Workspace lease;
5. the new Service imports and verifies;
6. the user confirms the switch, after which a Realm Profile with `package` pointing at the new package is published via `publish_profile`;
7. the old Service is archived after a rollback window; rolling back means publishing another Realm Profile, with a higher `sequence`, that points back at the old package.

This avoids a mismatch between the version in an ActorId and the workload actually running, and keeps a failed rollback explainable.

## 14. Local development and self-hosting

Supported:

- a local `.actr` file;
- a package address on localhost, a loopback address, or an explicitly trusted private CA;
- a local development signing key;
- hot-updating the Guest;
- an unpublished Program Catalog entry.

Restrictions:

- every code change produces a new dev digest;
- an unverified key does not inherit the formal Publisher permissions of the same domain;
- a local package does not enter the formal Marketplace;
- private-network, loopback and link-local URLs are only allowed in development mode or under enterprise policy;
- a development package must not be used to bypass Native Service restrictions.

## 15. Cache and offline

The cache is content-addressed by package digest:

```text
cache/packages/sha256/<digest>/package.actr
cache/extracted/sha256/<digest>/...
```

Requirements:

- the original package is kept, for re-verification;
- extracted files are only ever produced from an already-verified package;
- atomic writes;
- incomplete downloads are isolated;
- cache metadata records the Publisher key and verification time;
- an existing local Service can keep running its fixed digest while the Publisher is offline;
- expiry of Realm credentials/revocations is handled under offline security policy, and is not equivalent to package verification failure.
