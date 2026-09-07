# Actrix Realm v1 架构

本页是代码导航；规范性设计以
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md)为准。

## 边界

一个 Actrix 实例只承载一个 Realm。v1 是单节点、SQLite、官方 provider，不提供
HA、跨 Realm federation 或多租户 Realm 调度。系统管理员是 Realm 管理根。

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
│ Management: Admin UI/API + Realm descriptor             │
└────────────────────────┬─────────────────────────────────┘
                         ▼
                    SQLite schema v1
```

这些是逻辑平面，不要求永远同进程。v1 默认 all-in-one 是为了减少部署依赖；Signer
可在需要时通过 TLS/mTLS gRPC 独立隔离。平面拆分不能绕过 issuer、provider、
audience 与 endpoint scope。

## 身份与信任

- Realm Authority 以 Ed25519 签发短期 Actor credential。
- Actor ID 表示实例身份，包含 Realm、Snowflake serial 和出生程序类型。
- 稳定 Service Actor 通过 renewal possession proof 在重启后保留 ID。
- 程序身份规范写作 `publisher/program@version`。
- Publisher 是规范化 DNS 域名，通过
  `https://<publisher>/.well-known/actr/publisher.json` 发现。
- HTTPS 证明域名控制与安全传输；长期包真实性由 publisher key、signature、
  digest 和 revocation 状态决定。
- Signaling 只做已认证 Actor 的 rendezvous，不签发身份，也不是信任根。
- TURN credential 绑定 Realm、provider、`actr.turn` audience、Actor 与过期时间。

Realm registration capability 只属于管理员/可信宿主。用户编写的 Rill Guest
不得接触 Realm secret、Signer 私钥或管理 credential。

## 数据所有权

SQLite 持久化：

- 唯一 Realm 与 Realm secret；
- Signer key material（生产时由 KEK 加密）；
- Actor identity ledger 与 renewal 状态；
- ACL、Publisher descriptor/key/package metadata；
- 配置 override 与审计需要的 nonce。

Workspace 不属于 Actrix。Workspace 与 Service Actor 是多对多使用关系；关闭或删除
Actor 不删除目录；Workspace 加密密钥属于用户。这些规则由 Actro/宿主层实现。

## 请求流

### Realm provision

1. 空 schema 启动，但没有 Active Realm。
2. 系统管理员通过受保护管理面创建唯一 Realm。
3. 数据库唯一约束拒绝第二个 Realm。
4. `/.well-known/actr/realm.json` 开始发布非秘密 descriptor。

### Actor 注册

1. 可信宿主提交 Realm capability 与 package/linked workload 身份证明。
2. AIS 验证 Realm、Publisher/package、nonce、ACL 和注册模式。
3. AIS 从持久 identity ledger 预留 Snowflake serial。
4. Signer 签发 Actor credential；AIS 同时生成 scoped TURN credential。
5. Guest 只获得运行所需的最小能力。

### Signaling

1. 客户端在 WebSocket upgrade headers 或受约束 subprotocol transport 中传身份。
2. 服务验证 credential 的签名、期限、issuer/provider/audience 与连接 Actor。
3. envelope source 必须与连接身份一致。
4. ACL 默认拒绝；通过后才能发现或转发信令。

### Publisher 与包

1. 管理员录入 Publisher 域名，Actrix 生成短期 challenge。
2. Publisher 在固定 HTTPS well-known 路径发布 descriptor。
3. Actrix 以 SSRF-safe、无重定向、限大小的请求验证域名和 key。
4. 包发布使用一次性 nonce 与 Publisher 签名。
5. 消费端从允许的 HTTPS origin 下载后验证 digest 和签名；origin 不是信任根。

## 网络

HTTP 服务共享一个 HTTP/2/HTTPS listener；STUN/TURN 使用 ICE UDP listener 与独立
relay port range。端口可配置，descriptor 发布实际 endpoint。公网部署必须在反向
代理、证书、监听地址和 advertised endpoint 之间保持一致。

## 失败策略

- 配置未知字段、占位 secret、明文 admin 密码、生产无 TLS/KEK：拒绝启动。
- SQLite schema 不是 v1：拒绝启动并要求离线迁移。
- Realm 数量不是恰好一个：依操作返回 503 或拒绝写入，不猜测默认 Realm。
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
不能称为“纯内部实现”。全仓重命名需要作为独立 breaking migration 完成并同时更新
所有 bindings/codegen；本 PR 不以不安全的 serde alias 或双轨解析掩盖这项债务。

安全假设、P0/P1 和发布门槛见
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
