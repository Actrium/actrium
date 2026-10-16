# 12. 参考协议、样例与术语

> 本文件给出可讨论的 v1 wire shape。字段名和 endpoint path 在 P0 schema 冻结前可以调整；对象边界、身份与安全语义不得改变。

## 1. URI 样例

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

解码后的 v1 ActorId：

```text
2af01@42~dns:team.example/notes.example:team-notes:1.4.2
```

当前 Actr 的 legacy `2af01@42/notes.example:team-notes:1.4.2` 缺 Realm authority，只能通过显式导入流程补足，不能作为 portable Attach URI。

### Development

```text
actr-dev://localhost:7443/team-notes@0.0.0-dev
```

## 2. Publisher Profile

入口：

```text
https://notes.example/.well-known/actrium-publisher.json
```

样例：

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

`signature` 对象本身不进入 payload；其余 JSON 按 RFC 8785 JCS 规范化后做 detached Ed25519 验证。`sequence` 必须单调递增。

## 3. Release metadata

入口：

```text
https://notes.example/.well-known/actrium/v1/programs/team-notes/1.4.2/release.json
```

样例：

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

Release 的 `signature` 同样排除在 JCS payload 外，并由 Publisher Profile 授权的 signing key 验证。

## 4. `.actr` Manifest

示意：

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
actr_type = "notes.example:index-worker:1.4.2"
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

正式 schema 需要决定如何让 Actr 基础 manifest 保持向后兼容，并确保 `[actrium]` 字段也位于 manifest signature 覆盖范围内。

Native Capsule workload 另需：

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

同进程 `NativeCdylib` 不使用该 profile，也不能因此评为 `CONTROLLED`。

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

Actrium 不能只信这个文件；安装分析结果必须与它一致。

## 6. Realm Profile

入口：

```text
https://team.example/.well-known/actrium-realm.json
```

样例：

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

多个 signaling endpoint 共享 `controlPlaneId`，表示它们是同一 Realm presence/control plane 的副本或故障切换入口，而不是 v1 federation。

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

不包含 actor private key、Workspace credential 或 signaling/TURN token。

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

Guest 只读：

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

不提供：

- Front/Service credential；
- session id/secret；
- Workspace path；
- Actor arbitrary call API；
- Realm secret；
- signaling/TURN endpoint credential。

### 9.1 隐私状态上下文

Guest 只能看到最小化的用户可见摘要：

```json
{
  "privacy": {
    "color": "green",
    "ready": true,
    "policyId": "actrium.consumer.default",
    "workloadKind": "wasm-component",
    "applicationStateId": "…",
    "expiresAt": "2026-07-29T12:01:15Z"
  }
}
```

Guest 不获得 Actor 图、原始 evidence、attestation token 或被隐藏的 ActorId。完整对象见 [13](13-privacy-state-and-aggregation.md)；语义见 [docs/security](../../security/README.zh.md)。

## 10. 系统协议错误码

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

## 11. 状态枚举

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

## 12. 术语表

| 术语 | 定义 |
|---|---|
| Actrium | Actrium 程序浏览器与宿主平台 |
| Actr | Actor 通信、身份、发现和 workload runtime 框架 |
| Keel | 沙箱化动态原生 UI 的 Host/Guest runtime |
| Program | Publisher 命名空间内的程序 |
| Program Release | `publisher/program@version` 对应的不可变包 |
| Publisher | 以规范化 DNS 域名标识的发布者 |
| `.actr` package | Actr 签名 workload 包；在 Actrium 中同时携带 Service 与 Keel Guest 资源 |
| Tab | 用户交互容器 |
| Front Actor | 每个 resident app Tab 的短生命周期、Actrium 固定 Native workload Actor |
| Keel Guest | Publisher 唯一可定制的前台代码 |
| Primary Service | Front 唯一绑定的有状态业务 Actor |
| Auxiliary Actor | 由 Primary Service 编排的后台 Actor |
| ActorId | 携带 Realm authority、持久且密码学绑定的精确 Actor 身份 |
| ActrType | publisher/manufacturer + name/program + version |
| Workspace | Service 持有的业务数据 capability |
| Realm | Actor 身份、准入、ACL 和网络 Provider 的逻辑治理域 |
| Signaling | presence、发现与 WebRTC 协商控制面 |
| STUN | NAT 映射发现 |
| TURN | P2P 不可达时的受控流量中继 |
| Capability | Host 明确授予 Guest/Service 的最小能力 |
| Package digest | 精确 Program Release 的不可变内容指纹 |
| Attachment lease | Front 使用 Service 的短期会话租约 |
| Marketplace | 程序发现、审核和撤销信息服务，不是 Publisher 命名权威 |
| 应用图 | 有效 `Transfer(ActorId)` 形成的无根连通分量 |
| ActorAssurance | 验证者针对准确 `ActorStateId` 的节点结论；禁止自签 |
| ApplicationAssurance | 当前就绪切面的汇聚结论 |
| Ready | 快照当前且全部成员 ACTIVE；展示颜色的前提 |
| Native Capsule | 在独立 OS sandbox 中运行、只开放 Host brokered IPC 的 Native workload |

## 13. 规范与实现参考

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
