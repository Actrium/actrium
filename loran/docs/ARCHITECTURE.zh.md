# Loran Realm v1 架构

本页是代码导航；规范性设计以
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md)为准。

## 边界

目标契约：一个 Loran 实例是共享 Realm Provider，承载来自多个 Realm 域名的多个 Realm；Realm 对应
一次应用实例部署，以 Realm 名 `<realm_id>@<domain>`（如 `k7m2q9xh@acme.com`）标识，以
该 Realm 域名的 Realm 签名 key 为长期信任根。v1 是单节点、SQLite、单一 Realm Provider，
不提供 HA 或跨 Realm federation。管理权限分实例运营者、Realm 管理请求与单 Realm 的 Realm
管理 token 三类。
历史名称见[术语表](../../docs/GLOSSARY.zh.md)「历史名称对照」。

当前实现：代码仍强制一个实例一个 Realm，系统管理员是唯一 Realm 管理根。本页的
「当前实现」描述以代码为准，差异清单见
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md) 第 9 节。

```text
受信 Actr Host（Actro/Rill）
        │ Realm registration capability
        ▼
┌──────────────────── 单个 Loran 进程 ─────────────────────┐
│ Issuer ──► Issuer Signer                                 │
│       │ Actor credential / TURN credential              │
│       ├────────► Rendezvous: Signaling                   │
│       └────────► Relay: STUN / TURN                      │
│ Distribution: Publisher Registry                         │
│ Management: Admin UI/API + Realm 生命周期               │
└────────────────────────┬─────────────────────────────────┘
                         ▼
                    SQLite schema v2
```

这些是逻辑平面，不要求永远同进程。v1 默认 all-in-one 是为了减少部署依赖；Issuer
Signer 可在需要时通过 TLS/mTLS gRPC（`signer.v1`）独立隔离。平面拆分不能绕过 issuer、
Realm Provider、audience 与 endpoint scope。

## 身份与信任

- 每个 Realm 的 Realm issuer key（Ed25519）签发短期 Actor credential；Realm issuer key 由
  Realm 签名 key 签名的 Realm Profile 委派，Loran 未经委托不持有第三方域名的 Realm 签名 key。
- ActorId 表示实例身份，目标形式为 `<serial_hex>.<realm_id>@<domain>`，不含程序类型；
  程序类型由注册与 Actor credential 携带。当前实现仍使用数值 Realm 句柄（代码中的 `realm_id`）并内嵌程序类型。
- Service 通过 renewal possession proof 在重启后保留 ID。
- 程序身份规范写作 `publisher/program@version`。
- Publisher 是规范化 DNS 域名。当前实现通过
  `https://<publisher>/.well-known/actr/publisher.json` 发现；目标契约改为 authority
  descriptor `https://<publisher>/.well-known/actr` 中带签名的可选成员 `publisher`，
  旧路径移除。
- HTTPS 证明域名控制与安全传输；长期包真实性由 publisher key、signature、
  digest 和 revocation 状态决定。
- Signaling 只做已认证 Actor 的 rendezvous，不签发身份，也不是信任根。
- TURN credential 绑定 Realm、Realm Provider、`actr.turn` audience、Actor 与过期时间；
  目标契约下 TURN secret 按 Realm 名经 HKDF 派生。

Realm registration capability 只属于 Realm 管理员/可信 Actr Host。用户编写的 Rill Guest
不得接触 Realm secret、Issuer Signer 私钥或管理 credential。

## 数据所有权

SQLite 持久化：

- Realm 与 Realm secret（当前实现：唯一 Realm；目标：多个 Realm 及其 Realm 名、
  Realm Profile 与状态）；
- Issuer Signer key material（生产时由 KEK 加密，`ACTRIX_SIGNER_KEK`）；
- 已签发 Actor 身份登记与 renewal 状态；
- 连接 ACL、Publisher Profile/key/package metadata；
- 配置 override 与审计需要的 nonce。

Workspace 不属于 Loran。一个 Service 至多绑定一个 Workspace，同一 Workspace 可按
写入并发策略被多个 Service 使用；关闭或删除 Actor 不删除目录；Workspace 加密密钥属于用户。这些规则由 Actro 实现。

## 请求流

### Realm 创建（目标契约）

1. 调用方提交经 Realm 签名 key 持有方授权的 `create_realm` 请求（Actr 管理信封；Realm Provider
   域名下的 Realm 则为 Realm Provider 账号的 bearer token）。
2. Loran 按 authority-discovery 获取 `https://<domain>/.well-known/actr` 中的
   `realm_signing_keys` 并验签，创建 `pending` Realm，在 Issuer Signer 生成 Realm
   issuer key，返回待签 Realm Profile payload。
3. 调用方签名 Realm Profile，以 `publish_profile` 提交；Loran 验证 Realm Profile 指向自己
   后转为 `active`，在响应中一次性返回 Realm 管理 token，此后才签发 Actor credential。
   Realm Provider 域名下的 Realm 由 Loran 自签、无需此步，个人以 Realm Provider 账号认证，
   Realm 管理 token 在 `create_realm` 响应中返回。

### Realm provision（当前实现）

1. 空 schema 启动，但没有 Active Realm。
2. 系统管理员通过受保护管理面创建唯一 Realm。
3. 数据库唯一约束拒绝第二个 Realm。
4. `/.well-known/actr/realm.json` 开始发布非秘密 descriptor。该路径与
   authority-discovery「`/.well-known/actr` 下不定义子路径」冲突，目标契约中移除。

### Actor 注册

1. 可信 Actr Host 提交 Realm capability 与 package/linked workload 身份证明。
2. Issuer 验证 Realm（目标契约下须为 `active`）、publisher/package、nonce、连接 ACL 和
   注册模式。
3. Issuer 从持久的已签发身份登记预留 Snowflake serial。
4. Issuer Signer 签发 Actor credential（目标契约下携带 Actor 类型）；Issuer 同时生成 scoped
   TURN credential。
5. Guest 只获得运行所需的最小能力。

### Signaling

1. 客户端在 WebSocket upgrade headers 或受约束 subprotocol transport 中传身份。
2. 服务验证 Actor credential 的签名、期限、issuer/Realm Provider/audience 与连接 Actor。
3. envelope source 必须与连接身份一致；目标契约下还须与连接绑定的 Realm 一致。
4. 连接 ACL 默认拒绝；通过后才能发现或转发信令；发现只返回同 Realm 的 Actor。

### Publisher 与包

1. 当前实现：管理员录入 Publisher 域名，Publisher Registry 生成短期 challenge，Publisher 在固定路径
   `/.well-known/actr/publisher.json` 发布带 challenge 的 descriptor。目标契约：Publisher
   在 authority descriptor `/.well-known/actr` 的 `publisher` 成员发布签名 Publisher
   Profile，并以 Publisher Profile 签名 key 签名的 `register_publisher` 请求（Actr 管理信封）
   登记，不再使用 challenge。
2. Publisher Registry 以 SSRF-safe、限大小的请求验证域名和 key。当前实现不跟随重定向；目标契约与
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
- Actor credential scope、连接 ACL 或 envelope identity 不一致：拒绝。
- Publisher DNS 指向非公网或发生重绑定：拒绝。
- Actor serial 候选碰撞：继续分配，不覆盖旧身份。

## 代码入口

| 领域 | 位置 |
|---|---|
| 进程与服务组合 | `crates/loran-server/src/service/` |
| 配置与 schema gate | `crates/loran-platform/src/config/`、`storage/db.rs` |
| Realm/连接 ACL | `crates/loran-platform/src/realm/` |
| Issuer/renewal | `crates/loran-issuer/` |
| Issuer Signer | `crates/loran-signer/` |
| Signaling / Actor Registry | `crates/loran-signaling/` |
| STUN/TURN | `crates/loran-stun/`、`crates/loran-turn/` |
| Publisher Registry | `crates/loran-publisher-registry/` |
| 管理契约 | `crates/loran-proto/`、`crates/loran-control/` |

## 源码命名

历史名清理分支 `refactor/historical-name-cleanup-hardening` 已把代码中的历史名统一为
publisher、Publisher Registry、Issuer、Issuer Signer、Actor Registry（proto 字段编号不变，
包清单升为 `actr-package/2` 且不接受旧键，SQLite 表原地迁移），不使用 serde alias 或双轨
解析。仍沿用旧名的只有 `actrix*` 包名/二进制名，旧→新对照见[术语表](../../docs/GLOSSARY.zh.md)
「历史名称对照」。

安全假设、P0/P1 和发布门槛见
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
