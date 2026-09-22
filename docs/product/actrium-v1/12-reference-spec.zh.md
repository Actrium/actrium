# 12. 参考协议、样例与术语

> 本文件给出可讨论的 v1 wire shape。字段名和 endpoint path 在 P0 schema 冻结前可以调整；对象边界、身份与安全语义不得改变。

## 1. URI 样例

### Create

```text
actr://notes.example/team-notes@1.4.2
```

在指定命名空间下创建（新建一个 Realm；省略 `namespace` 时创建到本机 `local` 命名空间）：

```text
actr://notes.example/team-notes@1.4.2?namespace=acme.com
```

Create URI 不携带 `local_name`。新 Realm 的 `local_name` 默认由 Actro 随机生成（base32 小写短串，不含程序名），可读名称写入 Realm Profile 的 `display_name`；上例默认得到的 Realm 引用形如 `k7m2q9xh~dns:acme.com`。用户可在确认页显式改用可读 `local_name`，下文 Attach 样例中的 `notes` 即此类显式选择的名字。

### Create + Guest route

```text
actr://notes.example/team-notes@1.4.2/today?filter=assigned
```

### Attach

```text
actr://notes.example/team-notes@1.4.2/today?service=2af01%40notes~dns%3Aacme.com%2Fnotes.example%3Ateam-notes%3A1.4.2
```

解码后的 v1 ActorId：

```text
2af01@notes~dns:acme.com/notes.example:team-notes:1.4.2
```

其中 `notes~dns:acme.com` 是 Realm 引用：命名空间 `dns:acme.com` 下本地名为 `notes` 的 Realm，即一个应用实例。

当前 Actr 的 legacy `2af01@42/notes.example:team-notes:1.4.2` 缺 Realm 引用，只能通过显式导入流程补足，不能作为 portable Attach URI。早期草案 `2af01@42~dns:team.example/...` 已作废。

### Development

```text
actr-dev://localhost:7443/team-notes@0.0.0-dev
```

## 2. Publisher Profile

Publisher Profile 是 authority descriptor 的可选成员 `publisher`（已在 Actr Authority Discovery §4.5 定义），入口为 Publisher 域名唯一的 bootstrap 资源：

```text
https://notes.example/.well-known/actr
```

样例（`publisher` 成员使用 snake_case；`domain` 必须与 `authority` 的主机名一致；`program_resolver` 必填，没有默认值）：

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

`publisher.signature` 对象本身不进入 payload；`publisher` 成员的其余 JSON 按 RFC 8785 JCS 规范化后做 detached Ed25519 验证。`sequence` 必须单调递增。descriptor 中 `publisher` 以外的部分不签名，不参与 Publisher 信任判断。

## 3. Release metadata

入口：

```text
https://notes.example/actrium/programs/team-notes/1.4.2/release.json
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

正式 schema 需要决定如何让 Actr 基础 manifest 保持向后兼容，并确保 `[actro]` 字段也位于 manifest signature 覆盖范围内。

Native Capsule workload 另需：

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

Actro 不能只信这个文件；安装分析结果必须与它一致。

## 6. Realm 命名空间与 Realm Profile

### 6.1 命名空间：authority descriptor 扩展

DNS Realm 命名空间即 Actr authority。命名空间描述位于该 authority 的 descriptor：

```text
https://acme.com/.well-known/actr
```

样例（`realm_namespace` 成员与 `realms` relation 已在 Actr Authority Discovery §4.3–4.4 定义）：

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
  "realm_namespace": {
    "sequence": 3,
    "keys": [
      {
        "id": "ns-2026-1",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "status": "active"
      },
      {
        "id": "ns-2025-1",
        "algorithm": "Ed25519",
        "public_key": "<base64>",
        "status": "retired"
      }
    ],
    "rotations": [
      {
        "from": "ns-2025-1",
        "to": "ns-2026-1",
        "sequence": 2,
        "signature": "<base64>"
      }
    ],
    "signature": {
      "key_id": "ns-2026-1",
      "algorithm": "Ed25519",
      "value": "<base64>"
    }
  }
}
```

`realm_namespace.keys` 是该命名空间下所有 Realm 的长期信任根；`rotations` 中每一项由 `from` key 对 `{from, to, public_key, sequence}` 的 JCS 形式签名，`sequence` 为引入该轮换时的成员 `sequence`；从 `keys` 移除某个 key 即吊销它签过的全部 Profile；`realm_namespace.signature` 由某个 `active` key 对去掉 `signature` 后的该成员 JCS 规范化结果签名，覆盖 `sequence`。descriptor 其余部分不签名，也不列出 Realm 清单。`services` 中的 signaling、ice-config 等是 authority 的默认 Provider，只作创建新 Realm 时的建议；具体 Realm 只按其 Profile 委派，Profile 不可用、无效或退役时不得回退到默认 Provider。Provider 自身的 descriptor（`https://provider.example/.well-known/actr`）另以 `realm-management` link（protocol `actr-realm-management/1`）公布管理请求端点。

### 6.2 Realm Profile

入口为 `realms` relation 的 `href` 加本地名：

```text
https://realms.provider.example/acme.com/k7m2q9xh
```

样例：

```json
{
  "schema": "actr.realm/v1",
  "sequence": 31,
  "realm": {
    "namespace": "dns:acme.com",
    "local_name": "k7m2q9xh",
    "display_name": "Acme Notes",
    "status": "active"
  },
  "issuer": {
    "id": "actr:realm:k7m2q9xh~dns:acme.com:authority",
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
    "key_id": "ns-2026-1",
    "algorithm": "Ed25519",
    "value": "<base64>"
  }
}
```

`signature` 由 `realm_namespace` 中的 `active` key 签署，托管位置不影响有效性；由后来退役的 key 签发的既有 Profile 在被更高 `sequence` 取代前仍有效。Profile 不设过期时间，回滚保护只靠 `sequence`；`provider.id` 是 Provider 的规范化 DNS authority，在 Realm 生命周期内不可变。多个 signaling endpoint 共享 `control_plane_id`，表示它们是同一 Provider presence/control plane 的副本或故障切换入口，而不是 v1 federation。同一 Provider 下轮换 issuer key 或变更端点时发布更高 `sequence` 的 Profile；退役时发布 `status` 为 `retired` 的新 Profile，永久有效。

## 7. Service Inventory Record

```json
{
  "actorId": "2af01@notes~dns:acme.com/notes.example:team-notes:1.4.2",
  "program": {
    "publisher": "notes.example",
    "name": "team-notes",
    "version": "1.4.2",
    "packageDigest": "sha256:..."
  },
  "realm": {
    "namespace": "dns:acme.com",
    "localName": "notes"
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
    "serviceActorId": "2af01@notes~dns:acme.com/notes.example:team-notes:1.4.2",
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
    "policyId": "actro.consumer.default",
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
URI_NAMESPACE_INVALID
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
REALM_NAMESPACE_INVALID
REALM_NAMESPACE_UNTRUSTED
REALM_NAMESPACE_KEY_CHANGED
REALM_PROFILE_INVALID
REALM_PROVIDER_MISMATCH
REALM_PROFILE_ROLLBACK
REALM_RETIRED
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
| Actro | Actrium 程序浏览器与宿主平台 |
| Actr | Actor 通信、身份、发现和 workload runtime 框架 |
| Keel | 沙箱化动态原生 UI 的 Host/Guest runtime |
| Program | Publisher 命名空间内的程序 |
| Program Release | `publisher/program@version` 对应的不可变包 |
| Publisher | 以规范化 DNS 域名标识的发布者 |
| `.actr` package | Actr 签名 workload 包；在 Actro 中同时携带 Service 与 Keel Guest 资源 |
| Tab | 用户交互容器 |
| Front Actor | 每个 resident app Tab 的短生命周期、Actro 固定 Native workload Actor |
| Keel Guest | Publisher 唯一可定制的前台代码 |
| Primary Service | Front 唯一绑定的有状态业务 Actor |
| Auxiliary Actor | 由 Primary Service 编排的后台 Actor |
| ActorId | 携带 Realm 引用、持久且密码学绑定的精确 Actor 身份 |
| ActrType | publisher/manufacturer + name/program + version |
| Workspace | Service 持有的业务数据 capability |
| Realm | 一个应用实例的逻辑信任与治理域：Actor 身份、准入、ACL 和 Provider 委派 |
| Realm 命名空间 | Realm 的命名空间：`dns:<domain>`（即不含端口的 Actr authority）或 `local:<installation-id>`；持有命名空间 key |
| Realm 引用 | `<local_name>~dns:<domain>` 或 `<local_name>~local:<installation-id>` |
| Realm Profile | 由命名空间 key 签名、委派 issuer 与 Provider 的每 Realm 文档 |
| Provider | 承载大量 Realm 的共享基础设施（signaling、STUN/TURN、issuer） |
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
