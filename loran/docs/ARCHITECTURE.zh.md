# Actrix Realm v1 架构

本页是代码导航；规范性设计以
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md)为准。

## 边界

目标契约：一个 Actrix 实例是共享 provider，承载来自多个命名空间的多个 Realm；
Realm 对应一次应用实例部署，标识为命名空间 + 本地名（如 `notes~dns:acme.com`），
以命名空间 key 为长期信任根。v1 是单节点、SQLite、官方 provider，不提供 HA 或跨
Realm federation。管理权限分实例运营者、命名空间签名请求与单 Realm 管理 token 三类。

当前实现：代码仍强制一个实例一个 Realm，系统管理员是唯一 Realm 管理根。本页的
「当前实现」描述以代码为准，差异清单见
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md) 第 9 节。

```text
受信 Actro/Rill Host
        │ Realm registration capability
        ▼
┌──────────────────── 单个 Actrix 进程 ────────────────────┐
│ Authority: AIS ──► Signer                               │
│       │ Actor credential / TURN credential              │
│       ├────────► Rendezvous: Signaling                   │
│       └────────► Relay: STUN / TURN                      │
│ Distribution: Publisher Registry                        │
│ Management: Admin UI/API + Realm 生命周期               │
└────────────────────────┬─────────────────────────────────┘
                         ▼
                    SQLite schema v1
```

这些是逻辑平面，不要求永远同进程。v1 默认 all-in-one 是为了减少部署依赖；Signer
可在需要时通过 TLS/mTLS gRPC 独立隔离。平面拆分不能绕过 issuer、provider、
audience 与 endpoint scope。

## 身份与信任

- 每个 Realm 的 issuer key（Ed25519）签发短期 Actor credential；issuer key 由命名
  空间 key 签名的 Realm Profile 委派，Actrix 不持有第三方命名空间 key。
- Actor ID 表示实例身份，包含 Realm 引用、Snowflake serial 和出生程序类型。目标
  形式为 `<serial_hex>@<local_name>~dns:<domain>/<publisher>:<program>:<version>`；
  当前实现仍使用数值 realm_id。
- 稳定 Service Actor 通过 renewal possession proof 在重启后保留 ID。
- 程序身份规范写作 `publisher/program@version`。
- Publisher 是规范化 DNS 域名。当前实现通过
  `https://<publisher>/.well-known/actr/publisher.json` 发现；目标契约改为 authority
  descriptor `https://<publisher>/.well-known/actr` 中带签名的可选成员 `publisher`，
  旧路径移除。
- HTTPS 证明域名控制与安全传输；长期包真实性由 publisher key、signature、
  digest 和 revocation 状态决定。
- Signaling 只做已认证 Actor 的 rendezvous，不签发身份，也不是信任根。
- TURN credential 绑定 Realm、provider、`actr.turn` audience、Actor 与过期时间；
  目标契约下 TURN secret 按 Realm 引用经 HKDF 派生。

Realm registration capability 只属于 Realm 管理员/可信宿主。用户编写的 Rill Guest
不得接触 Realm secret、Signer 私钥或管理 credential。

## 数据所有权

SQLite 持久化：

- Realm 与 Realm secret（当前实现：唯一 Realm；目标：多个 Realm 及其命名空间、
  Profile 与状态）；
- Signer key material（生产时由 KEK 加密）；
- Actor identity ledger 与 renewal 状态；
- ACL、Publisher descriptor/key/package metadata；
- 配置 override 与审计需要的 nonce。

Workspace 不属于 Actrix。Workspace 与 Service Actor 是多对多使用关系；关闭或删除
Actor 不删除目录；Workspace 加密密钥属于用户。这些规则由 Actro/宿主层实现。

## 请求流

### Realm 创建（目标契约）

1. 调用方提交由命名空间 key 签名的 `create_realm` 请求。
2. Actrix 按 authority-discovery 获取 `https://<domain>/.well-known/actr` 中的
   `realm_namespace` 并验签，创建 `pending` Realm，在 Signer 生成 issuer key，返回
   待签 Profile payload。
3. 调用方签名 Profile，以 `activate_realm` 提交；Actrix 验证 Profile 指向自己后转为
   `active`，在响应中一次性返回管理 token，此后才签发 credential。provider 自有命名空间
   由 Actrix 自签，用户以 provider 账号认证。

### Realm provision（当前实现）

1. 空 schema 启动，但没有 Active Realm。
2. 系统管理员通过受保护管理面创建唯一 Realm。
3. 数据库唯一约束拒绝第二个 Realm。
4. `/.well-known/actr/realm.json` 开始发布非秘密 descriptor。该路径与
   authority-discovery「`/.well-known/actr` 下不定义子路径」冲突，目标契约中移除。

### Actor 注册

1. 可信宿主提交 Realm capability 与 package/linked workload 身份证明。
2. AIS 验证 Realm（目标契约下须为 `active`）、Publisher/package、nonce、ACL 和注册模式。
3. AIS 从持久 identity ledger 预留 Snowflake serial。
4. Signer 签发 Actor credential；AIS 同时生成 scoped TURN credential。
5. Guest 只获得运行所需的最小能力。

### Signaling

1. 客户端在 WebSocket upgrade headers 或受约束 subprotocol transport 中传身份。
2. 服务验证 credential 的签名、期限、issuer/provider/audience 与连接 Actor。
3. envelope source 必须与连接身份一致；目标契约下还须与连接绑定的 Realm 一致。
4. ACL 默认拒绝；通过后才能发现或转发信令；发现只返回同 Realm 的 Actor。

### Publisher 与包

1. 当前实现：管理员录入 Publisher 域名，Actrix 生成短期 challenge，Publisher 在固定路径
   `/.well-known/actr/publisher.json` 发布带 challenge 的 descriptor。目标契约：Publisher
   在 authority descriptor `/.well-known/actr` 的 `publisher` 成员发布签名 Profile，并以
   Profile signing key 签名的 `register_publisher` 请求登记，不再使用 challenge。
2. Actrix 以 SSRF-safe、限大小的请求验证域名和 key。当前实现不跟随重定向；目标契约与
   Realm 相同，按 authority-discovery 跟随重定向（最多 5 次、每跳 HTTPS、始终用原域名
   验证 `authority` 绑定），每一跳执行 SSRF 检查与 DNS pinning。
3. 包发布使用一次性 nonce 与 Publisher 签名。
4. 消费端从允许的 HTTPS origin 下载后验证 digest 和签名；origin 不是信任根。

## 网络

HTTP 服务共享一个 HTTP/2/HTTPS listener；STUN/TURN 使用 ICE UDP listener 与独立
relay port range。端口可配置，descriptor 发布实际 endpoint。公网部署必须在反向
代理、证书、监听地址和 advertised endpoint 之间保持一致。

## 失败策略

- 配置未知字段、占位 secret、明文 admin 密码、生产无 TLS/KEK：拒绝启动。
- SQLite schema 不是 v1：拒绝启动并要求离线迁移。
- 当前实现：Realm 数量不是恰好一个时，依操作返回 503 或拒绝写入，不猜测默认
  Realm。目标契约：请求所属 Realm 不存在或不是 `active` 时拒绝，不猜测默认 Realm。
- credential scope、ACL 或 envelope identity 不一致：拒绝。
- Publisher DNS 指向非公网或发生重绑定：拒绝。
- Actor serial 候选碰撞：继续分配，不覆盖旧身份。

## 代码入口

| 领域 | 位置 |
|---|---|
| 进程与服务组合 | `crates/actrixd/src/service/` |
| 配置与 schema gate | `crates/platform/src/config/`、`storage/db.rs` |
| Realm/ACL | `crates/platform/src/realm/` |
| AIS/renewal | `crates/ais/` |
| Signer | `crates/signer/` |
| Signaling | `crates/signaling/` |
| STUN/TURN | `crates/stun/`、`crates/turn/` |
| Publisher Registry | `crates/mfr/`（内部历史 crate 名，不是公开术语） |
| 管理契约 | `crates/contracts/`、`crates/control/` |

## 已知源码命名债务

Actrix v1 的配置、HTTP/JSON、包清单和 Actor ID 使用 Publisher 契约；Actr SDK 的
protobuf/Rust `ActrType.manufacturer`、`manufacturer_auth_*` 以及部分项目 authoring
config 仍是早期源码名称。它们不改变本页的运行时信任边界，但属于开发者可见 API，
不能称为「纯内部实现」。全仓重命名需要作为独立 breaking migration 完成并同时更新
所有 bindings/codegen；本 PR 不以不安全的 serde alias 或双轨解析掩盖这项债务。

安全假设、P0/P1 和发布门槛见
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
