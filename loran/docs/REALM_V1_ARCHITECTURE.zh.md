# Loran Realm v1 架构契约

状态：v1 架构契约，与当前代码一致。一个 Loran 实例作为共享 Realm Provider 承载多个
Realm；首版不承诺 HA 或跨 Realm federation。尚未落地的部分集中列在第 9 节。
历史名称（旧产品名、旧服务名）集中见[术语表](../../docs/GLOSSARY.zh.md)「历史名称对照」。

## 1. Realm 模型

### 1.1 Realm 是实例概念

Realm 对应一次应用实例部署，而不是一个应用（程序），也不是组织级成员域。同一个程序
`notes.example/team-notes` 被 A 组织部署一次、被 B 组织部署一次，就是两个 Realm。
在 Actrium 中，新建一个实例就同时创建一个新 Realm；该实例的 Service、Realm 内的其他
Actor 以及 Attach 到它的 Front 都在这个 Realm 内。Realm membership 与 Realm 客户端准入、ActorId、Actor credential 签发与续期、连接 ACL、撤销与审计
都属于该 Realm。

由此 Realm 的数量按部署次数增长，远多于应用数量，也远多于基础设施节点数量。Realm
必须是轻量的逻辑记录：创建一个 Realm 不启动新进程、不占用新端口、不需要新数据库。

### 1.2 Realm 名

Realm 以 Realm 域名命名；没有域名就不分配 Realm（本机 Realm 除外）。域名在现实中已经具备全局唯一性，且归属可验证；Realm 只使用这两点，
不要求域名指向 Loran，也不要求域名承载任何基础设施。

```text
Realm 名 = <realm_id>@<domain>        例：k7m2q9xh@acme.com、crm@acme.com
```

- `domain` 是规范化 DNS 域名：小写 IDNA A-label，无 scheme、端口、userinfo 或 path，
  与 Publisher ID 的规范化规则相同。它与不含端口的 Actr authority 同名：
  `k7m2q9xh@acme.com` 的域名就是 authority `actr://acme.com`。
- 本机 Realm 使用伪域名 `<installation-id>.actr.alt`（RFC 9476 `.alt`，永不经 DNS
  解析）。`installation-id` 是安装时随机生成的 128 位值（base32 小写 26 字符）。本机
  Realm 由 Actrium 内嵌实现，只有本地记录，没有 Realm Provider、Realm Profile 与 Realm 管理请求；Loran 只
  承载 DNS 域名下的 Realm。
- RealmId（`realm_id`）使用 DNS label 的字符规则：`[a-z0-9-]`，1..63 字符，不以 `-` 开头或结尾；在域名内
  唯一，创建后不可修改；默认由客户端随机生成（base32 小写短串，不含程序名），可读名称
  放在 Realm Profile 的 `display_name`。
- 已退役 Realm 的名字永不复用：域名所有者不得复用，Realm Provider 在本地以 Realm
  Provider tombstone 强制，避免旧 Actor credential、ActorId 或缓存被新 Realm 继承。

一个 Realm 域名下可以有多个 Realm。

### 1.3 Realm 与 Publisher 的区别

两者都使用域名，但角色不同：

| | Publisher | Realm |
| --- | --- | --- |
| 命名对象 | 程序（谁发布了这个包） | 部署（谁运营这个实例） |
| 标识 | `publisher` 域名 | Realm 名 `<realm_id>@<domain>` |
| 长期信任根 | Publisher Profile 签名 key | Realm 签名 key |

同一个包可以运行在 `notes@acme.com`，也可以运行在 `notes@foo.org`。

### 1.4 Realm 与 Realm Provider 的区别

Realm 是逻辑信任域；Realm Provider 是运营中的共享基础设施，Loran 是其参考实现。一个 Loran
实例承载来自多个 Realm 域名的多个 Realm，共用同一进程、同一 SQLite、同一 signaling 服务、同一
STUN/TURN 端口池和同一 Issuer Signer（每个 Realm 一把 Realm issuer key）。Realm 在创建时
绑定一个 Realm Provider，此后不可更换：Realm Profile 的 `provider.id` 在 Realm 生命周期内
不变。换运营方即在新 Realm Provider 上新建实例并做应用层数据迁移，旧实例退役。

首版部署形态是单节点、SQLite、单一 Realm Provider；Issuer、Rendezvous、Relay、Publisher
Registry 和 Management 是逻辑平面，可以同进程部署。Issuer Signer 支持同进程开发模式，也支持通过现有
TLS/mTLS gRPC 客户端配置放入独立安全边界。

只需要独占部署的场景（如企业自建）可配置 `max_realms = 1`；默认不限制。

Workspace 不属于 Realm 基础设施的数据模型。一个 Service 至多绑定一个
Workspace，同一 Workspace 可按写入并发策略被多个 Service 使用；删除 Actor 不删除 Workspace；Workspace 密钥属于用户。这些规则由 Actrium
实现，Loran 不取得目录所有权。

## 2. 信任根

1. Realm 签名 key（Ed25519）是一个 Realm 域名下所有 Realm 的长期信任根，由 Realm 签名
   key 持有方（域名所有者，或受其委托代持的 Realm Provider）持有，
   声明在该域名 authority descriptor 的 `realm_signing_keys` 成员中，支持多把与轮换链。
   首次信任来自对 `domain` 的 HTTPS（Web PKI）访问，或企业预置的 key pin。
2. Realm Profile 由 Realm 签名 key 签名，把 Realm issuer key 与 Realm Provider 端点委派给
   某个 Realm Provider。Realm Profile 的有效性只取决于签名，不取决于从哪里取到。
3. Realm issuer key（Ed25519）签发 Actor credential，每个 Realm 一把，私钥留在
   Realm Provider 的 Issuer Signer；容忍窗口仅用于验签，绝不延长签名或 Actor credential
   生命周期。
4. Publisher ID 是规范化 DNS 域名。Publisher Profile 是 authority descriptor
   `https://<publisher>/.well-known/actr` 中带签名的可选成员 `publisher`（Actr Authority
   Discovery §4.5）。其中的 Ed25519 key 才是包的长期真实性根；包 registry/origin 只是
   HTTPS 分发位置，包最终按 digest 和 publisher signature 验证。

信任链：

```text
Web PKI 或企业 pin → authority descriptor → realm_signing_keys（Realm 签名 key）
  → Realm Profile → Realm issuer key → Actor credential（Actor 长期 key 另行绑定）
```

Realm Provider 只持有 Realm issuer key 私钥；只有受托成为 Realm 签名 key 持有方时（见 3.6）
才持有 Realm 签名 key。

管理权限分三类：

- 实例运营者：Loran 实例唯一的系统管理根，管理 Realm Provider 配置与全部 Realm 生命周期。
  Admin UI 配置只接受 Argon2id PHC hash，不接受明文密码；管理会话使用进程内 CSPRNG
  生成的 JWT key。
- Realm 签名 key 持有方：通过 Realm 管理请求创建 Realm、发布 Realm Profile（首次即激活，
  之后即更新或退役）与重签 Realm 管理 token，无需在 Loran 另开账号（Realm Provider 域名下的
  Realm 由个人以 Realm Provider 账号操作，见 3.6）。
- Realm 管理员：Realm 激活时签发的、作用域为单个 Realm 的 Realm 管理 token，通常交给实例
  所有者（交付与重签见 3.5）；只能看到并管理本 Realm：secret 轮换、连接 ACL、Realm
  membership 与 Realm 客户端撤销、状态变更。跨作用域访问一律拒绝，且不得通过错误信息泄露其他 Realm 是否存在。

Realm registration secret 是 Realm 管理员/可信 Actr Host 能力，不是 Guest workload
credential。Issuer 的注册入口同时要求 Realm secret 与 package/linked 身份证明；
Actrium/Rill 等 Actr Host 可以持有该能力，但不得把它暴露给用户编写的 Rill Guest。注册请求
携带的连接 ACL 因此是 Actr Host 管理行为；无 ACL 时默认拒绝发现/中继。

## 3. Realm 创建与激活

### 3.1 流程

创建 Realm 不需要按 Realm 或按域名做 challenge 验证；授权来自 Realm 签名 key 持有方：

1. 调用方（Realm 签名 key 持有方的签名工具、实例运营者或 supervisor）以 3.2 的 Realm 管理
   请求提交 `create_realm`，其中携带该实例要运行的包 `package`（下载地址与 SHA-256）；
2. Loran 获取该域名的 authority descriptor（见 5.1）并验证 `realm_signing_keys`，再用
   其中的 key 验证请求签名；通过后创建 `pending` Realm，在 Issuer Signer 中生成该 Realm
   的 Realm issuer key，在响应中返回待签 Realm Profile payload（`package`、Realm issuer
   key、端点、`provider.id`）；
3. 调用方用 Realm 签名 key 签名 Realm Profile，**总是**以 `publish_profile` 请求（同一格式，
   携带签名 Realm Profile）提交给 Loran；
4. Loran 验证 Realm Profile 签名，并确认其中的 Realm issuer key 与 Realm Provider 端点正是
   自己，Realm 转为 `active`。`publish_profile` 的响应即激活响应，一次性返回该 Realm 的
   Realm 管理 token（见 3.5）。若 Realm Profile 存放位置（`realms`）由本 Realm Provider
   提供，Loran 同时对外提供该 Realm Profile；否则由调用方发布到自己声明的位置。

Realm Provider 域名下的 Realm（3.6）由 Loran 自己签 Realm Profile，第 3 步在内部完成，
激活结果与 Realm 管理 token 在 `create_realm` 响应中直接返回给通过 Realm Provider 账号认证的用户。

幂等与冲突：创建以 `(realm, 请求方)` 为幂等键，`realm` 为完整 Realm 名。持有方自己签名时，
请求方是 Actr 管理信封中可选的 `requester` 字段（持有方签名服务填写的不透明成员标识，缺省
为空）；Realm Provider 域名下的 Realm 以 bearer token 请求，请求方是 Realm Provider 账号，Loran 忽略请求中的 `requester`。同一请求方对自己的 pending 或 active Realm 重试时返回其状态，不返回
Realm 管理 token；不同请求方命中同名 Realm，或任何请求命中 Realm Provider tombstone，一律
返回冲突。对已 active 的 Realm，`publish_profile` 只接受 sequence 更高的 Realm Profile，不再返回 Realm 管理 token；Realm 管理 token 丢失（包括
Realm Provider 域名下的 Realm 丢失 `create_realm` 响应）只能用 `reissue_admin_token` 取回。

只有 `active` Realm 才签发 Actor credential、接受 signaling 连接和分配 TURN。

`pending` Realm 有 TTL，过期清理。因为创建请求必须经 Realm 签名 key 持有方授权，他人无法
在本 Realm Provider 上抢注某个 Realm 域名下的 RealmId。

谁来授权：

- 创建必须经该 Realm 域名的 Realm 签名 key 持有方授权。持有方自己签名时，按自己的策略
  认证请求方，再以 Actr 管理信封代签创建请求与 Realm Profile；Loran 不替持有方判断谁有权
  创建；
- 域名所有者可以委托某个 Realm Provider 代持 Realm 签名 key（例如自建 Loran 时由自己的
  Realm Provider 持有）；没有自己域名的个人由 Realm Provider 在其域名下分配 Realm，此时
  该 Realm Provider 就是持有方（见 3.6）。本机 Realm 不经过 Loran。
- 授权路径与信任根显示按同一可判定规则：在规范化 A-label 上比较，Realm 域名等于
  Realm Provider ID 或以 `"." + Realm Provider ID` 结尾（按 label 边界）时，为「Realm
  Provider 域名下的 Realm」，走 Realm Provider 账号（见 3.6），客户端显示信任根为该 Realm
  Provider；其他情形一律走 Actr 管理信封，由持有方（域名所有者自己或受托代持的 Realm
  Provider）的签名服务签发，客户端显示信任根为 Realm 域名。受托代持对客户端不可见（见
  THREAT_MODEL 残余风险）。

Loran 获取 authority descriptor 与 Realm Profile 时遵循 authority-discovery 的重定向规则：
最多跟随 5 次，每一跳必须是 HTTPS，并始终用原 authority 验证返回的 `authority` 绑定。
不另设比客户端更严的「一律不跟随」规则，避免同一 descriptor 对客户端有效而 Realm
Provider 拒绝。作为服务端，Loran 在每一跳额外执行 SSRF 检查：拒绝内网/loopback/link-local
DNS 结果，把该跳请求钉住到预检查的公网地址（逐跳 DNS pinning），响应限长 64 KiB，
避免 SSRF 和 DNS rebinding。由于验签前就要外联，Loran 按来源与目标域名限流，并缓存
descriptor 获取结果（含失败）。

### 3.2 Realm 管理请求

v1 的 Realm 管理请求只有 `create_realm`、`publish_profile` 与 `reissue_admin_token`，共用
Actr 管理信封（Actr management envelope），只有 `op` 与操作字段不同（Publisher 注册复用
同一格式，见第 7 节）。请求以 HTTPS `POST` 发往 Loran 自身 authority descriptor
（`https://<provider_id>/.well-known/actr`）中的 `realm-management` service link（protocol
`actr-realm-management/1`）；对 Realm Provider 域名下的 Realm，请求携带同一 payload、不带签名，
改由 Realm Provider 账号的 OAuth bearer token 授权：

```json
{
  "op": "create_realm",
  "realm": "k7m2q9xh@acme.com",
  "package": { "href": "https://cdn.notes.example/team-notes-1.4.2.actr", "sha256": "<64 hex>" },
  "requester": "<可选，持有方的成员标识>",
  "provider_id": "provider.example",
  "nonce": "<random>",
  "issued_at": "<RFC 3339>",
  "expires_at": "<RFC 3339>"
}
```

- payload 按 RFC 8785 JCS 规范化，附 Realm 签名 key 的 detached Ed25519 签名与 `key_id`；
- `provider_id` 必须等于接收方的 Realm Provider ID，即 Realm Provider 的规范化 DNS
  authority，防止请求被重放到其他 Realm Provider；
- `expires_at - issued_at` 不超过 5 分钟，Loran 拒绝已过期或尚未生效的请求；
- Loran 在有效期内记录 `nonce`，拒绝重复使用；nonce 只防字节级重放，重试靠 3.1 的
  幂等键；
- 签名 key 必须是 `realm_signing_keys` 中的 `active` key。

### 3.3 持续校验

Loran 以同一节奏周期性重新获取 authority descriptor（含 `realm_signing_keys`）与
Realm Profile。若已接受 Realm Profile 的签名 key 已从 `keys` 中移除（吊销），丢弃该 Profile 及其
`sequence` 基线，改用最后一份由仍列出的 key 签名的 Realm Profile；没有则按从未接受处理并
重新获取。出现以下任一情况时，本地挂起该 Realm，不再签发新 Actor credential：

- Realm Profile 的 `provider.id` 不是本 Realm Provider，或其 Realm issuer key 不在本 Realm
  Provider 的 Issuer Signer 中；
- Realm Profile `status` 为 `retired`；
- 找不到由 `realm_signing_keys` 中仍列出的 key 签名的有效 Realm Profile；
- Realm Profile 或 `realm_signing_keys` 的 `sequence` 低于已接受的值（回滚）。

### 3.4 退役

退役只有一条路径：Realm 签名 key 持有方发布 `status = retired` 的新 Realm Profile（以
`publish_profile` 提交，并发布到 `realms` 位置；即退役
Realm Profile，永久有效；保持相同 `provider.id`，`sequence` 更高）。Loran 通过 3.3 的周期校验得知后，在单一事务中撤销
Actor credential、renewal token、presence、Actor Registry 记录与连接 ACL，退役 Realm issuer
key，并写入以 Realm 名为键的 Realm Provider tombstone。若绑定的 Realm Provider 已停服，
持有方把 `realms` href 改指自控位置，并在那里发布退役 Realm Profile。若持有方本身就是已
停服的 Realm Provider，其域名下的 Realm 随之失效、无法退役，只能在别处新建。

### 3.5 Realm 管理 token 的交付

Realm 管理 token 一次性返回给发起创建的调用方：一般在激活响应中返回给持有方的签名工具
（由其交给实例所有者）；Realm Provider 域名下的 Realm 在 `create_realm` 响应中返回给通过
Realm Provider 账号认证的用户；存入安全存储。Loran 只保存 Realm 管理 token 的哈希。
Realm 管理 token 丢失时重新签发，旧 Realm 管理 token 立即失效：持有方自己签名时，由其以 Actr 管理信封发出
`reissue_admin_token`；对 Realm Provider 域名下的 Realm，则由绑定的 Realm Provider 账号登录后
完成，换机或丢失客户端只需重新登录。

### 3.6 Realm Provider 持有 Realm 签名 key 的情形

没有自己域名的个人可由 Realm Provider 在其域名下分配 Realm，例如
`k7m2q9xh@realms.provider.example`。此时 Realm Provider 是该域名的所有者和 Realm 签名 key
持有方，Loran 直接服务该域名的 authority descriptor 与 Realm Profile，信任根即该 Realm
Provider；按 3.1 的可判定规则，客户端显示这一点。域名所有者也可以委托某个 Realm Provider
代持其 Realm 签名 key；此时 Loran 以持有方签名服务的身份签发 Actr 管理信封，客户端仍显示
信任根为 Realm 域名（受托代持不可见）。未经委托，Loran 不代持第三方域名的 Realm 签名 key。

对 Realm Provider 域名下的 Realm，个人以 Realm Provider 账号认证：OAuth 2.0 授权码 + PKCE
（RFC 6749、RFC 7636）。Realm Provider 自身 descriptor（`https://<Realm Provider ID>/.well-known/actr`）
中 `identity` service link 的 `href` 即 OAuth 授权服务器标识（issuer identifier，RFC 8414），元数据按 RFC 8414 发现；授权服务器接受
固定公共 client ID `actr-host`（无 secret，redirect 按 RFC 8252），Actr Host 请求 scope
`realm_management`。bearer 请求中的 `requester` 被忽略，请求方即账号。Loran 按 Realm Provider 账号计配额，并把 Realm 管理权绑定到该账号。
账号注册、登录方式与防滥用由 Realm Provider 决定，不属于本契约。Realm Provider 能关联
同一账号创建的全部实例（它本来就是签名方）；实例成员看不到账号，Realm 客户端 key 仍按
Realm 独立。

Realm 域名等于 Realm Provider ID 时（如 `k7m2q9xh@provider.example`），Realm Provider 的
`/.well-known/actr` 同时承载它自己的 `services`（signaling、identity、ice-config、
realm-management 等）与该域名的 `realm_signing_keys`、`realms`，是同一份 descriptor；Realm
域名是其子域名时（如 `realms.provider.example`），两者各有一份 descriptor，账号登录入口
始终取 Realm Provider 自身 descriptor 的 `identity`。

### 3.7 配额

按 Realm 域名与按 Realm 设配额：Realm 域名可创建的 Realm 数（Realm Provider 域名下的 Realm
再按 Realm Provider 账号计）、连接数、注册
速率、signaling 消息速率、TURN allocation 数与带宽。

## 4. 身份模型

ActorId 是 Actor 实例身份，不是用户、Workspace、程序或逻辑服务名，只含两部分：

```text
<serial_hex>.<realm_id>@<domain>        例：2af01.k7m2q9xh@acme.com
```

- `serial_hex`：该 Realm 的 Issuer 分配的 54-bit Snowflake serial，小写十六进制；
- `realm-name`：完整 Realm 名。

程序身份（Actor 类型）`publisher/program@version` 不在 ActorId 中。它由注册请求声明、
经 package/linked 身份证明验证后记入 Actor Registry，并写入 Actor credential；连接 ACL、
发现与版本校验都使用注册与 Actor credential 中的类型，不读取 ActorId。legacy 形式
`<serial_hex>@<realm_id>/<type>` 导入时必须显式补足 Realm 名并重写，不做推断。

Loran 不从 RealmId 推断任何语义，也不在对外接口中泄露同一域名下其他 Realm 是否存在。

每个客户端安装在每个 Realm 使用独立的 Realm 客户端 key，不跨 Realm 复用；Realm 客户端
注册按 Realm 分别进行，Loran 无法据 Realm 客户端 key 关联同一客户端加入的不同 Realm。
没有单独的 membership credential：客户端凭 Realm 邀请完成 Realm 客户端注册后，Loran 记录其
Realm 客户端 key，这就是 Realm membership。此后每次申请 Actor credential 都须以该 key 证明
持有，Issuer 只对已注册且未撤销的 Realm 客户端 key 签发；撤销某客户端即撤销其 Realm 客户端
key 注册，已签发的短期 Actor credential 到期失效（或按既有撤销机制提前失效）。renewal token
属于 Actor credential 续期，同样绑定该 Realm 客户端 key。

数据库直接以完整 Realm 名为键：它唯一、不可变、退役后不复用，不需要另一个数字。

Worker ID 为显式配置的 0..31 值，不再由 PID/hostname 的 5-bit hash 猜测。单节点
默认 0；未来并发 Issuer 必须由部署租约分配不同 worker ID。SQLite 的
`issued_actor_identity` 永久登记本实例已签发的 `(Realm, serial)`；进程重启或时钟
回拨导致 Snowflake 候选碰撞时只会继续分配，绝不覆盖原身份。跨 package version
升级视为新 Actor：持有方发布 `package` 已更新、sequence 更高的 Realm Profile，新的
Service 仍在同一 Realm 内，Realm 链接不变。

Service 可用 renewal possession proof 在重启后保持 ActorId；Tab Actor 的新建策略属于
Actrium。Tab 与 Service 不由基础设施强制 1:1，同一 Service 允许多个 Tab/窗口连接和重连。

实例的 Service 就是本 Realm 内 Actor 类型与 Realm Profile 所指包相同的 Actor。Front 通过
Signaling 的发现在本 Realm 内按该类型找到它；Actor Registry 不另设入口角色。

## 5. Realm 签名 key 与 Realm Profile

### 5.1 authority descriptor 中的 Realm 信息

Realm 域名不另设发现文件，而是复用该域名的 Actr authority descriptor：

```text
GET https://<domain>/.well-known/actr
```

获取与校验规则以 `actr/protocols/authority-discovery` 为准。按该协议，`/.well-known/actr`
下不定义任何子路径，因此 Realm 相关信息全部以 descriptor 成员与 service relation 表达
（Actr Authority Discovery §4.3–4.4）：

- 可选成员 `realm_signing_keys`：Realm 签名 key（当前与历史，含 `key_id`）、轮换签名链、
  单调递增的 `sequence` 与 `signature`。`signature` 由某个 `active` key 对该成员（去掉
  `signature` 字段后按 RFC 8785 JCS 规范化）签名，覆盖 `sequence`，为已固定 key 的客户端
  和 Loran 提供回滚保护；authority descriptor 其余部分仍是无签名的公开文档；
- core relation `realms`（protocol `actr-realm-profile/1`），`href` 指向 Realm Profile
  服务；
- descriptor 中的 `signaling`、`identity`、`ice-config` 等 service 服务于该 authority
  自身，与该域名下的 Realm 无关。访问某个 Realm 只使用其 Realm Profile 指定的 Realm
  Provider：取不到、验不过或已退役的 Realm Profile 时，客户端不得使用 Realm Profile 以外的
  任何 Realm Provider。创建时选哪个 Realm Provider 由 Actrium、用户或持有方策略决定。

协议对象字段一律使用 snake_case（如 `realm_signing_keys`、`display_name`、`public_key`、
`key_id`、`credential_audience`）。

Realm 签名 key 轮换时，新 key 由旧 key 签名，形成 Realm 签名 key 轮换链（轮换项 payload 为
`{from, to, public_key, sequence}`）；没有有效轮换链的 key 变化按身份变化处理：签名 key
不在已固定的链中时不做 `sequence` 比较，直接要求重新确认信任，确认后以新成员的
`sequence` 为基线。authority descriptor 不列出 Realm 清单，避免枚举。

### 5.2 Realm Profile

每个 Realm 一份，位置为：

```text
GET <realms href>/<realm_id>
```

`realms` 的 `href` 可以指向 Realm 域名自己的静态存放位置（例如
`https://acme.com/actr/realms`），也可以指向 Realm Provider（例如
`https://realms.provider.example/acme.com`，可带显式端口）；Realm Profile 也可以随 Realm
邀请内嵌。Realm Profile 由 Realm 签名 key 签名，因此存放它的一方不成为信任根。`href` 不得
含 query/fragment，拼接前去掉尾部 `/`；多条 `realms` link 按出现顺序尝试，任一返回有效
Realm Profile 即止；所有 link 均未产出有效 Realm Profile（404、其他错误、超时或无效文档）
即视为不可用。

schema 是 `actr.realm/v1`，只包含非秘密信息，至少声明：

- `realm.name`（完整 Realm 名）、`display_name`；
- `status`（`active` | `retired`）；
- 单调递增的 `sequence` 与 `issued_at`（Realm Profile 不设过期时间）；
- issuer：Realm issuer key 的 `public_key` 与 `key_id`、注册和验签端点；
- provider：Realm Provider ID、signaling/STUN/TURN 端点、`credential_audience`；
- 加入策略。

Realm Profile payload 用 RFC 8785 JCS 规范化，附 `active` Realm 签名 key 的 detached Ed25519
签名；由后来退役的 key 签发的既有 Realm Profile 在被更高 `sequence` 取代前仍有效，但签名 key
被从 `keys` 移除时立即失效（见 3.3）。Realm Profile 不过期，回滚保护只靠每个 Realm 单调递增的
Realm Profile sequence（相同 `sequence` 内容必须一致）；`provider.id` 与已接受 Realm
Profile 不同的新 Realm Profile 一律拒绝。退役 Realm Profile 永久有效。

### 5.3 Actor credential 与 TURN 约束

Realm Provider ID 是 Realm Provider 的规范化 DNS authority（例如 `provider.example`），Loran 取
`[authority] uri` 的域名。Actor credential 必须同时满足：

- `protocol_version = 1`
- `issuer = <realm-name>`，例如 `k7m2q9xh@acme.com`
- `provider = <Realm Provider ID>`
- `audience = actr.signaling`
- 携带 Actor 类型 `publisher/program@version`（见第 4 节）

TURN 使用单一端口池。TURN credential 的 HMAC username 同时绑定 Realm 名、Realm Provider、
`actr.turn` audience 和 ActorId；TURN secret 按 Realm/Realm Provider 隔离，由 Realm
Provider 主密钥经 HKDF 以 Realm 名和 Realm Provider ID 派生。第三方 Realm Provider 在 v1
仅有 Realm Profile 数据契约，没有 federation、跨 Realm Provider 委托或信任协商协议。

## 6. 多 Realm 隔离

同一进程承载多个 Realm 时，隔离不能依赖进程边界，必须由以下机制显式保证：

| 风险 | 控制 |
| --- | --- |
| Actor credential 跨 Realm 重放 | claims 绑定 issuer（即 Realm 名）/Realm Provider/audience；signaling 以连接绑定的 Realm 校验每条消息 |
| 缓存串 Realm | signaling presence、Actor Registry、TURN authenticator cache、renewal token、连接 ACL 的 key 均包含 Realm；禁止仅以 Actor serial 或类型做 key |
| Discovery 越界 | Discovery 与 RouteCandidates 只返回同 Realm 的 Actor；v1 不做跨 Realm 发现 |
| 管理越权 | Realm 管理请求只作用于签名 key 所属 Realm 域名；Realm 管理 token 作用域在服务端逐请求检查；列表接口只返回作用域内的 Realm |
| 未激活或已挂起的 Realm 被使用 | 只有 `active` Realm 签发 Actor credential、接受连接与分配 TURN |
| 删除后残留 | 退役在单一事务中撤销 Actor credential、renewal token、presence、Actor Registry 记录与连接 ACL，退役 Realm issuer key，并写入以 Realm 名为键的 Realm Provider tombstone |
| 资源挤占 | 按 Realm 域名与按 Realm 配额 |
| 进程失陷影响全部 Realm | 首版接受 all-in-one 部署的此项风险；生产建议把 Issuer Signer 放入独立安全边界，使主进程失陷不泄露 Realm issuer key 私钥 |

跨 Realm 混淆必须有专门的回归测试：两个 Realm 使用相同 Actor 类型、相同 Actor serial
候选时，Actor credential、presence、TURN、连接 ACL 与管理接口均不得互通。

v1 的连接 ACL 只作用于同一 Realm 内的 Actor 类型。ACL 数据模型保留 `source_realm`
维度供后续跨 Realm 规则使用；v1 管理接口拒绝非空的 `source_realm`。实例内的应用权限
（读写、管理、分享）由应用决定，不属于 Loran。

## 7. Publisher 发现

Publisher Profile 是 `https://<publisher>/.well-known/actr` 中带签名的可选成员
`publisher`，schema 为 `actr.publisher/v1`，字段使用 snake_case，内容以产品规范
（`docs/product/actrium-v1` 05 §2.2、12 §2）为准：`domain` 必须等于 descriptor
`authority` 的主机名，Publisher Profile 签名 key 与 package signing key 分离，带轮换链、
`sequence` 与有效期。成员的有效性只取决于自身签名与已固定的
key history。

Loran 获取 descriptor 的重定向与 SSRF 规则与 3.1 相同。`register_publisher` 把一个
publisher 登记到本 Realm Provider 的 Publisher Registry（此后本 Realm Provider 才为该
publisher 托管包），与 Realm 管理请求共用同一端点 `POST /actr/realm-management`。它复用
3.2 的 Actr 管理信封：以 `publisher` 字段取代 `realm`，由 Publisher Profile 中 `active` 的
Publisher Profile 签名 key 签名，同样绑定 `provider_id`、5 分钟有效期与 nonce；公开文档中
不放 challenge。首个 Publisher Profile 固定其 `profile_signing_keys`；之后只接受由已固定
`active` key 签名、只列已固定 key、`sequence` 更高的 Publisher Profile（轮换链尚未支持，见
第 9 节）。Publisher Profile 须恰有一把 `active` 包签名 key，作为该 publisher 的当前 key；
被替换的 key 归档为 retired，默认只用于历史包验签，泄露时由实例运营者显式 revoke。Loran
不周期刷新 Publisher Profile，key 轮换由 publisher 再次发送 `register_publisher` 完成（见
第 9 节）。

程序身份写作 `publisher/program@version`。实例运行哪个包由 Realm Profile 的 `package`
（下载地址与 SHA-256）给出，包可以放在任何 HTTPS 位置；消费端必须核对 SHA-256，并验证
manifest、publisher key 签名和 key revocation 状态。

包托管：`loran-packages` 只是按内容寻址托管 `.actr` 文件的静态下载服务，
`package.href` 可以指向它，也可以指向 Publisher 自己的 CDN。它不把 Target 解析为包：
Package Resolution、`packages` relation 与 `program_resolver` 已从协议和代码中删除。

## 8. 数据与升级

当前 schema v6 使用 SQLite `PRAGMA user_version = 6`。Loran 遇到旧版或未知 schema
直接拒绝启动，不在启动路径中执行「尝试 ALTER 后忽略错误」。v2 → v3（新增
`issuer_control_attempt`）、v3 → v4（以 Realm 名取代数值 Realm id）与 v4 → v5（去掉单
Realm 约束，`realm.status` 改为 `pending | active | retired`，新增 `realm_domain`、
`realm_profile`、`realm_mgmt_nonce`）与 v5 → v6（删除 `publisher_challenge`，
`publisher.descriptor_json` 改为 `profile_json`）都没有离线迁移，需停机后重建数据库。

v4 已以 Realm 名为键：`realm` 表以列 `name`（完整 Realm 名）为主键，`actoracl`、
`pending_registration`、`issued_actor_identity`、`revoked_actor` 以 `realm` 列引用它，后三者按
`(realm, serial_number)` 建主键；Signaling 的 `actor_registry` 同样按 Realm 名与 serial 建键。

表结构：

- `realm_domain`：`domain`、已接受的 authority descriptor digest、`realm_signing_keys`
  的 `sequence` 与 key 集合、最近校验时间；
- `realm`：以完整 Realm 名（列 `name`）为主键、状态（`pending` | `active` | `retired`，
  实例运营者的本地挂起用独立的 `enabled` 开关）、Realm issuer key id、
  pending 到期时间、请求方（`requester` 或 Realm Provider 账号）、Realm 管理 token
  哈希及其绑定的 Realm Provider 账号（Realm Provider 域名下的 Realm）；Realm Provider tombstone 与活跃 Realm
  共用该主键约束；
- `realm_profile`：每个 Realm 已接受的最新 Realm Profile 原文及其 digest 与 `sequence`；
- Realm 管理请求的 `nonce` 记录，保留到对应请求过期；
- 其余按 Realm 分区的表（连接 ACL、已签发身份登记、Actor Registry、renewal state）以
  `realm` 为键。

Actor Registry 把持久元数据与在线状态分开：注册在 SQLite 写入完成后才成功；
Loran 重启恢复的记录可继续提供 ServiceSpec，但在原 Actor 认证重连前不能进入
Discovery 或 RouteCandidates。显式注销则先原子删除该 Actor 的缓存服务，再返回
成功，因而紧接着重启也不能复活已注销记录。

历史离线工具只覆盖到 v2，按序执行（数据库文件名沿用当前实现；未版本化库先 v1，v1 库直接 v2）：

```bash
python3 scripts/loran/migrate_schema_v1.py /var/lib/actrix/actrix.db --dry-run
python3 scripts/loran/migrate_schema_v1.py /var/lib/actrix/actrix.db
python3 scripts/loran/migrate_schema_v2.py /var/lib/actrix/actrix.db --dry-run
python3 scripts/loran/migrate_schema_v2.py /var/lib/actrix/actrix.db
```

v2 工具把历史表/列/索引改为 `publisher*`、`issuer*` 命名，并把已存 Actor 类型字符串
改写为 `publisher/program@version`。Actor Registry 表由 Signaling 启动时从
`service_registry` 原地改名为 `actor_registry`。

旧的非域名 publisher 身份没有安全的域名映射。可先人工重新 provision，或显式使用
`--drop-publisher-data` 丢弃旧 Publisher Registry/package 数据。工具通过 SQLite backup API
创建一致性备份；迁移失败时不删除备份。旧 Realm 没有 Realm 名，迁移时必须由实例运营者
显式指定映射，并由对应 Realm 域名的 Realm 签名 key 签署新 Realm Profile 后才能激活，不做自动推断。

## 9. 与当前实现的差异

当前代码已承载多个 Realm，以下差异仍需跟进：

- 已实现：authority descriptor 获取（逐跳 SSRF 检查、最多 5 次重定向、64 KiB、缓存含失败、按
  主机限流）、`realm_signing_keys` 成员自签名验证与 key pin（首次接受即 pin、`sequence` 只增、
  只接受已 pin 的 key）、Realm Provider 域名下 Realm 的 Realm Profile 签发、`realm_profile`
  存储与 `GET /actr/realms/{domain}/{realm_id}`、其他域名 Realm 的周期性刷新（含 key 移除后丢弃
  基线、取不到则停用）、退役 Realm Profile 触发的退役事务。尚未实现：`realm_signing_keys`
  轮换链（引入新 key 的成员一律拒绝）、身份变化的重新确认、Realm 签名 key 本身的轮换；
- 已实现其他域名的 Realm 管理请求 `create_realm`、`publish_profile`、`reissue_admin_token`
  （`provider_id` 绑定、5 分钟有效期、nonce 防重放、`(realm, 请求方)` 幂等、待签 Realm Profile）、
  Realm 管理 token（只存哈希、一次性返回、重签即失效、作用域一个 Realm、越权 `404`）与
  `[realm]` 按域名配额、`pending` TTL 清理。尚未实现：Realm 管理 token 目前只能查看 Realm 与 Realm Profile、轮换 secret（Realm 列表、连接 ACL、membership 待实现）；
- 已实现运行时分区：Actor credential 由该 Realm 的 `issuer:<realm>` key 签发（随首份 Realm
  Profile 草稿或运营者创建 `active` Realm 时生成，缺失时签发前补建），验证 key 按
  `(realm, key_id)` 缓存；signaling 连接绑定其身份的 Realm，source/target 不在该 Realm 的
  envelope 返回 `403`；Actor Registry 与 presence 按 Realm 分区；TURN secret 由主密钥按 Realm
  HKDF 派生；注册拒绝指向其他 Realm 的 ACL `source_realm`；跨 Realm 混淆回归测试
  `realms_with_the_same_actor_type_and_serial_are_isolated`。Issuer 原有的全局轮换 key 及其
  `/issuer/current-key`、`/issuer/signing-pubkey`、admin 与 gRPC 轮换入口已删除。尚未实现：
  Realm issuer key 的轮换与退役（Signer 中的 key 不标记退役）；
- Realm Provider 域名下的 Realm 的 Realm Provider 账号认证（OAuth 2.0 + PKCE）、按账号配额
  与账号绑定的 Realm 管理权；
- 已实现 `register_publisher`：读取 authority descriptor 的 `publisher` 成员（第 7 节），
  `/.well-known/actr/publisher.json` 与 challenge 已移除。尚未实现：Publisher Profile 签名 key 的
  轮换链与身份变化的重新确认、Publisher Profile 的周期刷新（过期或 key 移除后不自动停用该
  publisher）；
- 按 Realm 的连接、注册、消息与 TURN 配额；TURN 认证不检查 Realm 是否 `active`（TURN
  credential 随 Actor credential 过期）；
- 控制面与本契约的剩余差异：Realm Provider 域名下的 Realm 由运营者经 admin API 代办创建（等同
  Realm Provider 账号）；Loran 不是 Realm Provider（未启用 authority 或 Issuer 无 Signer）时，
  运营者仍可创建任意域名下没有 Realm Profile 的 `active` Realm，并可经 `RetireRealm` 与
  admin `…/retire` 直接退役；当前退役事务尚未清理 Actor Registry、presence、在线连接与 Realm
  issuer key，靠 `validate_realm` 拒绝已退役 Realm 的新注册与连接。

## 10. 明确不做

- 多节点 HA、自动 worker lease 或跨节点 SQLite；
- Realm federation、跨 Realm 发现与调用、跨 Realm ActorId 迁移；
- 第三方 Realm Provider 之间的 signaling/TURN 委托协议；
- 未经域名所有者委托代持其 Realm 签名 key；
- 由 Realm 签名 key 签发的受限创建委派 key（限定 RealmId 前缀、配额与有效期）；
- Realm 域名级 SSO 或成员目录；
- 更换 Realm Provider（Realm 与 Realm Provider 永久绑定，这是模型而非首版限制）；
- Workspace 生命周期或 Workspace 密钥托管；
- 旧 Actor credential、ActorId、publisher 身份或协议的双轨兼容。

这些能力若进入后续版本，应先扩展 authority descriptor/Realm Profile/protocol version，不得
削弱 v1 的 Realm 隔离不变量或默认拒绝策略。
