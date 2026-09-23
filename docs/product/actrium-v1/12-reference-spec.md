# 12. Reference protocols, sample and terminology

> This document gives us a discussionable view. v1 wire shape. Field name and endpoint path Yes. P0 schema The freeze may be adjusted before it is frozen; the borders of the object, identity and the synonym of security may not be changed.

## 1. URI Sample

### Create

```text
actr://notes.example/team-notes@1.4.2
```

Create under a given domain (creates a new Realm; without `domain` the instance gets a local Realm):

```text
actr://notes.example/team-notes@1.4.2?domain=acme.com
```

A Create URI does not carry a `label`. By default Actrium generates the new Realm's `label` at random (a short lowercase base32 string with no program name), and the human-readable name goes into the Realm Profile's `display_name`; by default the example above yields a Realm name such as `k7m2q9xh@acme.com`. The user may explicitly choose a readable `label` on the confirmation page; `notes` in the Attach example below is such an explicitly chosen name.

### Create + Guest route

```text
actr://notes.example/team-notes@1.4.2/today?filter=assigned
```

### Attach

```text
actr://notes.example/team-notes@1.4.2/today?service=2af01.notes%40acme.com
```

The decoded v1 ActorId:

```text
2af01.notes@acme.com
```

Here `notes@acme.com` is the Realm name: the Realm with Realm label `notes` under the Realm domain `acme.com`, i.e. one application instance. The ActorId only says which Actor in which Realm; the program identity `publisher/program@version` is carried by registration and the Actor credential, not by the ActorId.

The current Actr legacy form `2af01@42/notes.example:team-notes:1.4.2` lacks a Realm name and embeds the program type; it can only be rewritten by an explicit import flow and cannot be used as a portable Attach URI. The earlier draft `2af01@42~dns:team.example/...` is withdrawn.

### Development

```text
actr-dev://localhost:7443/team-notes@0.0.0-dev
```

## 2. Publisher Profile

The Publisher Profile is the optional `publisher` member of the authority descriptor (defined in Actr Authority Discovery §4.5), served from the Publisher domain's single bootstrap resource:

```text
https://notes.example/.well-known/actr
```

Example (the `publisher` member uses snake_case; `domain` must equal the host of `authority`; `program_resolver` is required and has no default):

```json
{
  "version": 1,
  "authority": "actr://notes.example",
  "services": [
    {
      "rel": "packages",
      "href": "https://packages.notes.example/actr/packages",
      "protocol": "actr-packages/1"
    }
  ],
  "publisher": {
    "schema": "actr.publisher/v1",
    "sequence": 17,
    "domain": "notes.example",
    "program_resolver": "https://notes.example/actrium/programs/",
    "profile_signing_keys": [
      {
        "id": "publisher-root-1",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "status": "active"
      }
    ],
    "keys": [
      {
        "id": "ed25519:2026-q2",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "not_before": "2026-04-01T00:00:00Z",
        "not_after": "2027-04-01T00:00:00Z",
        "status": "active"
      },
      {
        "id": "ed25519:2025",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "not_before": "2025-01-01T00:00:00Z",
        "not_after": "2026-06-30T00:00:00Z",
        "status": "retired"
      }
    ],
    "security_contact": "mailto:security@notes.example",
    "profile_issued_at": "2026-07-29T00:00:00Z",
    "profile_expires_at": "2026-08-29T00:00:00Z",
    "signature": {
      "key_id": "publisher-root-1",
      "algorithm": "Ed25519",
      "value": "<base64>"
    }
  }
}
```

The `publisher.signature` object itself is excluded from the payload; the rest of the `publisher` member is canonicalized with RFC 8785 JCS and verified as a detached Ed25519 signature. `sequence` must increase monotonically. The parts of the descriptor outside `publisher` are unsigned and play no part in Publisher trust decisions.

## 3. Release metadata

Entry:

```text
https://notes.example/actrium/programs/team-notes/1.4.2/release.json
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
publisher = "notes.example"
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
contract_digest = "sha256:<digest>"

[actrium.service]
primary_contract = "proto/primary-service.proto"
protocol_fingerprint = "sha256:<digest>"

[actrium.service.workspace]
requirement = "required"
provisioning = "user-selected"
access = "read-write"
providers = ["managed-store", "local-folder", "cloud-drive"]
lease = "exclusive-writer"

[actrium.session]
attachment_policy = "exclusive"
lease_seconds = 45
heartbeat_seconds = 15

[actrium.privacy]
schema_version = 1
data_classes = ["public", "private", "secret"]
default_unknown_action = "block-private-and-secret"

[[actrium.privacy.actor_egress]]
actr_type = "notes.example/index-worker@1.4.2"
required = false
data_classes = ["private"]

[actrium.permissions.front]
capabilities = [
  "primary-service",
  "host:clipboard.write",
  "host:notifications"
]

[actrium.permissions.service]
capabilities = [
  "workspace:read-write",
  "net:https://api.notes.example",
  "actor:call:notes.example:index-worker:1"
]

[actrium.components]
allow = [
  "View",
  "Text",
  "ScrollView",
  "TextInput",
  "Button",
  "FlatList",
  "ImageAsset"
]

[actrium.resources.front]
heap_bytes = 67108864
js_turn_timeout_ms = 100
max_render_nodes = 10000
max_batch_ops = 5000
max_rpc_concurrency = 32

[actrium.resources.service]
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

Official schema It's a decision to make. Actr Foundation manifest Keep back-compatibility and ensure that `[actrium]` Fields are also located manifest signature Coverage.

Native Capsule workload Additional requirements

```toml
[actrium.native_capsule]
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

Actrium This document cannot be trusted; the results of the installation must be consistent with it.

## 6. Realm Signing Keys and Realm Profile

### 6.1 Realm signing keys: authority descriptor extension

A domain publishes its Realm signing keys in the descriptor of that domain (the Actr authority without a port):

```text
https://acme.com/.well-known/actr
```

Sample (the `realm_signing_keys` member and the `realms` relation are defined in Actr Authority Discovery §4.3–4.4):

```json
{
  "version": 1,
  "authority": "actr://acme.com",
  "services": [
    {
      "rel": "signaling",
      "href": "wss://signal.provider.example/actr",
      "protocol": "actr-signaling/1"
    },
    {
      "rel": "ice-config",
      "href": "https://rtc.provider.example/actr/ice",
      "protocol": "actr-ice-config/1"
    },
    {
      "rel": "realms",
      "href": "https://realms.provider.example/acme.com",
      "protocol": "actr-realm-profile/1"
    }
  ],
  "realm_signing_keys": {
    "sequence": 3,
    "keys": [
      {
        "id": "rs-2026-1",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "status": "active"
      },
      {
        "id": "rs-2025-1",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "status": "retired"
      }
    ],
    "rotations": [
      {
        "from": "rs-2025-1",
        "to": "rs-2026-1",
        "sequence": 2,
        "signature": "<base64>"
      }
    ],
    "signature": {
      "key_id": "rs-2026-1",
      "algorithm": "Ed25519",
      "value": "<base64>"
    }
  }
}
```

`realm_signing_keys.keys` is the long-term trust root of every Realm under the domain; each entry in `rotations` is signed by its `from` key over the JCS form of `{from, to, public_key, sequence}`, where `sequence` is the member `sequence` that introduced it; removing a key from `keys` revokes every Realm Profile it signed; `realm_signing_keys.signature` is made by an `active` key over the JCS canonical form of the member with `signature` removed, covering `sequence`. The rest of the descriptor is unsigned and does not list Realms. The signaling, ice-config and other entries in `services` serve the authority itself and have nothing to do with the Realms under the domain; a Realm is reached only through the Realm Provider named in its Realm Profile, and clients must not use any other Realm Provider when the Realm Profile is unavailable, invalid or retired. The Realm Provider's own descriptor (`https://provider.example/.well-known/actr`) advertises its Realm management request endpoint with a `realm-management` link (protocol `actr-realm-management/1`).

### 6.2 Realm Profile

The entry point is the `realms` relation `href` plus the `label`:

```text
https://realms.provider.example/acme.com/k7m2q9xh
```

Sample:

```json
{
  "schema": "actr.realm/v1",
  "sequence": 31,
  "realm": {
    "name": "k7m2q9xh@acme.com",
    "display_name": "Acme Notes",
    "status": "active"
  },
  "issuer": {
    "url": "https://identity.provider.example/realms/acme.com/k7m2q9xh",
    "keys": [
      {
        "id": "issuer-2026-2",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "status": "active"
      }
    ]
  },
  "provider": {
    "id": "provider.example",
    "signaling": [
      {
        "url": "wss://signal-a.provider.example/ws",
        "priority": 10,
        "control_plane_id": "provider.example/v1",
        "ticket_audience": "signal-a"
      },
      {
        "url": "wss://signal-b.provider.example:8443/ws",
        "priority": 20,
        "control_plane_id": "provider.example/v1",
        "ticket_audience": "signal-b"
      }
    ],
    "ice": [
      {
        "urls": ["stun:stun.provider.example:3478"]
      },
      {
        "urls": ["turns:turn.provider.example:5349"],
        "credential_audience": "turn-pool-1"
      }
    ]
  },
  "policy": {
    "membership": "invite-only"
  },
  "issued_at": "2026-07-29T00:00:00Z",
  "signature": {
    "key_id": "rs-2026-1",
    "algorithm": "Ed25519",
    "value": "<base64>"
  }
}
```

`signature` is made by an `active` key from `realm_signing_keys`, and the Actor credential issuer is `realm.name`; the storage location does not affect validity, and an existing Realm Profile signed by a key that is later retired stays valid until a higher `sequence` supersedes it. Realm Profiles do not expire; rollback protection relies only on `sequence`. `provider.id` is the Realm Provider's normalized DNS authority and is immutable for the Realm's lifetime. Multiple signaling endpoints sharing a `control_plane_id` are replicas or failover entries of the same Realm Provider presence/control plane, not v1 federation. Rotating Realm issuer keys or changing endpoints within the same Realm Provider publishes a Realm Profile with a higher `sequence`; retirement publishes a new Realm Profile with `status` set to `retired`, which stays valid forever.

## 7. Service Inventory Record

```json
{
  "actorId": "2af01.notes@acme.com",
  "program": {
    "publisher": "notes.example",
    "name": "team-notes",
    "version": "1.4.2",
    "packageDigest": "sha256:..."
  },
  "realm": "notes@acme.com",
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
    "serviceActorId": "2af01.notes@acme.com",
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
    "realmDisplayName": "Acme Notes"
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
    "policyId": "actrium.consumer.default",
    "workloadKind": "wasm-component",
    "applicationStateDigest": "…",
    "expiresAt": "2026-07-29T12:01:15Z"
  }
}
```

Guest Not obtained Actor Figure, original evidence, attestation token Or hidden. ActorId. See the full object [13](13-privacy-state-and-aggregation.md); semantics [docs/security](../../architecture/security/actrium/README.zh.md).

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
URI_DOMAIN_INVALID
URI_DOMAIN_NOT_ALLOWED
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
REALM_KEYS_INVALID
REALM_DOMAIN_UNTRUSTED
REALM_SIGNING_KEY_CHANGED
REALM_PROFILE_UNAVAILABLE
REALM_PROFILE_INVALID
REALM_PROVIDER_MISMATCH
REALM_PROFILE_ROLLBACK
REALM_RETIRED
REALM_LOGIN_REQUIRED
REALM_MEMBERSHIP_REVOKED
CLIENT_REVOKED
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

Terms used in these documents are defined in one place, the [glossary](../../GLOSSARY.zh.md); they are not repeated here.

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
