# Actrix Realm v1 架构契约

状态：首版实现契约。它刻意不承诺多 Realm、HA 或跨 Realm federation。

## 1. 产品边界

一个生产 Actrix 实例承载且只承载一个 Realm。首版部署形态是单节点、
SQLite、官方 Actrix provider；Authority、Rendezvous、Relay、Publisher Registry
和 Management 是逻辑平面，可以同进程部署。Signer 支持同进程开发模式，也支持
通过现有 TLS/mTLS gRPC 客户端配置放入独立安全边界。

数据库用唯一表达式索引强制单 Realm，而不是只在管理 API 中约定。未 provision
Realm 时，公开 descriptor 返回 503；第二个 Realm 的并发创建在数据库层失败。
管理 API 不提供 Realm 硬删除；换 Realm 属于停机、备份、显式重新 provision 的
实例级操作，避免遗留 renewal token、registry 或 signaling cache 被误解释为新 Realm
的数据。

Workspace 不属于 Realm 基础设施的数据模型。Workspace 与 Service Actor 是多对多
使用关系，删除 Actor 不删除 Workspace；Workspace 密钥属于用户。这些规则由 Actro
实现，Actrix 不取得目录所有权。

## 2. 信任根

1. 系统管理员是本实例唯一 Realm 管理根。Admin UI 配置只接受 Argon2id PHC hash，
   不接受明文密码；管理会话使用进程内 CSPRNG 生成的 JWT key。
2. Realm Authority 的 Ed25519 key 签发 Actor credential。私钥留在 Signer；
   容忍窗口仅用于验签，绝不延长签名或 credential 生命周期。
3. Publisher ID 是规范化 DNS 域名，不含 scheme、端口、userinfo 或 path。HTTPS
   固定发现点是 `https://<publisher>/.well-known/actr/publisher.json`。
4. Publisher descriptor 中的 Ed25519 key 才是包的长期真实性根。registry/origin
   只是 HTTPS 分发位置，下载位置不是信任根；包最终按 digest 和 publisher signature
   验证。

Realm registration secret 是管理员/可信宿主能力，不是 Guest workload credential。
AIS 的注册入口同时要求 Realm secret 与 package/linked 身份证明；Actro/Rill host
可以持有该能力，但不得把它暴露给用户编写的 Rill Guest。注册请求携带的 ACL 因此
是宿主管理行为；无 ACL 时默认拒绝发现/中继。

Realm 服务端点可以包含非默认端口，descriptor 会精确发布端口。Publisher ID
禁止端口，因为端口属于可变路由而非长期身份；这也避免同一 DNS 所有者被人为拆成
多个身份。

## 3. 身份模型

Actor ID 是实例身份，不是用户、Workspace 或逻辑服务名。当前格式携带：

- Realm ID；
- 54-bit Snowflake serial；
- Actor 的出生类型 `publisher/program@version`。

Worker ID 为显式配置的 0..31 值，不再由 PID/hostname 的 5-bit hash 猜测。单节点
默认 0；未来并发 issuer 必须由部署租约分配不同 worker ID。实例升级若必须保留
Actor ID，应重签同一出生类型。SQLite 的 `issued_actor_identity` 永久登记本实例已
签发的 `(Realm, serial)`；进程重启或时钟回拨导致 Snowflake 候选碰撞时只会继续
分配，绝不覆盖原身份。跨 package version 升级视为新实例，稳定业务入口应使用
Service alias/ActorId 记录层表达，不能修改既有 Actor ID 的含义。

Service Actor 可用 renewal possession proof 在重启后保持 Actor ID；Tab Actor 的
新建策略属于 Actro。Tab 与 Service 不由基础设施强制 1:1，同一 Service Actor
允许多个 Tab/窗口连接和重连。

## 4. Descriptor 与 provider

公开 Realm descriptor：

`GET /.well-known/actr/realm.json`

schema 是 `actr.realm/v1`，声明 Realm、Authority issuer/audience、注册和验签端点、
官方 provider 的 signaling/STUN/TURN 端点以及本地 publisher registry。只发布
非秘密信息。

首版 provider ID 固定为 `actrix-official`。Actor credential 必须同时满足：

- `protocol_version = 1`
- `issuer = actr:realm:<realm_id>:authority`
- `provider = actrix-official`
- `audience = actr.signaling`

TURN credential 的 HMAC username 同时绑定 Realm、provider、`actr.turn` audience
和 Actor ID；TURN secret 至少按 Realm/provider 隔离。第三方 provider 在 v1 仅有
descriptor 数据契约，没有 federation、跨 provider 委托或信任协商协议。

## 5. Publisher 发现

Publisher descriptor schema 为 `actr.publisher/v1`：

```json
{
  "schema": "actr.publisher/v1",
  "publisher": "apps.example.com",
  "key_id": "actr-pub-<public-key-fingerprint>",
  "public_key": "<base64-ed25519-public-key>",
  "registries": ["https://registry.example.com/v1"],
  "origins": ["https://cdn.example.com/packages/"],
  "verification_challenge": "<short-lived-actrix-challenge>"
}
```

Actrix 验证时禁用重定向、限制响应为 64 KiB、拒绝内网/loopback/link-local DNS
结果，并把请求钉住到预检查的公网地址，避免 SSRF 和 DNS rebinding。域名验证后
完整 descriptor 会持久化并随 publisher 解析结果返回。

常规 publisher key rotation 不能由 Realm Admin 直接上传公钥覆盖：管理面先签发
新的短期 challenge，publisher 更新 HTTPS descriptor，Actrix 重新发现并验证后才
归档旧 key、启用新 key。旧 key 默认只用于历史包验签；泄露时由管理员显式 revoke。

程序身份写作 `publisher/program@version`。合法包可以由任意 descriptor 允许的
HTTPS origin/CDN 分发；消费端必须验证 manifest、二进制 digest、publisher key
签名和 key revocation 状态。

## 6. 数据与升级

schema v1 使用 SQLite `PRAGMA user_version = 1`。Actrix 遇到旧版或未知 schema
直接拒绝启动，不在启动路径中执行“尝试 ALTER 后忽略错误”。

Service Registry 把持久元数据与在线状态分开：注册在 SQLite 写入完成后才成功；
Actrix 重启恢复的记录可继续提供 ServiceSpec，但在原 Actor 认证重连前不能进入
Discovery 或 RouteCandidates。显式注销则先原子删除该 Actor 的缓存服务，再返回
成功，因而紧接着重启也不能复活已注销记录。

离线工具：

```bash
python3 scripts/migrate_schema_v1.py /var/lib/actrix/actrix.db --dry-run
python3 scripts/migrate_schema_v1.py /var/lib/actrix/actrix.db
```

旧 GitHub-style MFR 身份没有安全的域名映射。可先人工重新 provision，或显式使用
`--drop-publisher-data` 丢弃旧 registry/package 数据。工具通过 SQLite backup API
创建一致性备份；迁移失败时不删除备份。

## 7. 明确不做

- 一个进程多 Realm；
- 多节点 HA、自动 worker lease 或跨节点 SQLite；
- Realm federation、跨 Realm Actor ID 迁移；
- 第三方 signaling/TURN provider 的委托协议；
- Workspace 生命周期或 Workspace 密钥托管；
- 旧 credential、Actor ID、MFR 身份或协议的双轨兼容。

这些能力若进入后续版本，应先扩展 descriptor/protocol version，不得削弱 v1 的
单 Realm 不变量或默认拒绝策略。
