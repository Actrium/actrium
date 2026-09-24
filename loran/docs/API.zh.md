# Loran Realm v1 API 边界

状态：本文只描述当前实现，不承诺旧 Issuer Signer/Publisher Registry HTTP API 兼容。当前实现仍是一个实例
一个 Realm；目标契约是一个实例承载多个 Realm（Realm 名 `<realm_id>@<domain>`），与本文
不同的部分以「目标契约」标注，完整差异见
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md) 第 9 节。历史名称见
[术语表](../../docs/GLOSSARY.zh.md)「历史名称对照」。

## 传输与认证规则

- 生产实例只接受 HTTPS/WSS；端口可以配置，但 endpoint/audience 必须进入凭证作用域。
- 凭证不得放在 URL、query string、普通访问日志或错误文本中。
- 原生 Signaling WebSocket 使用 `x-actr-id` 与
  `Authorization: ActrCredential <base64url-protobuf>` upgrade headers。浏览器不能设置这
  两个 header，因此改用 `actr.v1`、`actr.id.<base64url>` 和
  `actr.credential.<base64url-protobuf>` 子协议项；服务器验证全部身份后只协商返回
  `actr.v1`，避免把凭证暴露为页面可读的已选子协议。
- 当前实现：Realm 管理、Publisher 管理和配置变更只允许系统管理员调用。目标契约：
  Realm 生命周期还可由 Realm 管理请求管理，单个 Realm 的 secret、连接 ACL、Realm membership
  与状态可由作用域为该 Realm 的 Realm 管理 token 管理。`create_realm`、`publish_profile`、
  `reissue_admin_token` 以 `POST` 发往 Loran authority descriptor 中的 `realm-management`
  service link（Actr Realm Profile 第 8 节）。
- Issuer Signer 是内部信任平面；规范入口是同一 HTTP/2 监听器上的 gRPC，
  不是公开 REST 密钥下载服务。

## Realm discovery

`GET /.well-known/actr/realm.json`

当前实现：成功时返回当前实例唯一 Active Realm 的 v1 descriptor，包括 issuer、Realm Provider、
Signaling/TURN audience 与 endpoint。没有且仅有一个可用 Realm 时返回 `503`，不猜测。

目标契约：移除该路径。它与 `actr/protocols/authority-discovery`「`/.well-known/actr`
下不定义子路径」冲突，也无法表达多个 Realm。Realm 的发现改为该域名的 authority
descriptor（`https://<domain>/.well-known/actr` 中的 `realm_signing_keys` 成员与
`realms` relation，Actr Authority Discovery §4.3–4.4），Realm Profile 位于
`<realms href>/<realm_id>`，由 Realm 签名 key 签名。Loran 仅在 `realms` relation 指向
本 Realm Provider 时提供 Realm Profile。

## Issuer

路由前缀：`/issuer`。

注册、续签与 signing-key 查询使用 protobuf body。Realm secret 是管理员或受信 Actr Host 的
注册能力，不得下发给 Rill Guest。签发的 Actor credential claims 固定包含：

- `protocol_version = 1`
- `issuer`：Realm 名，例如 `k7m2q9xh@acme.com`
- `provider`：当前实现为固定常量；目标契约为 Realm Provider 的规范化 DNS authority，例如
  `provider.example`
- `audience = actr.signaling`
- `realm`（Realm 名，与 `issuer` 相同）、完整 ActorId 与过期时间
- `actor_type`：注册时声明并经验证的 Actor 类型 `publisher/program@version`

## Signaling

`GET /signaling/ws` 升级到 WebSocket。身份只从原生 upgrade headers 或上述浏览器
子协议项读取；连接身份、Actor credential 中的 Actor 与每条 envelope 的 source 必须完全一致。
单帧最大 1 MiB，并执行连接/消息速率限制。目标契约下，每条 envelope 还须属于连接
绑定的 Realm，发现只返回同 Realm 的 Actor，速率限制同时按 Realm 计。

## Publisher Registry

公开路由前缀：`/registry`。

- `GET /publishers/{name}/verifying-key`
- `POST /packages/nonce`
- `POST /packages/publish`

v1 JSON 统一使用 publisher 术语：

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

其他旧 JSON/query 名称不属于 v1 契约；内部 SQLite 列名和 crate 名不构成 API。Publisher
key id 形如 `actr-pub-<16 hex>`（公钥 SHA-256 的前 16 个十六进制字符）。

管理路由前缀：`/admin/api/publishers`，受 Admin JWT 保护。Issuer key 管理为
`/admin/api/services/issuer/keys`（及 `/rotate`）。Publisher ID 是规范化 HTTPS
域名。当前实现通过固定路径 `https://<publisher>/.well-known/actr/publisher.json` 的
descriptor 证明控制权；目标契约中该路径移除（与 authority-discovery「`/.well-known/actr`
下不定义子路径」冲突），改为读取 `https://<publisher>/.well-known/actr` authority
descriptor 中带签名的可选成员 `publisher`（Actr Authority Discovery §4.5），并以 Publisher
Profile 签名 key 签名的 `register_publisher` 请求（Actr 管理信封）登记，公开文档中不再放 challenge；当前
descriptor 结构为预发布格式，与目标不兼容。包的长期真实性由 publisher key、signature
与 digest 决定，下载 origin/包 registry 不是信任根。

ActorId 的字符串形式为：

`<serial_hex>.<realm_id>@<domain>`

例如 `2af01.k7m2q9xh@acme.com`。程序类型 `publisher/program@version` 不在 ActorId 中，
由注册与 Actor credential 携带。

可下载 package 的 publisher 必须是规范 DNS 域名；linked/local workload 可以使用
Actr Host 管理的非 DNS publisher 值，但不能借此进入 Publisher Registry。

## Issuer Signer

规范入口：`/signer.v1.Signer/<Method>`（gRPC）。

方法包括 `GenerateSigningKey`、`Sign`、`GetVerifyingKey` 与 `HealthCheck`。认证使用
nonce-auth request message；`Sign` 的认证 payload 同时绑定 protocol、`key_id` 和待签
消息 SHA-256。不存在下载私钥的 API。生产环境必须配置 KEK，私钥明文落盘会导致配置
校验失败。

## Admin

- `POST /admin/api/auth/login`
- 其余 `/admin/api/**` 需要 `Authorization: Bearer <JWT>`

当前实现：每实例只允许一个 Realm。在线 API 可以首次 provision、读取、更新和轮换
Realm secret，但不提供硬删除或替换另一个 Realm；替换属于停机迁移/重建操作。Realm 以 Realm
名寻址：`/admin/api/realms/{realm}`（`@` 编码为 `%40`），创建 body 为
`{"realm": "k7m2q9xh@acme.com", "enabled": true, "expires_at": 0}`。

目标契约：每实例承载多个 Realm。创建需经 Realm 签名 key 持有方授权，并经 Realm Profile
验证后才从 `pending` 转为 `active`；退役由持有方发布退役 Realm Profile 触发，Loran 在单一
事务中撤销该 Realm 的状态并写入以 Realm 名为键的 Realm Provider tombstone，Realm 名不复用。

## Schema migration

当前 schema 为 v4（`PRAGMA user_version = 4`）：v3 在 v2 上新增 Issuer 幂等日志表
`issuer_control_attempt`；v4 以 Realm 名为键（`realm.name` 为主键，其余表以 `realm` 列引用，
身份登记按 `(realm, serial_number)`）。运行时只自动初始化空数据库；其他版本拒绝启动。v2 → v3
与 v3 → v4 不提供迁移工具，需停机后重建数据库。下面两个历史工具只覆盖到 v2（未版本化库先 v1，再 v2）：

```bash
python3 scripts/loran/migrate_schema_v1.py /path/to/actrix.db   # 未版本化 → v1
python3 scripts/loran/migrate_schema_v2.py /path/to/actrix.db   # v1 → v2
```

两个脚本都先创建备份、在单个排他事务中执行，并支持 `--dry-run`。v1 工具拒绝多 Realm 或
无法安全映射的旧 publisher 数据；v2 工具改名历史表/列/索引（`mfr*` → `publisher*`、
`ais*` → `issuer*`、`manufacturer` → `publisher`），并把已存 Actor 类型字符串从
`publisher:name:version` 改写为 `publisher/program@version`。目标契约下，旧
Realm 须由实例运营者显式映射到 Realm 名，并由对应 Realm 域名的 Realm 签名 key 签署 Realm
Profile 后才能激活。

完整信任边界与发布门槛见
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md) 和
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
