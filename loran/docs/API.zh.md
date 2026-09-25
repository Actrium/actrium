# Loran Realm v1 API 边界

状态：本文只描述当前实现，不承诺旧 Issuer Signer/Publisher Registry HTTP API 兼容。一个实例
承载多个 Realm（Realm 名 `<realm_id>@<domain>`）；尚未实现的部分见
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
- Publisher 由 Publisher Profile 签名 key 持有方以 `register_publisher` 登记，Publisher
  管理（停用、撤销 key）和配置变更只允许系统管理员调用。其他域名的 Realm 由 Realm 签名
  key 持有方以 Realm 管理请求（`create_realm`、`publish_profile`、`reissue_admin_token`）创建、
  更新与退役，`POST` 到 Loran authority descriptor 中的 `realm-management` service link（Actr
  Realm Profile 第 8 节，见下文「Realm 管理请求」）；Realm Provider 域名下的 Realm 由系统管理员
  创建。作用域为单个 Realm 的 Realm 管理 token 可查看该 Realm 及其 Realm Profile、轮换其
  secret；管理连接 ACL、Realm membership 与状态尚未实现，见架构契约第 9 节。
- Issuer Signer 是内部信任平面；规范入口是同一 HTTP/2 监听器上的 gRPC，
  不是公开 REST 密钥下载服务。

## Realm discovery

Loran 不再提供 `/.well-known/actr/realm.json`：它与 `actr/protocols/authority-discovery`
「`/.well-known/actr` 下不定义子路径」冲突，也无法表达多个 Realm。Realm 的发现走该域名的
authority descriptor（`https://<domain>/.well-known/actr` 中的 `realm_signing_keys` 成员与
`realms` relation，Actr Authority Discovery §4.3–4.4），Realm Profile 位于
`<realms href>/<realm_id>`，由 Realm 签名 key 签名。Loran 仅在 `realms` relation 指向本 Realm
Provider 时提供 Realm Profile。

`[authority] enabled = true` 时 Loran 只为自己的 Realm Provider 域名提供一份
`/.well-known/actr`（支持 `GET`/`HEAD`，带 `Access-Control-Allow-Origin: *` 与
`Cache-Control`）。启用 Issuer 且有 Signer 时，descriptor 额外公布：

- `realms` relation：`https://<provider>/actr/realms/<provider>`（protocol `actr-realm-profile/1`）；
- `realm-management` relation：`https://<provider>/actr/realm-management`（protocol
  `actr-realm-management/1`）；
- `realm_signing_keys`：Signer 中名为 `realm-signing:<provider>` 的 Ed25519 key（启动时缺失即
  生成、不过期），`sequence = 1`，由该 key 对去掉 `signature` 的成员按 JCS 自签名；descriptor 的
  key id 与之相同。

Realm 签名 key 尚未就绪时 descriptor 返回 `503`。`[authority] links` 配置 `links` 成员。

`GET /actr/realms/{domain}/{realm_id}`（`HEAD` 同）原样返回 Realm `<realm_id>@<domain>` 已接受的
Realm Profile（`application/json`，带 `Access-Control-Allow-Origin: *` 与
`Cache-Control: public, max-age=300`）。`active` 与 `retired` Realm 都提供；未知、`pending`
或尚无 Realm Profile 的 Realm 返回 `404`。Realm Provider 域名下的 Realm 由 Loran 用
`realm-signing:<provider>` 签发 Realm Profile：`issuer.url = <origin>/issuer`，`issuer.keys`
为该 Realm 当前列出的 issuer key（`id` 为 Signer 数字 key id 的十进制，即 credential `key_id`；
首把是 Signer 中的 `issuer:<realm>`，轮换见下文「Admin」），`provider.signaling` 的
`control_plane_id = <provider>/v1`、`ticket_audience = actr.signaling`，TURN 条目的
`credential_audience = actr.turn`，`policy.membership = invite-only`；每份都先按 Actr Host
的规则（`RealmProfile::verify` 与 `check_successor`）验证再保存。

其他域名的 Realm 若有已接受的 Realm Profile，Loran 每 `[realm] profile_refresh_secs`（默认
3600，取值限定在 60–86400）刷新一次：经逐跳 SSRF 检查的抓取器（仅 HTTPS、最多 5 次重定向、
拒绝非公网地址并钉住解析结果、64 KiB、结果缓存 5 分钟、含失败）取该域名的 authority
descriptor，按 key pin 接受 `realm_signing_keys`（Actr Authority Discovery §4.4：成员自签名，
首次接受即 pin；此后同 id 不换公钥、`retired` 不复活、已移除不复现、`sequence` 不回退，新 key
只经 `sequence` 更高的有效轮换项引入；签名 key 链接不到已 pin 的 key 即身份变化，拒绝），再从其 `realms`
位置取更高 sequence 的 Realm Profile；退役 Realm Profile 触发退役事务。已接受 Realm Profile
的签名 key 被移除时丢弃该基线，改用由仍列出的 key 签名的 Realm Profile，取不到则停用该 Realm。

## Realm 管理请求

`POST /actr/realm-management`（Loran 作为 Realm Provider 时提供）接受 Actr 管理信封：JSON 对象，
含 `op`、`realm`、`provider_id`、`nonce`、`issued_at`、`expires_at`（RFC 3339）、可选
`requester` 与各操作的字段，加上 `signature`（`{"key_id", "algorithm": "Ed25519", "value"}`，
对去掉 `signature` 的请求按 JCS 规范化后的 detached 签名，与 Realm Profile 相同）：

- `create_realm`：带 `package`（`href` 须为 HTTPS，`sha256` 为 64 位小写十六进制）。建为
  `pending`，在 Signer 中生成该 Realm 的首把 issuer key `issuer:<realm>`，响应
  `{"realm", "status": "pending", "pending_expires_at", "profile"}`，`profile` 是 `sequence = 1`、
  指明本 Realm Provider 的 Issuer、issuer key 与端点的未签名首份 Realm Profile。以
  `(realm, requester)` 幂等（`requester` 缺省为空）：同一请求方重试 `pending` Realm 再次返回
  `profile`，对 `active` Realm 只返回 `{"realm", "status": "active"}`；其他请求方或已退役 Realm
  返回 `409`。
- `publish_profile`：`profile` 为持有方签名的 Realm Profile。须能以该域名当前的
  `realm_signing_keys` 验证、`provider.id` 为本 Realm Provider；`active` Realm Profile 的
  `issuer` 与 `provider` 须与 Loran 生成的完全一致。首次提交激活 `pending` Realm，响应一次性带
  `admin_token`；之后只接受通过 `check_successor` 的更高 sequence（重复提交返回 `409`），不再
  返回 token；`status = retired` 的 Realm Profile 触发退役事务。
- `reissue_admin_token`：`active` Realm 换发 Realm 管理 token（`{"realm", "admin_token"}`），
  旧 token 立即失效。
- `draft_profile`：`active` 且有已接受 Realm Profile 的 Realm，返回 `{"realm", "profile"}`：
  下一份未签名 Realm Profile（`sequence` 为已接受值加一，`display_name`、`package`、`policy`
  取自已接受的 Realm Profile，`issuer` 与 `provider` 为 Loran 当前值，含轮换中的 issuer key）。
  不改变 Realm；持有方签名后以 `publish_profile` 提交。
- `register_publisher`（Actr Authority Discovery §4.5）：以 `publisher`（规范化 Publisher 域名）
  取代 `realm`，由该域名 Publisher Profile 中 `active` 的 Publisher Profile 签名 key 签名。见下文
  「Publisher Registry」。

校验顺序：请求格式、`provider_id` 等于本 Realm Provider ID、有效期不超过 5 分钟且包含当前时间、
`nonce` 为 1–128 字符（`register_publisher` 之后的步骤见「Publisher Registry」）、Realm 名规范且不在 Realm Provider 域名（及其子域名）下（否则 `403`，
这些 Realm 由运营者创建）；然后经 Realm discovery 中的抓取器取
`https://<domain>/.well-known/actr`，按 key pin 接受其 `realm_signing_keys`，要求签名 key 为其中
`active` key；最后才记录 `nonce`（有效期内重复即 `400`）。请求按来源地址限流，抓取按目标主机
限流并缓存 5 分钟（含失败）。`create_realm` 受 `[realm]` 配额约束：`max_realms`、
`max_realms_per_domain`、`max_pending_per_domain`（超出返回 `429`）；`pending` Realm 在
`pending_ttl_secs` 后清除，名字可再用。

错误以 `{"error": "…"}` 返回：`400` 请求无效，`403` key 或签名不被接受，`404` Realm 不存在，
`409` 冲突（幂等键、回滚、已退役），`429` 配额，`503` 取不到 descriptor 或 Signer 不可用。
响应带 `Cache-Control: no-store`。持有方工具：`actr realm keygen | create-request |
sign-profile | retire | reissue-request | draft-request`。

## Issuer

路由前缀：`/issuer`。

注册、续签与 signing-key 查询使用 protobuf body。Realm secret 是管理员或受信 Actr Host 的
注册能力，不得下发给 Rill Guest。`/issuer/register` 与 `/issuer/reissue` 先校验 Realm secret：
Realm 不存在、处于 pending/停用/退役，或 secret 缺失、错误，一律返回 `403`「Realm secret not
accepted」，不透露 Realm 状态；secret 通过后才报告 Realm 不可用的原因。签发的 Actor
credential claims 固定包含：

- `protocol_version = 1`
- `issuer`：Realm 名，例如 `k7m2q9xh@acme.com`
- `provider`：Realm Provider ID，即 `[authority] uri` 的规范化 DNS 域名，例如
  `provider.example`
- `audience = actr.signaling`
- `realm`（Realm 名，与 `issuer` 相同）、完整 ActorId 与过期时间
- `actor_type`：注册时声明并经验证的 Actor 类型 `publisher/program@version`

Actor credential 由该 Realm 的当前 issuer key（Realm Profile `issuer.keys` 中 `id` 为其十进制的
key）签名，`key_id` 与注册响应的 `signing_key_id` 为该 key 的 Signer key id；
Signaling 与 TURN 按 `(realm, key_id)` 查验证 key，轮换后退役 key 的缓存项在其可移除时刻失效。Issuer 没有全局签名 key；`GET /issuer/health` 报告
数据库与 Signer 是否可用（`database`、`signer_service`）以及已加载的 Realm issuer key 数
（`realm_keys`）。

TURN credential 的 password 为 `base64(HMAC-SHA1(k, username))`，`k` 为该 Realm 的 TURN
secret：`HKDF-SHA256(turn_secret, salt = "actr.turn", info = "<realm>|<provider>")`。TURN 拒绝
不可用（pending、停用、过期或已退役）Realm 的 credential，缓存命中也不例外：Realm 状态变更即时
生效，过期最迟 10 秒后生效；该 Realm 已建立的 allocation 同时被关闭（按 TURN username 中的 Realm 名）。

`/issuer/renew` 与 `/issuer/reissue` 带客户端生成的 16 字节 `attempt_id`（UUID），网络失败时
客户端用完全相同的请求体重试。同一 attempt 的第一个请求占用它，身份状态变化（renewal token
轮换；reissue 消耗持有证明与 publisher nonce）与响应在同一事务提交，响应加密后存于
`issuer_control_attempt`。之后用同一 `attempt_id`：

- 请求体相同：返回首次的响应原文（成功响应保留到 credential 或 renewal token 到期，拒绝保留
  5 分钟）；重放成功响应前仍重查 Actor 撤销与 Realm、包、publisher 状态，已不允许则返回该错误；
- 请求体不同：`409`；
- 超过响应保留期：`410`，客户端应以新 `attempt_id` 重来；
- 另一个请求仍在处理：`425`，带 `Retry-After`（秒）；
- `attempt_id` 不是 16 字节：`400`。

这些以 HTTP 状态返回；`/reissue` 的其他结果仍放在 `RegisterResponse` body 中、HTTP 200，只有提交时
才发现的失败例外：另一个 attempt 先消耗了同一持有证明（`422`）或 publisher nonce（`403`），或存储
出错（`500`）；这时 attempt 被释放，同一 attempt 可以重试。
`/reissue` 先校验 Realm secret 再看 `attempt_id`。一个持有证明只能成功 reissue 一次：此后用它
发起的新 attempt 返回 `422`（持有证明不成立），丢失响应只能靠同一 attempt 重试取回。

`/renew` 先只读校验 renewal token（无效或过期返回 `401`），通过后才报告 Realm 不可用的原因，
未持有有效 token 的调用方探知不到 Realm 是否停用或退役。renewal token 的后继已存在（它已被
`/renew` 轮换或被 `/reissue` 消耗）时，`/renew` 只在后继生成后 5 分钟内接受它，且只返回这个
后继及其到期时间；此后返回 `401`。`/renew` 与 `/reissue` 在提交事务内重查 Actor 撤销、Realm
状态、包撤销与 publisher 停用，首次检查之后落地的变更同样阻止签发：此时 attempt 被释放，
按该错误返回（`/renew` 以 HTTP 状态，`/reissue` 在 body 中）。

注册请求的 ACL 规则只作用于本 Realm：`source_realm` 可以不填、为 `any_realm`（v1 只匹配本
Realm）或本 Realm 名；写成其他 Realm 名或不合法的 Realm 名时注册返回 `400`。

## Signaling

`GET /signaling/ws` 升级到 WebSocket。身份只从原生 upgrade headers 或上述浏览器
子协议项读取；连接身份、Actor credential 中的 Actor 与每条 envelope 的 source 必须完全一致。
单帧最大 1 MiB，并执行连接/消息速率限制。连接绑定其身份所属的 Actor 与 Realm：envelope 的
`source` 不是该 Actor、或 relay 的 `target` 不属于该 Realm 时返回 `403`。Actor Registry 与 presence 按
Realm 分区，发现、路由候选、ServiceSpec 查询与上线通知只涉及同 Realm 的 Actor；
`GetSigningKey` 只返回本 Realm 的 issuer key。Realm 不可用（不存在、pending、停用、过期或已
退役）时 upgrade 返回 `403`；Realm 退役时其在线连接以 close code 1008（`realm retired`）关闭。
按 Realm 分别计的速率限制尚未实现，见架构契约第 9 节。

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

Publisher ID 是规范化 DNS 域名。Publisher 以 `register_publisher` 登记（见「Realm 管理请求」）：
Loran 经同一抓取器取 `https://<publisher>/.well-known/actr`，读取其 `publisher` 成员（Publisher
Profile，`docs/product/actrium-v1` 12 §2），要求 `domain` 等于该域名、在
`profile_issued_at`–`profile_expires_at` 之内、由 `profile_signing_keys` 中 `active` 的 key 按 JCS
签名；请求信封须由其中 `active` 的 key 签名；然后记录 `nonce`。首次接受的 Publisher Profile 固定
（pin）其 `profile_signing_keys`；此后按与 `realm_signing_keys` 相同的 §4.4 规则接受（跟随
`rotations` 轮换链，身份变化拒绝），相同 Publisher Profile 重复登记不变。
Publisher Profile 的 `keys` 至多 8 项，`id` 须按上文规则由公钥算出；须有 1 至 2 把 `active` 包签名
key（`not_after` 为其发布期限，缺省一年；登记时须已过 `not_before`、未到 `not_after`）。两把
`active` key 都可发布与证明持有（轮换重叠期）；原当前 key 只要仍 `active` 就保持为当前 key，否则
另一把成为当前 key。不再 `active` 的 key 归档为 retired，仍可验证旧包；Publisher Profile 不再列出
的 retired key 转为 revoked，其签过的包不再通过验证；用过的 key 不能再成为 `active`。响应 `{"publisher", "status", "sequence"}`；`400` 请求无效，
`403` Publisher Profile 或签名不被接受，`409` Publisher 已停用或并发修改，`429` 已登记的
Publisher 数（不论状态）达到 `[realm] max_publishers`（默认 1000；已登记者更新 Publisher Profile
不受限），`503` 取不到 descriptor。
公开文档中不放 challenge，也没有 `/.well-known/actr/publisher.json`。

管理路由前缀：`/admin/api/publishers`，受 Admin JWT 保护：列表、状态、停用/恢复、删除、key
历史与撤销（含当前 key 的紧急撤销，之后须登记新的 Publisher Profile 才能再发布）、包列表与撤销；
没有由管理员登记 Publisher 的入口。包的长期真实性由 publisher key、signature 与 digest 决定，下载 origin/包 registry
不是信任根。

ActorId 的字符串形式为：

`<serial_hex>.<realm_id>@<domain>`

例如 `2af01.k7m2q9xh@acme.com`。程序类型 `publisher/program@version` 不在 ActorId 中，
由注册与 Actor credential 携带。

可下载 package 的 publisher 必须是规范 DNS 域名；linked/local workload 可以使用
Actr Host 管理的非 DNS publisher 值，但不能借此进入 Publisher Registry。

## Issuer Signer

规范入口：`/signer.v1.Signer/<Method>`（gRPC）。

方法包括 `Sign`、`GetVerifyingKey`、`EnsureNamedKey`、`RetireNamedKey`（退役命名 key，此后不再签名）与 `HealthCheck`。认证使用
nonce-auth request message；`Sign` 的认证 payload 同时绑定 protocol、`key_id` 和待签
消息 SHA-256。不存在下载私钥的 API。生产环境必须配置 KEK，私钥明文落盘会导致配置
校验失败。

## Admin

- `POST /admin/api/auth/login`
- 其余 `/admin/api/**` 需要 `Authorization: Bearer <JWT>`；Realm 管理 token（`rat_…`）只能用于
  自己 Realm 的 `GET /admin/api/realms/{realm}`、`GET …/profile` 与 `POST …/secret/rotate`，
  其他任何路径（含其他 Realm）一律 `404`；未知 token 为 `401`。

实例运营者可创建多个 Realm，数量受 `[realm] max_realms` 限制（不计已退役的）。
Realm 以 Realm 名寻址，`@` 编码为 `%40`：

- `GET /admin/api/realms`、`POST /admin/api/realms`：列出；创建（body
  `{"realm": "k7m2q9xh@provider.example", "enabled": true, "expires_at": 0, "package": {"href":
  "https://…", "sha256": "<64 hex>"}}`，可选 `display_name`，响应一次性返回 Realm secret）。Loran
  是 Realm Provider（启用 authority 与带 Signer 的 Issuer）时只创建其 Realm Provider 域名下的
  Realm：先建为 `pending`（`pending_ttl_secs` 后过期），Loran 签发首份 Realm Profile 后转为
  `active`，签发失败则丢弃该 `pending` 行，响应另一次性返回 `realm_admin_token`；其他域名的
  Realm 由持有方以 Realm 管理请求创建。Loran 不是 Realm Provider 时创建没有 Realm Profile 的
  `active` Realm，不接受 `package` 与 `display_name`；
- `POST /admin/api/realms/{realm}/admin-token/reissue`：为 `active` Realm 换发 Realm 管理 token
  （`realm_admin_token`），旧 token 立即失效；
- `GET /admin/api/realms/{realm}/profile`：已接受的 Realm Profile（原样 JSON），没有则 `404`；
- `GET`/`PUT /admin/api/realms/{realm}`：读取；`PUT` 只收 `enabled` 与 `expires_at`，
  `enabled = false` 即停用、`true` 即恢复，Realm 名不可修改；
- `POST /admin/api/realms/{realm}/secret/rotate`：轮换 Realm secret；
- `POST /admin/api/realms/{realm}/issuer-key/rotate`：开始轮换该 `active` Realm 的 issuer key
  （Realm Profile §6.1），响应 `{"success", "realm", "key_id"}`。新 key 先在 Signer 中生成并
  列入 Realm Profile（Realm Provider 域名下的 Realm 由 Loran 立即签新 Realm Profile；其他域名
  的 Realm 由持有方经 `draft_profile` 取得并发布），列出它的 Realm Profile 被接受满 24 小时后
  Loran 改用它签发，旧 key 以 `retired` 列出直到最后一个 credential 过期再过 5 分钟，然后移除并
  在 Signer 中退役。并发轮换只有一个能列出新 key，另一个（以及与 Realm 退役竞态的轮换）把刚在
  Signer 中生成的 key 当即退役（此时 Signer 不可用则只记日志，key 留在 Signer 中但不被列出、不签发）。Realm 不是 `active`、已有待切换的 key 或仍列着 3 把 key 时返回 `400`，
  Loran 不是 Realm Provider 时返回 `503`。Realm Provider 域名下的 Realm 每 90 天自动轮换；
- `POST /admin/api/realms/{realm}/retire`：退役。单一事务中删除该 Realm 的连接 ACL、待连接
  注册、renewal token 与 `/renew`、`/reissue` 的 attempt 记录，保留已签发身份登记与撤销表；`realm` 行以 `retired` 状态永久保留，
  作为 Realm Provider tombstone，Realm 名不复用。没有硬删除。Realm Provider 域名下的 Realm 由
  Loran 签发退役 Realm Profile 并在同一事务中退役；其他域名的 Realm 若已有 key 持有方签名的
  Realm Profile，运营者不能退役（只能停用），退役只由持有方发布的退役 Realm Profile 触发。
  事务提交后，Signaling 删除该 Realm 的 Actor Registry 记录（内存与缓存）与 presence 订阅并
  关闭其在线连接，Issuer 在 Signer 中退役该 Realm 的全部 issuer key（此后不再签名）；这两步幂等，进程
  重启时对已退役 Realm 重做，Signer 暂不可用时每分钟重试。

gRPC `NodeAdminService` 对应 `CreateRealm`/`GetRealm`/`UpdateRealm`/`ListRealms`/
`RetireRealm`，同样按 Realm 名寻址；Loran 是 Realm Provider 时 `CreateRealm` 不能新建 Realm
（没有 `package`），只能更新已有 Realm。

尚未实现：Realm Provider 域名下的 Realm 由 Realm Provider 账号（OAuth 2.0 + PKCE）以
bearer 请求自助创建（目前由运营者代办）。

## Schema migration

当前 schema 为 v8（`PRAGMA user_version = 8`）：v3 在 v2 上新增 Issuer 幂等日志表
`issuer_control_attempt`；v4 以 Realm 名为键（`realm.name` 为主键，其余表以 `realm` 列引用，
身份登记按 `(realm, serial_number)`）；v5 去掉单 Realm 约束，`realm.status` 改为
`pending | active | retired`，并新增 `realm_domain`、`realm_profile`、`realm_mgmt_nonce`；v6 删除
`publisher_challenge`，`publisher.descriptor_json` 改为 `profile_json`（已接受的 Publisher Profile）；v7 删除
`issuer_reissue_response_cache` 与 `issuer_renewal_token.delivered_at`（由 `issuer_control_attempt` 取代）；
v8 让 `realm_domain` 保存 key pins（`pins_json`），新增 `realm_issuer_key`（每个 Realm 的 issuer
key 及轮换状态），`publisher` 新增第二把 `active` 包签名 key 与 `profile_pins`，删除未用的
`realm.issuer_key_id`。
运行时只自动初始化空数据库；其他版本拒绝启动。v2 之后的升级都不提供迁移工具，需停机后重建数据库。下面两个历史工具只覆盖到 v2（未版本化库先 v1，再 v2）：

```bash
python3 scripts/loran/migrate_schema_v1.py /path/to/actrix.db   # 未版本化 → v1
python3 scripts/loran/migrate_schema_v2.py /path/to/actrix.db   # v1 → v2
```

两个脚本都先创建备份、在单个排他事务中执行，并支持 `--dry-run`。v1 工具拒绝多 Realm 或
无法安全映射的旧 publisher 数据；v2 工具改名历史表/列/索引（`mfr*` → `publisher*`、
`ais*` → `issuer*`、`manufacturer` → `publisher`），并把已存 Actor 类型字符串从
`publisher:name:version` 改写为 `publisher/program@version`。v2 之后没有自动迁移工具：
升级到多 Realm schema 需要停机重建数据库，旧 Realm 须由实例运营者显式映射到 Realm 名，
并由对应 Realm 域名的 Realm 签名 key 签署 Realm Profile 后才能激活。

完整信任边界与发布门槛见
[`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md) 和
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
