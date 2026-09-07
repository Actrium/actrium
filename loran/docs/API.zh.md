# Actrix Realm v1 API 边界

状态：首版契约。本文只描述当前实现，不承诺旧 KS/MFR HTTP API 兼容。

## 传输与认证规则

- 生产实例只接受 HTTPS/WSS；端口可以配置，但 endpoint/audience 必须进入凭证作用域。
- 凭证不得放在 URL、query string、普通访问日志或错误文本中。
- 原生 Signaling WebSocket 使用 `x-actr-id` 与
  `Authorization: ActrCredential <base64url-protobuf>` upgrade headers。浏览器不能设置这
  两个 header，因此改用 `actr.v1`、`actr.id.<base64url>` 和
  `actr.credential.<base64url-protobuf>` 子协议项；服务器验证全部身份后只协商返回
  `actr.v1`，避免把凭证暴露为页面可读的已选子协议。
- Realm 管理、Publisher 管理和配置变更只允许系统管理员调用。
- Signer 是内部信任平面；规范入口是同一 HTTP/2 监听器上的 gRPC，
  不是公开 REST 密钥下载服务。

## Realm discovery

`GET /.well-known/actr/realm.json`

成功时返回当前实例唯一 Active Realm 的 v1 descriptor，包括 issuer、官方 provider、
Signaling/TURN audience 与 endpoint。没有且仅有一个可用 Realm 时返回 `503`，不猜测。

## Authority（AIS）

路由前缀：`/ais`。

注册、续签与 signing-key 查询使用 protobuf body。Realm secret 是管理员或受信宿主的
注册能力，不得下发给 Rill Guest。签发的 identity claims 固定包含：

- `protocol_version = 1`
- `issuer = actr:realm:<realm_id>:authority`
- `provider = actrix-official`
- `audience = actr.signaling`
- Realm、完整 Actor ID 与过期时间

## Signaling

`GET /signaling/ws` 升级到 WebSocket。身份只从原生 upgrade headers 或上述浏览器
子协议项读取；连接身份、credential actor 与每条 envelope 的 source 必须完全一致。
单帧最大 1 MiB，并执行连接/消息速率限制。

## Publisher Registry

公开路由前缀：`/registry`。

- `GET /publishers/{domain}/verifying-key`
- `POST /packages/nonce`
- `POST /packages/publish`

v1 JSON 统一使用 Publisher 术语：

```json
{"publisher":"apps.example.com"}
```

发布 body 的身份字段同样为 `publisher`，成功返回的 package 使用
`publisher_id`、`publisher` 和规范 `type_str`：

```json
{
  "publisher_id": 7,
  "publisher": "apps.example.com",
  "name": "editor",
  "version": "1.2.0",
  "type_str": "apps.example.com/editor@1.2.0"
}
```

旧 `mfr`、`mfr_id`、`manufacturer` JSON/query 名称不属于 v1 契约。内部 SQLite
列名和 crate 名不构成 API。

管理路由前缀：`/admin/api/publishers`，受 Admin JWT 保护。Publisher ID 是规范化 HTTPS
域名；控制权通过固定路径
`https://<publisher>/.well-known/actr/publisher.json` 的 v1 descriptor 证明。包的长期
真实性由 publisher key、signature 与 digest 决定，下载 origin/registry 不是信任根。

Actor ID 的字符串形式为：

`<serial-hex>@<realm-id>/<publisher-or-linked-namespace>/<program>@<version>`

可下载 package 的 Publisher 必须是规范 DNS 域名；linked/local workload 可以使用
宿主管理的非 DNS namespace，但不能借此进入 Publisher Registry。

## Signer

规范入口：`/signer.v1.Signer/<Method>`（gRPC）。

方法包括 `GenerateSigningKey`、`Sign`、`GetVerifyingKey` 与 `HealthCheck`。认证使用
nonce-auth request message；`Sign` 的认证 payload 同时绑定 protocol、`key_id` 和待签
消息 SHA-256。不存在下载私钥的 API。生产环境必须配置 KEK，私钥明文落盘会导致配置
校验失败。

## Admin

- `POST /admin/api/auth/login`
- 其余 `/admin/api/**` 需要 `Authorization: Bearer <JWT>`

首版每实例只允许一个 Realm。在线 API 可以首次 provision、读取、更新和轮换 Realm
secret，但不提供硬删除或替换另一个 Realm；替换属于停机迁移/重建操作。

## Schema migration

运行时只自动初始化空数据库。旧的未版本化 SQLite 必须停机执行：

```bash
python3 actrix/scripts/migrate_schema_v1.py /path/to/actrix.db
```

脚本先创建备份，并拒绝多 Realm 或无法安全映射的旧 publisher 数据。

完整信任边界与发布门槛见
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md) 和
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
