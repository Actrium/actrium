# 05. Program Package with HTTPS Distribution

## 1. Third-level identity.

The distribution design must separate three things:

| Layer | Identification | Prove what? |
|---|---|---|
| Publisher | Normalize domain names | Who controls the launch entrance now? |
| Program Release | `publisher/program@version` + digest | Which group to run is byte unvariable |
| Service Actor | ActorId + Realm credential | What instance of operation is currently communicating with |

HTTPS (a) The procedure cannot be proved to have been malicious;Marketplace Audits do not prove that there is a network. Actor (a) Identity;Actor credential It doesn't prove it's loaded locally. Guest Is the right version.

## 2. Publisher Identity

### 2.1 Domain name is Namespace

Publisher The agreement to use a domain name is reasonable because it is directly re-used:

- DNS Global uniqueness;
- (a) Domain name ownership;
- CA/TLS (a) Certificates;
- HTTPS;
- Transparency of certificates;
- CDN, cache, agent and enterprise network facilities;
- The organization has a security response capacity.

Actro Do not maintain another global set Publisher ID Registration centre.

### 2.2 Publisher Profile

A Publisher domain is also an Actr authority. The Publisher Profile is published as the optional `publisher` member of the authority descriptor, in the domain's single bootstrap resource:

```text
GET https://<publisher>/.well-known/actr
```

The `publisher` member is defined in Actr Authority Discovery §4.5. Authority discovery defines no path components below `/.well-known/actr`, so Publishers do not get a separate well-known file. The same descriptor may also carry `realm_namespace` (when the domain is also a Realm namespace) and `services` (such as `packages`).

The bootstrap URL is derived directly from the Publisher identity domain. It must use standard HTTPS on port 443 and cannot take a port from the `actr://` URI. Once the Profile has been verified, the `program_resolver`, CDN and other service endpoints it declares may use explicit ports.

Fetching the descriptor follows the redirect rules of authority discovery: at most five redirects, each over HTTPS; a redirect target never becomes a new authority, and the `authority` binding is still validated against the original Publisher domain. The validity of the `publisher` member depends only on its own signature and the pinned root/key history, not on the origin it was fetched from. To move the resolver, the old Profile signs a delegation to the new resolver, or a root rotation is performed.

The `publisher` member uses snake_case field names and contains at least:

- `schema`: `actr.publisher/v1`;
- `domain`: the normalized Publisher domain, which must equal the host of the descriptor's `authority`;
- current and historical Profile continuity keys;
- current and historical package signing keys;
- key id, algorithm, validity period and revocation status;
- `program_resolver`: the required Program resolver base, with no default;
- a security contact address;
- optional Marketplace/transparency log statements.

On first retrieval, trust comes from the Web PKI of the URL hostname, or from an enterprise-provisioned Publisher root pin; a root carried inside the Profile cannot vouch for itself. After accepting the first Profile, Actro pins the root/key history, and every later Profile must:

- be signed over an RFC 8785 JCS canonicalized payload (the `publisher` member with its `signature` field removed), verified as a detached Ed25519 signature;
- carry a monotonically increasing `sequence`, an issue time and an expiry time;
- introduce any new root through a rotation chain signed by the old root;
- be rejected on rollback, expiry or unauthorized root change.

The parts of the descriptor outside `publisher` are unsigned and play no part in Publisher trust decisions.

offline root It's highly recommended. Publisher Mode of transportation, but hardware required/Offline Save by Marketplace or an enhanced corporate strategy;wire protocol It must be supported.

### 2.3 Domain Name Transfer

Domain name control only determines the future launch entrance and cannot rewrite history silently:

- Installed `coordinate + digest + signing key` (b) Can continue offline;
- New key And history key When no chain of rotation is certified, consider Publisher ownership change;
- The installed version should not change with the current domain name returning the different package;
- New installation or updating must show identity changes;
- High-risk strategies can be required. Marketplace/The organizational manager again approves.

## 3. Coordinate Present. package ..the determination resolution of the

The Profile's required `program_resolver` determines the resolution base; the release description lives at:

```text
<program_resolver><program>/<version>/release.json
```

For example `https://notes.example/actrium/programs/team-notes/1.4.2/release.json`. The resolver may be any HTTPS origin and path and may use a non-standard port; it is declared by the verified Profile, and no well-known default path is used.

`release.json` It is a small, cached publication description, which includes at least:

```json
{
  "schemaVersion": 1,
  "publisher": "notes.example",
  "program": "team-notes",
  "version": "1.4.2",
  "packageUrl": "https://cdn.notes.example/actr/team-notes/1.4.2/package.actr",
  "packageSha256": "<64-hex>",
  "packageSize": 4829012,
  "signingKeyId": "ed25519:2026-q2",
  "publishedAt": "2026-06-01T10:00:00Z",
  "status": "active",
  "signature": {
    "keyId": "ed25519:2026-q2",
    "algorithm": "Ed25519",
    "value": "<base64>"
  }
}
```

Requests:

- in Path program/version Consistent with the response;
- production Only allowed HTTPS;
- Yeah. 3xx Jump to CDNBut eventually package It has to match. Publisher Description digest;
- release metadata Short-cut caches and expiry times are required;
- release descriptor You have to press it. JCS Regulate and by Profile Authorized. key Do it. detached signature;
- package It can be long in itself. immutable cache;
- Same coordinate Return Different digest Time hard fail.

Promise. `publisher/program@version` A valid download with certainty. `.actr` workload Package, and allow Publisher It's credible. Profile Self-selection after assignment resolver, CDN And the port.

## 4. One. Program ReleaseOne. `.actr` Package

### 4.1 Duties of the Mission

Actro Program Release Directly reuse Actr `.actr` Package:

- The main subject to execute is Primary Service workload;
- Keel Guest (b) As a signature bag resource;
- Front–Service proto/contract (b) As a signature bag resource;
- Icons, static resources, default configurations into the resource area;
- Actro Extension field states front desk entrances, privileges, components,Workspace Compatibility.

Actro No more inventions of a charade `.actr` The second-floor installation package outside.

### 4.2 Recommended Directory

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
│   └── primary-service.proto
└── assets/
    ├── icon.png
    └── ...
```

Manifest The signature must bind all resources hash, so replace Guest, contract Or the icon changes. package digest.

### 4.3 Nothing in the bag.

A formal third party package shall not contain:

- Front Host Binary;
- Keel Host Alternative realization;
- Custom Host Module Achieved;
- React Native Primary Plugins;
- Front Dynamic libraries;
- Not here. manifest The list of enforceable scripts;
- Replaced from network on run Guest The entrance.

Publisher Only if you're available. Guest (a) With the declaration;Host "and will always be." Actro Client release.

## 5. Service workload Policy

### 5.1 Marketplace Default Level

v1 Marketplace Third parties Service workload It must be:

- WebAssembly Component Model;
- Objective ABI Yes. Actro Supports the list;
- Pass. import/export allowlist;
- - Yeah. fuel, wall-clock timeout, memory, table, stream and I/O Quotas;
- All WorkspaceNetworks and networks Actor Capacity Service Host broker.

### 5.2 Native Service

Actr Existing native cdylib/linked workload Capability, but it's not equal to Marketplace By default, allow:

- native workload with Actro (a) The process is more risky;
- iOS The platform will not allow any random downloads of the machine code;
- Dynamic inventory in ABI, signature and supply chain differences.

And so... Native Service Only for:

- Actro (a) The first party system;
- (b) Corporate distribution;
- Desktop development mode;
- User explicitly accepted elevated trust tier.

Native Service It can't change. Front Still allowed only Keel Guest Rule.

Native workload A further distinction must be made:

- `NativeCdylib`: Load Host Address space; no export closure can be shown and no third-party examples may be green;
- `NativeCapsule`: the process of independence restricted, only OS sandboxNo original network/Documentation,sealed libraries and brokered IPC It's possible to rate it as all effective. `CONTROLLED`;
- `NativeTrusted`: Actro Home or enterprise preset TCBIt's not open. Marketplace trust tier.

v1 Marketplace It's still open. WASM Service.Native Capsule It is the desktop priority follow-up capability, as specified in the table.[Privacy tags and condensation protocols](13-privacy-state-and-aggregation.md).

### 5.3 UI-only Procedures

v1 Do Not Include“No, I don't. Service Page Application”Special case. The tool chain is UI-only Program generates the smallest of a platform definition WASM Service Component:

- It still has a connection. Program Release Same ActrType Independence ActorId;
- Default not Workspace and external network capacity;
- Only achieved session, health And the least option available key-value contract;
- Same as the signature, resource constraints and Service Life cycle;
- Publisher You can't use it to bypass. Host API Or inject. native code.

♪ So all ♪ app Tab Still unified. Attach One. Primary Service, the difference is business only Service Whether or not to include a custom logic.

## 6. Actro Manifest Extension

Actr Foundation manifest Continue to provide:

- manufacturer/publisher;
- name/program;
- version;
- binary target/kind/hash;
- Ed25519 signature and key id;
- resources;
- proto files;
- dependency lock.

Actro Add:

```toml
[actro]
schema_version = 1
min_actro_version = "1.0.0"
front_host_api = "^1.0"
service_host_api = "^1.0"

[actro.front]
guest_runtime = "keel-js"
guest_entry = "ui/guest.bundle.js"
keel_api = "^1.0"
contract_manifest = "ui/capability-manifest.json"
contract_digest = "sha256:..."

[actro.service]
primary_contract = "proto/primary-service.proto"
protocol_fingerprint = "sha256:..."

[actro.service.workspace]
requirement = "required"
provisioning = "user-selected"
access = "read-write"
providers = ["managed-store", "local-folder", "cloud-drive"]
lease = "exclusive-writer"

[actro.session]
attachment_policy = "exclusive"

[actro.privacy]
schema_version = 1
data_classes = ["public", "private", "secret"]
default_unknown_action = "block-private-and-secret"

[[actro.privacy.actor_egress]]
actr_type = "notes.example:index-worker:1.4.2"
required = false
data_classes = ["private"]
```

See full examples[Reference Agreements](12-reference-spec.md).

## 7. Declaration of competence, components and quotas

The package says:“Applications”, not actually authorized:

```text
effective capabilities =
  package requested
  ∩ Actro Host supported
  ∩ user/admin granted
  ∩ current platform allowed
  ∩ current session scope
```

Manifest At least declare:

- Front capability;
- Service capability;
- Keel (a) Component collections;
- outbound network domain;
- camera/microphone/clipboard/notification/location;
- Workspace access mode;
- Guest heapsingle execution time,render op;
- Service memory/fuel/storage/network;
- Minimum Host/SDK/contract Version.
- Data disaggregation, expected Actor Export,terminal egress, required/optional and the lowest conclusions.

Additional high-risk privileges must be issued and reconfirmed at the time of the upgrade.

Privacy Declaration It's static. allowlist and installation instructions are not the authoritative source of running privacy maps.Host We have to. WASM importsActual capability Compared to actual edge (a) A comparative statement;Publisher Mark Dependency as optional I can't cover it. Host The data flow is the basis of the closed package.

## 8. Download and verify current lines

```text
URI coordinate
  → Publisher Profile
  → Release metadata
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

1. Verify Publisher DNS/IDNA;
2. TLS and hostname;
3. Publisher Profile;
4. release coordinate;
5. (a) The download size limit;
6. package SHA-256;
7. Ed25519 key id and signatures;
8. ZIP path, number of documents, total compression ratio and total decompression;
9. manifest It's... publisher/program/version;
10. binary kind/target;
11. Every one. resource/proto/lock hash;
12. Guest Static Dependency and capability manifest;
13. Service WASM imports;
14. Host API, Keel API with contract Compatibility;
15. Local/Organizational security strategy.

No one can fail to give a part of it to you. Keel or Service runtime.

### 8.2 Keel Loading Borders

Keel `Engine.loadBundle(URL)` It is simply a loading mechanism, not a trust mechanism.Actro It is necessary to:

1. Download and authenticate on your own package;
2. From content-addressed cache Read certified Guest bytes;
3. Put Local bytes/code Here. Keel;
4. Ban Guest Specify replacement on startup URL.

## 9. Version is unchangeable

Actro Locally saves with the following joint keys:

```text
(publisher, program, version, package_digest)
```

Rules:

- Fixed after first acceptance digest;
- Follow-up and approval coordinate Different. digest = `PACKAGE_EQUIVOCATION`;
- It's not working.“Publisher Updated Document”Override caches;
- Update must use new SemVer;
- Service Examples fixed package digest;
- Cache Cleaning cannot delete still being Service Reference digest;
- Redownload must be verified and fixed again digest Compare.

Local first-seen pin Only current installations can be protected;Marketplace Helps multiple clients match already seen digest. To provide a global view of all clients non-equivocation, need to be introduced into the public afterward append-only transparency log.v1 No singles. pin Exaggerated global transparency.

## 10. Signature and HTTPS Division of labour

### HTTPS Provision

- Certificate of control of current domain name;
- (a) Transfer of confidentiality and integrity;
- Profile and release metadata (a) Sources of information;
- Standard Agent/CDN/Cache capacity.

### Package Signature and digest Provision

- CDN(c) The integrity of the mirror and the setting;
- Offline validation;
- key rotation history;
- (a) The exact version is not subject to change;
- (a) Continue to operate when domain names are temporarily unavailable;
- Test for the same name.

Both must be used simultaneously and not substitute for the other.

## 11. Marketplace

Marketplace This could include:

- Search, classification and recommendation;
- Records Publisher Validation;
- Static/(a) Dynamic audits;
- Risk rating;
- We've got the record. coordinate/digest;
- (b) Issuance of revocation and malicious software information;
- Provides caches or mirrors.

Marketplace You can't:

- (b) Acquiring for third parties the space for their domain names;
- Silent Replace Publisher package;
- Replace with market-entry identity Publisher Domain names;
- Because“Audited”Around Actro (a) Competence;
- Put the lower shelf with Service/Workspace Automatically deletes the equivalent.

## 12. Avoidance policy

Ungrade:

| Level | Behaviour |
|---|---|
| Advisory | Show risk and allow continuation |
| Block new | Stop installation/New, with instance tips available |
| Quarantine | Default stops and isolates already available instances, user/Administrators auditable |
| Critical block | Stop Loading Guest/Service, retain data for the re-entry |

The revocation record must contain:

- coordinate/digest/key;
- Reasons;
- (a) The time of publication;
- (a) The issuer or market signature;
- Recommended operations;
- Whether impact is limited Guest, Service Or sign key.

Undo does not automatically delete Workspace.

## 13. Update and Upgrade

Program Catalog You can suggest new versions, but... Service No automatic in situ upgrades.

v1 Upgrade:

1. Install new Program Release;
2. Create New Service Actor, get new ActorId;
3. Old Service (b) Generating migration snapshots;
4. Visible Intersection Workspace lease;
5. New Service Import and verify;
6. User confirmation switch;
7. New Tab URI Use New ActorId;
8. Old Service Keeps the back-rolling window after the archive.

It's a way to avoid it. ActorId Medium version versus actual workload Inconsistencies also make it possible to roll back and back.

## 14. Local development and self-building

Support:

- Local `.actr` Documentation;
- `actr-dev://` HTTPS Origin;
- Local development signing key;
- Hot Update Guest;
- Not published Program Catalog entry.

Limitation:

- Every code change makes new dev digest;
- Not validated key Do not inherit the same domain name formally Publisher (a) Competence;
- Local package is not entered officially Marketplace;
- The Internet,loopback, link-local URL Allows only in development models or business strategies;
- Development package Not for circumventing Native Service Limit.

## 15. Cache & Offline

Cache Press package digest Contents address:

```text
cache/packages/sha256/<digest>/package.actr
cache/extracted/sha256/<digest>/...
```

Requests:

- Original package (a) Retain for re-certification;
- extracted Only verified files package (a) Generation;
- Atom writing;
- (a) The downloading isolation was not completed;
- Cache metadata records Publisher key (a) The time of validation;
- Local Service Yes. Publisher Run fixed when offline digest;
- Realm Evidence/Discontinuation is processed under the offline security policy and is not equivalent to package Could not close temporary folder: %s
