# 12. Reference protocols, sample and terminology

> This document gives us a discussionable view. v1 wire shape. Field name and endpoint path Yes. P0 schema The freeze may be adjusted before it is frozen; the borders of the object, identity and the synonym of security may not be changed.

## 1. URI Sample

### Create

```text
actr://notes.example/team-notes@1.4.2
```

### Create + Guest route

```text
actr://notes.example/team-notes@1.4.2/today?filter=assigned
```

### Attach

```text
actr://notes.example/team-notes@1.4.2/today?service=2af01%4042~dns%3Ateam.example%2Fnotes.example%3Ateam-notes%3A1.4.2
```

After decoded v1 ActorId:

```text
2af01@42~dns:team.example/notes.example:team-notes:1.4.2
```

Current Actr It's... legacy `2af01@42/notes.example:team-notes:1.4.2` Missing Realm authority, can only be filled by a visible import process, not by portable Attach URI.

### Development

```text
actr-dev://localhost:7443/team-notes@0.0.0-dev
```

## 2. Publisher Profile

Entry:

```text
https://notes.example/.well-known/actrium-publisher.json
```

Example:

```json
{
  "schemaVersion": 1,
  "sequence": 17,
  "publisher": "notes.example",
  "programResolver": "https://notes.example/.well-known/actrium/v1/programs/",
  "profileSigningKeys": [
    {
      "id": "publisher-root-1",
      "algorithm": "Ed25519",
      "publicKey": "<base64>",
      "status": "active"
    }
  ],
  "keys": [
    {
      "id": "ed25519:2026-q2",
      "algorithm": "Ed25519",
      "publicKey": "<base64>",
      "notBefore": "2026-04-01T00:00:00Z",
      "notAfter": "2027-04-01T00:00:00Z",
      "status": "active"
    },
    {
      "id": "ed25519:2025",
      "algorithm": "Ed25519",
      "publicKey": "<base64>",
      "notBefore": "2025-01-01T00:00:00Z",
      "notAfter": "2026-06-30T00:00:00Z",
      "status": "retired"
    }
  ],
  "securityContact": "mailto:security@notes.example",
  "profileIssuedAt": "2026-07-29T00:00:00Z",
  "profileExpiresAt": "2026-08-29T00:00:00Z",
  "signature": {
    "keyId": "publisher-root-1",
    "algorithm": "Ed25519",
    "value": "<base64>"
  }
}
```

`signature` Object does not enter itself payload;the rest JSON Press RFC 8785 JCS Standardize and do it. detached Ed25519 Verify.`sequence` I'm going to have to do it on a one-way basis.

## 3. Release metadata

Entry:

```text
https://notes.example/.well-known/actrium/v1/programs/team-notes/1.4.2/release.json
```

Example:

```json
{
  "schemaVersion": 1,
  "publisher": "notes.example",
  "program": "team-notes",
  "version": "1.4.2",
  "packageUrl": "https://cdn.notes.example/actr/team-notes/1.4.2/package.actr",
  "packageSha256": "f6d4...64hex",
  "packageSize": 4829012,
  "signingKeyId": "ed25519:2026-q2",
  "publishedAt": "2026-06-01T10:00:00Z",
  "status": "active",
  "metadataExpiresAt": "2026-08-01T00:00:00Z",
  "signature": {
    "keyId": "ed25519:2026-q2",
    "algorithm": "Ed25519",
    "value": "<base64>"
  }
}
```

Release It's... `signature` Also excluded. JCS payload Outside and by Publisher Profile Authorized. signing key Verify.

## 4. `.actr` Manifest

Indications:

```toml
manufacturer = "notes.example"
name = "team-notes"
version = "1.4.2"
signature_algorithm = "ed25519"
signing_key_id = "ed25519:2026-q2"

[binary]
path = "bin/service.wasm"
target = "wasm32-wasip2"
kind = "component"
hash = "<sha256>"
size = 1832000

[metadata]
description = "Workspace-bound team notes"
license = "Apache-2.0"

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
contract_digest = "sha256:<digest>"

[actro.service]
primary_contract = "proto/primary-service.proto"
protocol_fingerprint = "sha256:<digest>"

[actro.service.workspace]
requirement = "required"
provisioning = "user-selected"
access = "read-write"
providers = ["managed-store", "local-folder", "cloud-drive"]
lease = "exclusive-writer"

[actro.session]
attachment_policy = "exclusive"
lease_seconds = 45
heartbeat_seconds = 15

[actro.privacy]
schema_version = 1
data_classes = ["public", "private", "secret"]
default_unknown_action = "block-private-and-secret"

[[actro.privacy.actor_egress]]
actr_type = "notes.example:index-worker:1.4.2"
required = false
data_classes = ["private"]

[actro.permissions.front]
capabilities = [
  "primary-service",
  "host:clipboard.write",
  "host:notifications"
]

[actro.permissions.service]
capabilities = [
  "workspace:read-write",
  "net:https://api.notes.example",
  "actor:call:notes.example:index-worker:1"
]

[actro.components]
allow = [
  "View",
  "Text",
  "ScrollView",
  "TextInput",
  "Button",
  "FlatList",
  "ImageAsset"
]

[actro.resources.front]
heap_bytes = 67108864
js_turn_timeout_ms = 100
max_render_nodes = 10000
max_batch_ops = 5000
max_rpc_concurrency = 32

[actro.resources.service]
memory_bytes = 268435456
max_mailbox = 4096
max_rpc_concurrency = 128
max_workspace_bytes = 10737418240

[[resources]]
path = "ui/guest.bundle.js"
hash = "<sha256>"

[[resources]]
path = "ui/capability-manifest.json"
hash = "<sha256>"

[[resources]]
path = "assets/icon.png"
hash = "<sha256>"

[[proto_files]]
name = "primary-service.proto"
path = "proto/primary-service.proto"
hash = "<sha256>"

[lock_file]
path = "manifest.lock.toml"
hash = "<sha256>"
```

Official schema It's a decision to make. Actr Foundation manifest Keep back-compatibility and ensure that `[actro]` Fields are also located manifest signature Coverage.

Native Capsule workload Additional requirements

```toml
[actro.native_capsule]
profile = "privacy-closed-v1"
network = "broker-only"
workspace = "broker-only"
child_process = false
ptrace = false
shared_writable_memory = false
dynamic_libraries = "sealed"
ipc = ["actr.capability.v1"]
```

Concurrent process `NativeCdylib` Do not use the profileAnd I can't judge him for that. `CONTROLLED`.

## 5. Keel Capability Manifest

```json
{
  "schemaVersion": 1,
  "keelApi": "1.0",
  "guestEntry": "ui/guest.bundle.js",
  "imports": [
    "react",
    "react/jsx-runtime",
    "keel/guest",
    "host:primary-service",
    "host:clipboard"
  ],
  "hostModules": {
    "primary-service": {
      "rpc": [
        "notes.v1.NoteService.List",
        "notes.v1.NoteService.Get",
        "notes.v1.NoteService.Save"
      ],
      "events": [
        "notes.v1.changed"
      ]
    },
    "clipboard": {
      "rpc": [
        "writeText"
      ]
    }
  },
  "components": [
    "View",
    "Text",
    "TextInput",
    "Button",
    "FlatList",
    "ImageAsset"
  ]
}
```

Actro This document cannot be trusted; the results of the installation must be consistent with it.

## 6. Realm Profile

Entry:

```text
https://team.example/.well-known/actrium-realm.json
```

Example:

```json
{
  "schemaVersion": 1,
  "sequence": 31,
  "realm": {
    "domain": "team.example",
    "realmId": 42,
    "displayName": "Example Team",
    "status": "active"
  },
  "profileSigningKeys": [
    {
      "id": "realm-root-1",
      "algorithm": "Ed25519",
      "publicKey": "<base64>",
      "status": "active"
    }
  ],
  "issuer": {
    "url": "https://identity.team.example",
    "keys": [
      {
        "id": "realm-key-2026-2",
        "algorithm": "Ed25519",
        "publicKey": "<base64>",
        "status": "active"
      }
    ]
  },
  "signaling": [
    {
      "url": "wss://signal-a.team.example/ws",
      "priority": 10,
      "controlPlaneId": "team.example/v1",
      "ticketAudience": "signal-a"
    },
    {
      "url": "wss://signal.provider.example:8443/ws",
      "priority": 20,
      "controlPlaneId": "team.example/v1",
      "ticketAudience": "signal-b"
    }
  ],
  "ice": [
    {
      "urls": ["stun:stun.team.example:3478"]
    },
    {
      "urls": ["turns:turn.provider.example:5349"],
      "credentialAudience": "turn-pool-1"
    }
  ],
  "policy": {
    "membership": "invite-only",
    "crossRealm": false,
    "offlineCredentialGraceSeconds": 0
  },
  "profileIssuedAt": "2026-07-29T00:00:00Z",
  "profileExpiresAt": "2026-08-29T00:00:00Z",
  "signature": {
    "keyId": "realm-root-1",
    "algorithm": "Ed25519",
    "value": "<base64>"
  }
}
```

Multiple signaling endpoint Share `controlPlaneId`It means they're the same. Realm presence/control plane A copy or a malfunction to switch the entrance instead of v1 federation.

## 7. Service Inventory Record

```json
{
  "actorId": "2af01@42~dns:team.example/notes.example:team-notes:1.4.2",
  "program": {
    "publisher": "notes.example",
    "name": "team-notes",
    "version": "1.4.2",
    "packageDigest": "sha256:..."
  },
  "realm": {
    "domain": "team.example",
    "realmId": 42
  },
  "workspaceId": "ws_01...",
  "runtime": {
    "placement": "local",
    "hostDeviceId": "device_...",
    "lifecycle": "running",
    "desiredState": "running",
    "reachability": "online",
    "health": "healthy"
  },
  "attachment": {
    "state": "leased",
    "policy": "exclusive"
  },
  "createdAt": "2026-07-29T10:00:00Z",
  "lastOpenedAt": "2026-07-29T11:00:00Z"
}
```

Other Organiser actor private key, Workspace credential or signaling/TURN token.

## 8. Permission Grant

```json
{
  "subject": {
    "publisher": "notes.example",
    "program": "team-notes",
    "majorVersion": 1,
    "packageDigest": "sha256:..."
  },
  "capability": "host:clipboard.write",
  "scope": {
    "serviceActorId": "2af01@42~dns:team.example/notes.example:team-notes:1.4.2",
    "workspaceId": "ws_01..."
  },
  "decision": "allow-while-open",
  "source": "user",
  "grantedAt": "2026-07-29T10:00:00Z",
  "expiresAt": null
}
```

## 9. Front Launch Context

Guest Read only:

```json
{
  "program": {
    "publisher": "notes.example",
    "name": "team-notes",
    "version": "1.4.2"
  },
  "route": "/today",
  "query": {
    "filter": "assigned"
  },
  "session": {
    "connection": "p2p",
    "realmDisplayName": "Example Team"
  },
  "workspace": {
    "displayName": "Team Notes",
    "mode": "read-write"
  },
  "theme": "system",
  "locale": "zh-CN"
}
```

Not provided:

- Front/Service credential;
- session id/secret;
- Workspace path;
- Actor arbitrary call API;
- Realm secret;
- signaling/TURN endpoint credential.

### 9.1 Privacy Context

Guest Only the minimalized user can be seen in summary:

```json
{
  "privacy": {
    "color": "green",
    "ready": true,
    "policyId": "actro.consumer.default",
    "workloadKind": "wasm-component",
    "applicationStateId": "…",
    "expiresAt": "2026-07-29T12:01:15Z"
  }
}
```

Guest Not obtained Actor Figure, original evidence, attestation token Or hidden. ActorId. See the full object [13](13-privacy-state-and-aggregation.md); semantics [docs/security](../../security/README.zh.md).

## 10. System protocol error code

### URI

```text
URI_SCHEME_INVALID
URI_PUBLISHER_INVALID
URI_PORT_NOT_ALLOWED
URI_PROGRAM_INVALID
URI_VERSION_REQUIRED
URI_VERSION_INVALID
URI_RESERVED_DUPLICATE
URI_ACTOR_ID_INVALID
URI_ACTOR_TYPE_MISMATCH
URI_REALM_REQUIRED
URI_REALM_MISMATCH
URI_LOCAL_REALM_FOREIGN
```

### Package

```text
PUBLISHER_PROFILE_INVALID
PUBLISHER_PROFILE_EXPIRED
PUBLISHER_PROFILE_ROLLBACK
PUBLISHER_IDENTITY_CHANGED
RELEASE_NOT_FOUND
RELEASE_SIGNATURE_INVALID
PACKAGE_DOWNLOAD_FAILED
PACKAGE_DIGEST_MISMATCH
PACKAGE_EQUIVOCATION
PACKAGE_SIGNATURE_INVALID
PACKAGE_KEY_REVOKED
PACKAGE_ARCHIVE_INVALID
PACKAGE_ABI_UNSUPPORTED
PACKAGE_REVOKED
GUEST_POLICY_INVALID
SERVICE_IMPORT_FORBIDDEN
```

### Realm/Identity

```text
REALM_PROFILE_INVALID
REALM_PROFILE_EXPIRED
REALM_PROFILE_ROLLBACK
REALM_LOGIN_REQUIRED
REALM_MEMBERSHIP_REVOKED
DEVICE_REVOKED
INVITE_EXPIRED
INVITE_ALREADY_USED
INVITE_REVOKED
INVITE_REALM_MISMATCH
```

### Service/Workspace

```text
SERVICE_CREATE_FAILED
SERVICE_IDENTITY_RECOVERY_FAILED
SERVICE_STOPPED
SERVICE_SUSPENDED
SERVICE_OFFLINE
SERVICE_UNREACHABLE
SERVICE_NOT_FOUND
SERVICE_MIGRATING
SERVICE_TRASHED
SERVICE_DELETED
WORKSPACE_REQUIRED
WORKSPACE_UNAVAILABLE
WORKSPACE_PERMISSION_DENIED
WORKSPACE_WRITE_LEASE_CONFLICT
WORKSPACE_QUOTA_EXCEEDED
```

### Session

```text
ATTACH_PERMISSION_DENIED
ATTACH_IDENTITY_MISMATCH
ATTACH_PROGRAM_MISMATCH
ATTACH_DIGEST_MISMATCH
ATTACH_PROTOCOL_MISMATCH
ATTACH_SESSION_OCCUPIED
ATTACH_REALM_MISMATCH
SESSION_EXPIRED
SESSION_REPLAYED
SESSION_TAKEOVER_DENIED
```

### Guest/Capability

```text
GUEST_LOAD_FAILED
GUEST_RUNTIME_TIMEOUT
GUEST_HEAP_EXCEEDED
GUEST_RENDER_LIMIT_EXCEEDED
CAPABILITY_NOT_DECLARED
CAPABILITY_NOT_GRANTED
CAPABILITY_PLATFORM_DENIED
CAPABILITY_RATE_LIMITED
CONTRACT_INPUT_INVALID
CONTRACT_OUTPUT_INVALID
```

### Assurance

```text
ASSURANCE_INVALID
ASSURANCE_EXPIRED
ASSURANCE_REVOKED
ASSURANCE_SUBJECT_MISMATCH
ASSURANCE_SELF_SIGNED
ASSURANCE_CONFLICT_WEAKEST
POLICY_UNSUPPORTED
GRAPH_INCOMPLETE
SNAPSHOT_NOT_CURRENT
GRAPH_LIMIT_EXCEEDED
ACTOR_SUSPENDED
UNDECLARED_EGRESS
GENERATION_VIOLATION
TERMINAL_NOT_AUTHORIZED
NATIVE_NOT_MEASURABLE
CHANNEL_INVALID
```

## 11. Status Count

### Program Release

```text
discovered
resolving
downloading
verifying
installed
update-available
identity-changed
revoked
quarantined
incompatible
```

### Service lifecycle

```text
creating
stopped
starting
running
suspended
stopping
migrating
archived
trashed
deleting
deleted
```

### Service desired state

```text
running
stopped
archived
```

### Service reachability

```text
online
offline
unreachable
unknown
```

### Service health

```text
healthy
degraded
failed
unknown
```

### Attachment

```text
idle
leased
occupied
```

### Application color

```text
green
yellow
red
unavailable
```

### Privacy enforcement

```text
allow
block
```

### Shell navigation

```text
parsing
resolving-package
awaiting-confirmation
creating-service
attaching
negotiating
loading-guest
failed
```

### Resident Front

```text
loading
running
suspended
failed
closing
closed
```

## 12. Glossary

| Terminology | Definitions |
|---|---|
| Actro | Actrium Program Browser and Host Platform |
| Actr | Actor Communications, identity, discovery and workload runtime Frame |
| Keel | Sandbox dynamic raw UI It's... Host/Guest runtime |
| Program | Publisher Program in Namespace |
| Program Release | `publisher/program@version` Corresponding Non-Variable Packages |
| Publisher | To regularize DNS Issuer of domain name identification |
| `.actr` package | Actr Signature workload packages;in Actro with Service with Keel Guest Resources |
| Tab | User Interactive Container |
| Front Actor | Every one. resident app Tab the short life cycle,Actro Fixed Native workload Actor |
| Keel Guest | Publisher Only custom front-office code |
| Primary Service | Front The only thing bound up is a status business. Actor |
| Auxiliary Actor | By Primary Service Organization of the background Actor |
| ActorId | Carry Realm authority- Persistence and precision of the coded. Actor Identity |
| ActrType | publisher/manufacturer + name/program + version |
| Workspace | Service Operational data held capability |
| Realm | Actor Identity, access,ACL and Network Provider The Logical Governance Field |
| Signaling | presence, Discovery and WebRTC Consultation and control |
| STUN | NAT Map found |
| TURN | P2P Controlled flow relays in unattractive times |
| Capability | Host Explicitly granted Guest/Service The minimum capacity |
| Package digest | Exact Program Release The unchangeable print. |
| Attachment lease | Front Use Service Short term conference lease |
| Marketplace | Program detection, audit and cancellation of information services, no Publisher Name Authority |
| Apply Graphics | Valid. `Transfer(ActorId)` The no root-connected weights formed |
| ActorAssurance | The certifier is accurate. `ActorStateId` and the prohibition of self-signatures |
| ApplicationAssurance | Convergence of current readiness |
| Ready | Quick-scenes current and all members ACTIVE;presupposing colours |
| Native Capsule | In independence OS sandbox Running, open only Host brokered IPC It's... Native workload |

## 13. Norms and practice references

### Actrium

- [Actr repository](https://github.com/Actrium/actr)
- [Keel repository](https://github.com/Actrium/keel)
- [Actr ActorId/ActrType formatting](https://github.com/Actrium/actr/blob/main/core/protocol/src/actr_ext.rs)
- [Actr package manifest](https://github.com/Actrium/actr/blob/main/core/pack/src/manifest.rs)
- [Actr package builder](https://github.com/Actrium/actr/blob/main/cli/src/commands/package_build.rs)
- [Keel Guest–Host interaction](https://github.com/Actrium/keel/blob/main/docs/architecture/guest-host-interaction.md)
- [Keel Guest runtime](https://github.com/Actrium/keel/blob/main/docs/architecture/guest-runtime.md)
- [Keel contract](https://github.com/Actrium/keel/blob/main/src/contract/index.ts)

### Internet standards

- [RFC 3986: URI Generic Syntax](https://www.rfc-editor.org/rfc/rfc3986)
- [RFC 7595: URI Scheme Guidelines](https://www.rfc-editor.org/rfc/rfc7595)
- [RFC 5890: IDNA Definitions](https://www.rfc-editor.org/rfc/rfc5890)
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785)
- [RFC 8489: STUN](https://www.rfc-editor.org/rfc/rfc8489)
- [RFC 8656: TURN](https://www.rfc-editor.org/rfc/rfc8656)
- [RFC 9530: TURN Third-Party Authorization](https://www.rfc-editor.org/rfc/rfc9530)
- [The Update Framework Specification](https://theupdateframework.github.io/specification/latest/)
