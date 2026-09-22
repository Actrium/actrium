# Actr Realm Profile

语言：[English](realm-profile.md) | **简体中文**

状态：初始草案。

本文定义 Actr 客户端或 provider 如何把 Realm 引用解析为经过验证的 Realm Profile、
`actr.realm/v1` Profile 格式，以及 provider 接受的管理请求。

## 1. 范围

Realm 是一个应用实例的信任与治理域。创建一个应用实例就创建一个新的 Realm。Realm 由
命名空间和本地名命名，写作 Realm 引用，例如 `k7m2q9xh~dns:acme.com`。Realm 引用的语法
见 [Actr URI](uri.zh.md) 第 7.1 节。

Realm Profile 是一份签名的公开文档，说明由哪个 issuer 和哪个 provider 服务该 Realm。
它由命名空间 key 签名，因此可以托管在任何地方：域名自己的静态托管、provider，或内嵌在
邀请中。取得它的位置不影响其有效性。

Realm 在整个生命周期内只由一个 provider 服务。provider 是由某个运营方运行的部署，为
Realm 提供 signaling、TURN 与 issuer。把应用迁到另一个运营方，意味着在那里新建 Realm
并迁移应用数据，旧 Realm 退役。

本文不定义 provider 提供的 signaling、identity 或 ICE 协议，不定义 Actor credential
格式，也不定义应用实例内部的 membership 与授权。

本文中的大写关键词 **MUST**、**MUST NOT**、**SHOULD**、**SHOULD NOT** 和 **MAY**
按 BCP 14 解释。

## 2. 命名空间

| 命名空间 | 所有者 | 信任根 |
| --- | --- | --- |
| `dns:<domain>` | 域名所有者 | `https://<domain>/.well-known/actr` 中 `realm_namespace` member 列出的命名空间 key |
| `local:<installation-id>` | 某个 Actr Host 安装 | Host 自己的本地记录 |

### 2.1 DNS 命名空间

DNS 命名空间与不含端口的 Actr authority `actr://<domain>` 同名。其 Ed25519 命名空间
key 是该命名空间下所有 Realm 的长期信任根，发布、轮换与回滚保护由
[Actr Authority Discovery](authority-discovery.zh.md) 第 4.4 节定义。域名只需公布一次
命名空间 key；创建 Realm 不改动域名托管的任何内容。

provider 自有命名空间（如 `dns:realms.provider.example`）是由 provider 持有 key 的普通
DNS 命名空间，让没有域名的用户也能创建 Realm（第 7.3 节）。

### 2.2 本机命名空间

本机命名空间属于某个 Host 安装。其 `installation-id` 是安装根公钥（原始 32 字节）
SHA-256 摘要前 16 字节的 RFC 4648 base32 小写编码，不带 padding。

本机 Realm 没有 Profile、没有 provider，也不使用管理请求。Host 为每个本机 Realm 保存
本地记录。本机 Realm 引用只在该安装上可用，其他方无法解析。本文其余部分只适用于 DNS
命名空间。

## 3. 定位 Realm Profile

对 Realm 引用 `<local_name>~dns:<domain>`，客户端：

1. 按 Actr Authority Discovery 取得并验证 `actr://<domain>` 的 authority descriptor 及其
   `realm_namespace` member；
2. 选出 `rel` 为 `realms`、protocol 为 `actr-realm-profile/1` 的 service link；
3. 按文档顺序逐条处理：去掉 `href` 末尾的 `/`，以 `Accept: application/json` 发送
   `GET <href>/<local_name>`，得到有效 Profile 即停止。

`realms` 的 href **MUST NOT** 含 query 或 fragment；**MAY** 指向域名自己的托管或
provider，**MAY** 带显式端口。所有 link 都未产出有效 Profile（`404 Not Found`、其他错误、超时或无效文档）时，视为 Profile
不可用。

该请求是公开的。客户端 **MUST** 使用 HTTPS，**MUST NOT** 发送 `Authorization` header、
cookie 或其他 credential。redirect 遵循 Actr Authority Discovery 的规则：最多五次，
每一跳都使用 HTTPS。provider 等特权进程 **MUST** 在每一跳额外执行 SSRF 检查：拒绝非公网
地址，把每一跳钉住到预检查过的地址，并把响应限制在 64 KiB。

Realm Profile 也 **MAY** 通过带外方式交付，例如内嵌在邀请中；客户端对这类 Profile 的
验证与通过 HTTPS 取得的完全相同。

## 4. Profile 格式

Realm Profile 是 UTF-8 JSON object，member 名使用 snake_case。

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
      { "id": "issuer-2026-2", "algorithm": "Ed25519", "public_key": "<base64>", "status": "active" }
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
      }
    ],
    "ice": [
      { "urls": ["stun:stun.provider.example:3478"] },
      { "urls": ["turns:turn.provider.example:5349"], "credential_audience": "turn-pool-1" }
    ]
  },
  "policy": {
    "membership": "invite-only"
  },
  "issued_at": "2026-07-29T00:00:00Z",
  "signature": { "key_id": "ns-2026-1", "algorithm": "Ed25519", "value": "<base64>" }
}
```

| Member | 含义 |
| --- | --- |
| `schema` | **MUST** 为 `actr.realm/v1`。 |
| `sequence` | 非负整数，该 Realm 每发布一次 Profile 都递增。 |
| `realm.namespace`、`realm.local_name` | Realm 引用，**MUST** 与正在解析的引用一致。 |
| `realm.display_name` | 人类可读名称，不是标识符。 |
| `realm.status` | `active` 或 `retired`。 |
| `issuer.id` | **MUST** 为 `actr:realm:<realm-ref>:authority`。该 Realm 的 Actor credential 以此作为 issuer。 |
| `issuer.url` | issuer 的注册与验证端点。 |
| `issuer.keys` | 用于验证 Actor credential 的 issuer 公钥。 |
| `provider.id` | provider 的规范化 DNS authority，在 Realm 生命周期内不变。 |
| `provider.signaling` | signaling 端点。共享同一 `control_plane_id` 的条目是同一 provider control plane 的副本或故障切换入口，不是 federation。 |
| `provider.ice` | STUN 与 TURN 端点。TURN 条目标明短期 TURN credential 的 `credential_audience`。 |
| `policy` | 该 Realm 的加入策略。 |
| `issued_at` | Profile 的签署时间（RFC 3339），仅供参考；Profile 不过期。 |
| `signature` | 命名空间 key 的 detached 签名。 |

Profile **MUST NOT** 包含 secret、私钥、access token、成员列表或用户专属数据。客户端
**MUST** 忽略未知 member。

## 5. 验证

`signature` 是对去掉 `signature` member 后的 Profile 按 RFC 8785 JCS 规范化结果所做的
detached Ed25519 签名。签署方 **MUST** 使用 `active` 命名空间 key。

客户端或 provider 只在以下条件全部满足时接受 Profile：

1. `schema` 为 `actr.realm/v1`，且 `realm.namespace` 与 `realm.local_name` 与正在解析的
   Realm 引用一致；
2. `signature.key_id` 指向已接受的 `realm_namespace` member 中列出的 key（Actr
   Authority Discovery 第 4.4 节），且用该 key 验签通过；
3. `issuer.id` 等于 `actr:realm:<realm-ref>:authority`；
4. 若此前已接受过该 Realm 的 Profile：
   - `sequence` 不低于已接受的最高 `sequence`，等于时内容必须与已接受的 Profile 完全相同；
   - `provider.id` 与已接受的 Profile 相同；
   - 已接受的 Profile 不是已退役状态。

有效性只取决于这些检查，与 Profile 的来源 URL 无关。因此托管 Profile 的 provider 不是
信任根。

`status` 为 `retired` 的 Profile 是永久 tombstone。接受它之后，客户端 **MUST** 停止使用
该 Realm。

## 6. 使用 Realm

客户端只通过已接受 Profile 指定的 issuer 和 provider 访问 Realm。该 Realm 的 Actor
credential 用 `issuer.keys` 验证，其 issuer **MUST** 等于 `issuer.id`。Realm membership
只决定主体能否进入该实例的网络；实例内的角色与权限由应用决定，不属于本文范围。

客户端 **SHOULD** 在与 Realm 建立会话时刷新该命名空间的 `realm_namespace` member，并从
`realms` 位置刷新 Profile，保持连接期间至少每 24 小时刷新一次。provider **SHOULD** 以同样
间隔为其服务的 Realm 刷新这两者；最新 Profile 已退役或不再列出自己用于签发的 issuer key
时，停止为该 Realm 签发 credential。

若签署已接受 Profile 的 key 已从命名空间的 `keys` 中移除（即吊销，见 Actr Authority
Discovery 第 4.4 节），客户端或 provider 丢弃该 Profile 及其 `sequence` 与 `provider.id`
基线，改用最后一份由仍列出的 key 签名的已接受 Profile；没有则按从未接受过的情况重新取得
Profile。这样命名空间所有者只需移除 key 并用 `active`
key 重签受影响的 Profile，即可清除用泄露 key 签发的 Profile，包括 `sequence` 被抬高的。

刷新时 Profile 不可用，客户端继续使用已接受的 Profile。取得的 Profile 验证失败时，客户端
忽略它、保留已接受的 Profile 并报告失败。从未接受过 Profile 且无法取得时，客户端
**MUST** 报错。任何情况下客户端都 **MUST NOT** 回退到 authority descriptor 的默认
service 或任何其他 provider。

## 7. 生命周期

### 7.1 创建

1. 命名空间 key 持有者向选定的 provider 发送 `create_realm` 请求（第 8 节）。provider
   创建 pending Realm，准备其 issuer key 与端点，并返回未签名的首个 Profile。
2. 命名空间 key 持有者签署该 Profile，并总是以 `activate_realm` 请求提交。provider 验证
   （第 5 节）后激活 Realm，在 `activate_realm` 响应中一次性返回该 Realm 的管理 token。
   若 `realms` 位置由 provider 托管，provider 此后对外提供该 Profile；否则由命名空间所有者
   发布到 `realms` 位置。

在 provider 自有命名空间中，Profile 由 provider 自己签署，无需 `activate_realm` 请求
（第 7.3 节）。未按时激活的 pending Realm 过期后被清除。

### 7.2 更新

`sequence` 更高的 Profile 可以更改 issuer key、provider 端点、`display_name` 与
`policy`，**MUST NOT** 更改 `provider.id`。

### 7.3 provider 自有命名空间

在 provider 自有命名空间中，provider 持有命名空间 key，为在该 provider 有账号的用户创建
Realm。Host 使用 OAuth 2.0 授权码流程加 PKCE（RFC 6749、RFC 7636）认证用户。该命名空间
authority descriptor 中 `identity` service link 的 `href` 即 OAuth issuer identifier，
授权服务器元数据位于其 RFC 8414 well-known 位置。授权服务器 **MUST** 接受固定的公共
client ID `actr-host`（无 client secret），redirect 按 RFC 8252 使用 loopback 或私有 URI
scheme；Host 请求 scope `realm_management`。随后 Host 以 access token 作为 bearer token
（RFC 6750）发送 `create_realm` 与 `reissue_admin_token`，不使用信封签名；provider 自己签署
Profile，并在 `create_realm` 响应中返回管理 token。丢失该响应的 Host 用
`reissue_admin_token` 取回 token。

配额与 Realm 的管理权（包括重新签发管理 token）绑定到账号，用户换机或丢失设备后只需重新
登录。账号注册、登录方式与防滥用措施由 provider 决定。

客户端 **MUST** 如实显示：provider 自有命名空间中 Realm 的信任根是 provider 运营方。

在组织拥有的命名空间中，由组织的签名服务按自身策略认证成员，代为签署请求与 Profile。
版本 1 不定义创建委派 key。

### 7.4 退役

退役只有一条路径：命名空间所有者发布 `status` 为 `retired` 的 Profile。provider 通过刷新
得知，写入本地 tombstone 并停止服务该 Realm。该本地名在该命名空间内永不复用。已退役 Profile 保持
相同的 `provider.id`，且 `sequence` 更高。若该 Realm 的 provider 已停服，命名空间所有者把
`realms` link 改指自己控制的位置，并在那里发布已退役的 Profile。provider 自有命名空间随其
provider 一起消失：其中的 Realm 无法退役，只能在别处重新创建。

### 7.5 命名空间 key 丢失

命名空间私钥全部丢失时，已有 Realm 无法再获得新 Profile。域名所有者可以发布新的命名空间
key（客户端按身份变化处理，见 Actr Authority Discovery 第 4.4 节），并在新的本地名下创建
新的 Realm。把丢失的 key 继续以 `retired` 状态列出，已有 Realm 就能继续使用当前 Profile。

## 8. Provider 管理请求

provider 以 HTTPS `POST` 请求的形式，在其自身 authority descriptor
`https://<provider_id>/.well-known/actr` 的 `realm-management` service link（Actr
Authority Discovery 第 4.3 节）接受以下管理请求：

| `op` | 用途 |
| --- | --- |
| `create_realm` | 创建 pending Realm，返回其未签名的首个 Profile。 |
| `activate_realm` | 在 `profile` 中提交已签名的首个 Profile；响应一次性携带管理 token。 |
| `reissue_admin_token` | 签发新的管理 token，并使旧 token 失效。 |

在 DNS 命名空间中，每个请求都用下述信封，由 `active` 命名空间 key 签名。在 provider 自有
命名空间中，请求携带相同的 payload（含 `provider_id`、`nonce` 与有效期）但不带签名，由账号
的 bearer access token 授权（第 7.3 节）。

`create_realm` 请求的 payload 为：

```json
{
  "op": "create_realm",
  "namespace": "dns:acme.com",
  "local_name": "k7m2q9xh",
  "provider_id": "provider.example",
  "requester": "<opaque member id>",
  "nonce": "<random>",
  "issued_at": "2026-07-29T00:00:00Z",
  "expires_at": "2026-07-29T00:05:00Z"
}
```

其他操作使用同一信封与各自的 `op` 和字段。`requester` 可选：组织签名服务把它设为所代签
成员的不透明标识。其他规范 **MAY** 以自己的 `op` 复用该信封，例如 Actr Authority
Discovery 第 4.5 节用它登记 publisher。

- payload 按 RFC 8785 JCS 规范化，附 detached Ed25519 签名及其 `key_id`。
- `provider_id` **MUST** 等于接收方 provider 的规范化 DNS authority，使请求无法重放到
  其他 provider。
- `expires_at` 与 `issued_at` 之差 **MUST NOT** 超过五分钟，provider 拒绝窗口之外的请求。
- provider 记录每个 `nonce` 直到请求过期，并拒绝重复使用。nonce 只防字节级重放。
- `create_realm` 以 `(namespace, local_name, 请求方)` 为幂等键。请求方为 `requester` 字段
  （缺省为空），provider 自有命名空间中为账号。同一请求方对自己的 pending 或 active Realm
  的重复请求返回该 Realm 的状态，不返回管理 token；指向其他请求方的 Realm 或已退役 Realm
  的请求，一律返回冲突。
- 对已 active 的 Realm 重复 `activate_realm` 返回冲突。管理 token 丢失只能用
  `reissue_admin_token` 取回。

接受签名请求前，provider 按与客户端相同的规则，通过 Actr Authority Discovery 验证签名的
命名空间 key。
由于这次抓取发生在验签之前，provider **MUST** 按来源地址和目标域名限流，**SHOULD** 缓存
结果（含失败）。provider **SHOULD** 按命名空间和按 Realm 执行配额。

## 9. 安全与隐私考虑

- 命名空间 key 价值很高，强烈建议离线或硬件保存；provider 不为第三方域名代持命名空间
  key。
- Profile 不过期。从未见过较新 Profile 的客户端，可能接受带外交付的旧 Profile（例如旧
  邀请中的）。从 `realms` 位置刷新可以缩小这一窗口；对见过较新 Profile 的客户端，
  `sequence` 可防止回滚。
- 禁止回退到默认 service，可防止攻击者阻断 Profile 获取后把客户端导向未经授权的
  provider。
- authority descriptor 不列出 Realm，但 Realm 引用在分享时会暴露命名空间与本地名。Host
  **SHOULD** 默认生成随机本地名，把可读名称放在 `display_name`。
- 泄露的命名空间 key 通过从 `keys` 中移除并重签受影响的 Profile 来吊销（第 6 节）。尚未
  见到更新后 `realm_namespace` member 的客户端无法分辨，这是残余风险，由刷新限定窗口。
- 移除 key 也会使建立在其所签 Profile 上的退役 tombstone 与 `provider.id` 基线失效。这是
  命名空间所有者信任边界内接受的后果：「本地名永不复用」是对所有者的要求，客户端只能对由
  仍列出的 key 签名的 Profile 强制执行。provider 本地 tombstone 不受影响。
- Host 在每个 Realm 使用独立的设备 key，Realm 之间无法据 key 关联同一设备。在 provider
  自有命名空间中，provider 能关联同一账号创建的 Realm（它本就为这些 Realm 签名）；Realm
  成员看不到账号。

## 10. 待完成工作

- 为 `actr.realm/v1` 与管理请求定义机器可读的 JSON Schema 与签名 test vector；
- 定义限定本地名前缀与有效期的创建委派 key。

## 11. 参考资料

- [Actr URI](uri.zh.md)
- [Actr Authority Discovery](authority-discovery.zh.md)
- [RFC 3339：Date and Time on the Internet: Timestamps](https://www.rfc-editor.org/rfc/rfc3339.html)
- [RFC 4648：The Base16, Base32, and Base64 Data Encodings](https://www.rfc-editor.org/rfc/rfc4648.html)
- [RFC 6749：The OAuth 2.0 Authorization Framework](https://www.rfc-editor.org/rfc/rfc6749.html)
- [RFC 6750：The OAuth 2.0 Authorization Framework: Bearer Token Usage](https://www.rfc-editor.org/rfc/rfc6750.html)
- [RFC 7636：Proof Key for Code Exchange by OAuth Public Clients](https://www.rfc-editor.org/rfc/rfc7636.html)
- [RFC 8032：Edwards-Curve Digital Signature Algorithm (EdDSA)](https://www.rfc-editor.org/rfc/rfc8032.html)
- [RFC 8252：OAuth 2.0 for Native Apps](https://www.rfc-editor.org/rfc/rfc8252.html)
- [RFC 8414：OAuth 2.0 Authorization Server Metadata](https://www.rfc-editor.org/rfc/rfc8414.html)
- [RFC 8785：JSON Canonicalization Scheme (JCS)](https://www.rfc-editor.org/rfc/rfc8785.html)
