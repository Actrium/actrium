# Actrix Realm v1 架构契约

状态：首版目标契约。一个 Actrix 实例作为共享 provider 承载多个 Realm；首版不承诺
HA 或跨 Realm federation。当前代码仍按早先的单 Realm 契约实现，差异见第 9 节。

## 1. Realm 模型

### 1.1 Realm 是实例概念

Realm 对应一次应用实例部署，而不是一个应用（程序），也不是组织级成员域。同一个程序
`notes.example/team-notes` 被 A 组织部署一次、被 B 组织部署一次，就是两个 Realm。
在 Actrium 中，Create Intent 创建 Primary Service 实例时同时创建一个新 Realm；该实例
的 Primary Service、它编排的 Auxiliary Actors 以及 Attach 到它的 Front 都在这个
Realm 内。成员与设备准入、ActorId 命名空间、Actor credential 签发与续期、ACL、撤销与
审计都属于该 Realm。

由此 Realm 的数量按部署次数增长，远多于应用数量，也远多于基础设施节点数量。Realm
必须是轻量的逻辑记录：创建一个 Realm 不启动新进程、不占用新端口、不需要新数据库。

### 1.2 命名空间与本地名

Realm 借用命名空间命名。联网 Realm 使用 DNS 域名作为命名空间：域名在现实中已经具备
全局唯一性，且归属可验证；Realm 只使用这两点，不要求域名指向 Actrix，也不要求域名
承载任何基础设施。本机 Realm 使用 Actro 安装作为命名空间。

| 命名空间 | 形式 | 归属 | 命名空间 key |
| --- | --- | --- | --- |
| DNS | `dns:<domain>` | 域名所有者 | authority descriptor 中声明的 Ed25519 key |
| 本机 | `local:<installation-id>` | 该 Actro 安装 | 无（`installation-id` 由安装根 key 派生；本机 Realm 不使用 Profile） |

DNS 命名空间就是不含端口的 Actr authority：`notes~dns:acme.com` 的命名空间是
authority `actr://acme.com`。这与 Actr URI 草案中「同一 authority 可以运行多个 realm」
一致。

一个命名空间下可以有多个 Realm，每个 Realm 在命名空间内有一个本地名：

```text
Realm = <namespace> + <local_name>
例：dns:acme.com 下的 notes、dns:acme.com 下的 crm
```

- `domain` 是规范化 DNS 域名：小写 IDNA A-label，无 scheme、端口、userinfo 或 path，
  与 Publisher ID 的规范化规则相同；
- `local_name` 使用 DNS label 规则：`[a-z0-9-]`，1..63 字符，不以 `-` 开头或结尾；
  在命名空间内唯一，创建后不可修改；
- 已退役 Realm 的 `local_name` 永不复用：命名空间所有者不得复用，provider 在本地以
  tombstone 强制，避免旧 credential、ActorId 或缓存被新 Realm 继承。

Realm 引用的文本形式：

```text
<local_name>~dns:<domain>                例：notes~dns:acme.com
<local_name>~local:<installation-id>     本机 Realm
```

本机命名空间由 Actro 内嵌实现，本机 Realm 只有本地 record，没有 provider、Profile 与签名请求；Actrix 只承载 DNS 命名空间下的 Realm。

### 1.3 Realm 与 Publisher 的区别

两者都使用域名，但角色不同：

| | Publisher | Realm |
| --- | --- | --- |
| 命名对象 | 程序（谁发布了这个包） | 部署（谁运营这个实例） |
| 标识 | `publisher` 域名 | 命名空间 + `local_name` |
| 长期信任根 | Publisher descriptor 中的签名 key | 命名空间 key |

同一个包可以运行在 `notes~dns:acme.com`，也可以运行在 `notes~dns:foo.org`。

### 1.4 Realm 与 provider 的区别

Realm 是逻辑信任域；Actrix 是 provider，即共享基础设施。一个 Actrix 实例承载来自
多个命名空间的多个 Realm，共用同一进程、同一 SQLite、同一 signaling 服务、同一
STUN/TURN 端口池和同一 Signer（每个 Realm 一把 issuer key）。Realm 在创建时绑定一个
provider，此后不可更换：Profile 的 `provider.id` 在 Realm 生命周期内不变。换运营方即在
新 provider 上新建实例并做应用层数据迁移，旧实例退役。

首版部署形态是单节点、SQLite、官方 Actrix provider；Authority、Rendezvous、Relay、
Publisher Registry 和 Management 是逻辑平面，可以同进程部署。Signer 支持同进程
开发模式，也支持通过现有 TLS/mTLS gRPC 客户端配置放入独立安全边界。

只需要独占部署的场景（如企业自建）可配置 `max_realms = 1`；默认不限制。

Workspace 不属于 Realm 基础设施的数据模型。Workspace 与 Service Actor 是多对多
使用关系，删除 Actor 不删除 Workspace；Workspace 密钥属于用户。这些规则由 Actro
实现，Actrix 不取得目录所有权。

## 2. 信任根

1. 命名空间 key（Ed25519）是命名空间下所有 Realm 的长期信任根，由命名空间所有者
   持有，声明在该域名的 authority descriptor 中，支持多把与轮换链。首次信任来自对
   `domain` 的 HTTPS（Web PKI）访问，或企业预置的命名空间 key pin。
2. Realm Profile 由命名空间 key 签名，把 issuer key 与 provider 端点委派给某个
   provider。Profile 的有效性只取决于签名，不取决于从哪里取到。
3. Realm issuer key（Ed25519）签发 Actor credential，每个 Realm 一把，私钥留在
   provider 的 Signer；容忍窗口仅用于验签，绝不延长签名或 credential 生命周期。
4. Publisher ID 是规范化 DNS 域名。Publisher descriptor 是 authority descriptor
   `https://<publisher>/.well-known/actr` 中带签名的可选成员 `publisher`（已在 Actr
   Authority Discovery §4.5 定义；当前实现的
   `/.well-known/actr/publisher.json` 目标移除，见第 9 节）。Publisher descriptor 中的
   Ed25519 key 才是包的长期真实性根；registry/origin 只是 HTTPS 分发位置，包最终按
   digest 和 publisher signature 验证。

信任链：

```text
Web PKI 或企业 pin → authority descriptor（命名空间描述）→ 命名空间 key
  → Realm Profile → issuer key → Actor credential（Actor 长期 key 另行绑定）
```

provider 只持有 issuer 私钥，不持有命名空间 key；provider 自有命名空间是唯一例外
（见 3.6）。

管理权限分三类：

- 实例运营者：Actrix 实例唯一的系统管理根，管理 provider 配置与全部 Realm 生命周期。
  Admin UI 配置只接受 Argon2id PHC hash，不接受明文密码；管理会话使用进程内 CSPRNG
  生成的 JWT key。
- 命名空间管理：持命名空间 key 者通过签名请求创建、激活 Realm 与重签管理 token；更新
  与退役通过发布新 Profile 完成，无需在 Actrix 另开账号（provider 自有命名空间的个人
  用户除外，见 3.6）。
- Realm 管理员：Realm 激活时签发的、作用域为单个 Realm 的管理 token，通常交给实例
  所有者（交付与重签见 3.5）；只能看到并管理本 Realm：secret 轮换、ACL、成员与设备
  撤销、状态变更。跨作用域访问一律拒绝，且不得通过错误信息泄露其他 Realm 是否存在。

Realm registration secret 是 Realm 管理员/可信宿主能力，不是 Guest workload
credential。AIS 的注册入口同时要求 Realm secret 与 package/linked 身份证明；
Actro/Rill host 可以持有该能力，但不得把它暴露给用户编写的 Rill Guest。注册请求
携带的 ACL 因此是宿主管理行为；无 ACL 时默认拒绝发现/中继。

## 3. Realm 创建与激活

### 3.1 流程

创建 Realm 不需要按 Realm 或按域名做 challenge 验证；授权来自命名空间 key 签名：

1. 调用方（持命名空间 key 的工具、实例运营者或 superv）以 3.2 的签名信封提交
   `create_realm`；
2. Actrix 获取该域名的 authority descriptor（见 5.1）并验证 `realm_namespace`，再用
   其中的命名空间 key 验证请求签名；通过后创建 `pending` Realm，在 Signer 中生成该
   Realm 的 issuer key，在响应中返回待签 Profile payload（issuer key、端点、
   `provider.id`）；
3. 调用方用命名空间 key 签名 Profile，**总是**以 `activate_realm` 请求（同一信封，
   携带签名 Profile）提交给 Actrix；
4. Actrix 验证 Profile 签名，并确认 Profile 中的 issuer key 与 provider 端点正是自己，
   Realm 转为 `active`。`activate_realm` 的响应即激活响应，一次性返回该 Realm 的管理
   token（见 3.5）。若 `realms` 位置由本 provider 托管，Actrix 同时对外提供该 Profile；
   否则由调用方发布到自己声明的位置。

provider 自有命名空间（3.6）由 Actrix 自己签 Profile，第 3 步在内部完成，激活结果直接
返回给通过账号认证的用户。

幂等与冲突：创建以 `(namespace, local_name, 请求方)` 为幂等键。DNS 命名空间的请求方是
信封中可选的 `requester` 字段（组织签名服务填写的不透明成员标识，缺省为空）；provider
自有命名空间的请求方是用户账号。同一请求方对自己的 pending 或 active Realm 重试时返回
其状态，不返回管理 token；不同请求方命中同名 Realm，或任何请求命中 tombstone，一律返回
冲突。对已 active 的 Realm 重复 `activate_realm` 返回冲突；管理 token 丢失（包括 provider
自有命名空间中丢失 `create_realm` 响应）只能用 `reissue_admin_token` 取回。

只有 `active` Realm 才签发 credential、接受 signaling 连接和分配 TURN。

`pending` Realm 有 TTL，过期清理。因为创建请求必须由命名空间 key 签名，他人无法在
本 provider 上抢注某个命名空间下的本地名。

谁来签名：

- 个人用户在 v1 只能在本机命名空间或 provider 自有命名空间（见 3.6）自助创建 Realm；
  后者由 provider 代签，用户以其在该 provider 的账号认证（见 3.6）；
- 在组织命名空间创建 Realm，必须经组织持有命名空间 key 的签名工具或服务：它按组织
  自己的策略认证成员，再代签创建请求与 Profile；Actrix 不替组织判断谁有权创建；
- 组织自建 provider 时，可以选择让自己的 provider 持有命名空间 key，这是组织自己的
  基础设施，不属于第三方代持。

Actrix 获取 authority descriptor 与 Profile 时遵循 authority-discovery 的重定向规则：
最多跟随 5 次，每一跳必须是 HTTPS，并始终用原 authority 验证返回的 `authority` 绑定。
不另设比客户端更严的「一律不跟随」规则，避免同一 descriptor 对客户端有效而 provider
拒绝。作为服务端，Actrix 在每一跳额外执行 SSRF 检查：拒绝内网/loopback/link-local
DNS 结果，把该跳请求钉住到预检查的公网地址（逐跳 DNS pinning），响应限长 64 KiB，
避免 SSRF 和 DNS rebinding。由于验签前就要外联，Actrix 按来源与目标域名限流，并缓存
descriptor 获取结果（含失败）。

### 3.2 命名空间签名信封

v1 由命名空间 key 授权的管理操作只有 `create_realm`、`activate_realm` 与
`reissue_admin_token`，使用同一签名信封，只有 `op` 与操作字段不同（Publisher 注册复用
同一信封，见第 7 节）。请求以 HTTPS `POST` 发往 Actrix 自身 authority descriptor
（`https://<provider_id>/.well-known/actr`）中的 `realm-management` service link（protocol
`actr-realm-management/1`）；provider 自有命名空间的请求携带同一 payload、不带签名，
改由账号的 OAuth bearer token 授权：


```json
{
  "op": "create_realm",
  "namespace": "dns:acme.com",
  "local_name": "k7m2q9xh",
  "requester": "<可选，组织成员标识>",
  "provider_id": "provider.example",
  "nonce": "<random>",
  "issued_at": "<RFC 3339>",
  "expires_at": "<RFC 3339>"
}
```

- payload 按 RFC 8785 JCS 规范化，附命名空间 key 的 detached Ed25519 签名与 `key_id`；
- `provider_id` 必须等于接收方的 provider ID，即 provider 的规范化 DNS authority，防止请求
  被重放到其他 provider；
- `expires_at - issued_at` 不超过 5 分钟，Actrix 拒绝已过期或尚未生效的请求；
- Actrix 在有效期内记录 `nonce`，拒绝重复使用；nonce 只防字节级重放，重试靠 3.1 的
  幂等键；
- 签名 key 必须是 `realm_namespace` 中的 `active` key。

### 3.3 持续校验

Actrix 以同一节奏周期性重新获取 authority descriptor（含 `realm_namespace`）与
Profile。若已接受 Profile 的签名 key 已从 `keys` 中移除（吊销），丢弃该 Profile 及其
`sequence` 基线，改用最后一份由仍列出的 key 签名的 Profile；没有则按从未接受处理并
重新获取。出现以下任一情况时，本地挂起该 Realm，不再签发新 credential：

- Profile 的 `provider.id` 不是本 provider，或其 issuer key 不在本 provider 的 Signer 中；
- Profile `status` 为 `retired`；
- 找不到由 `realm_namespace` 中仍列出的 key 签名的有效 Profile；
- Profile 或 `realm_namespace` 的 `sequence` 低于已接受的值（回滚）。

### 3.4 退役

退役只有一条路径：命名空间所有者发布 `status = retired` 的新 Profile（永久有效；保持
相同 `provider.id`，`sequence` 更高）。
Actrix 通过 3.3 的周期校验得知后，在单一事务中撤销 credential、renewal token、presence、
registry 与 ACL，退役 issuer key，并写入 `(namespace, local_name)` tombstone。若绑定的
provider 已停服，命名空间所有者把 `realms` href 改指自控位置，并在那里发布 retired
Profile。provider 自有命名空间随 provider 一起消失，其中的 Realm 无法退役，只能在别处
新建。

### 3.5 Realm 管理 token 的交付

Realm 管理 token 在激活响应中一次性返回给发起创建的调用方，即命名空间签名工具（由其
交给实例所有者），或 provider 自有命名空间下通过账号认证的用户；存入安全存储。Actrix
只保存 token 的哈希。token 丢失时重新签发，旧 token 立即失效：DNS 命名空间由命名空间
key 签名的 `reissue_admin_token` 请求完成；provider 自有命名空间由绑定的账号登录后
完成，换机或丢设备只需重新登录。

### 3.6 provider 自有命名空间

没有自己域名的用户可以使用 provider 运营方的命名空间，例如
`dns:realms.provider.example`。此时命名空间 key 由 provider 运营方托管，Actrix 可以
直接服务该命名空间的 authority descriptor 与 Profile，信任根即 provider 运营方；
客户端 UI 必须如实显示这一点。对第三方域名，Actrix 默认不代持命名空间 key。

在 provider 自有命名空间自助创建时，用户以其在该 provider 的账号认证：OAuth 2.0 授权码
+ PKCE（RFC 6749、RFC 7636）。该命名空间 descriptor 中 `identity` service link 的
`href` 即 OAuth issuer identifier，元数据按 RFC 8414 发现；授权服务器接受固定公共
client ID `actr-host`（无 secret，redirect 按 RFC 8252），Host 请求 scope
`realm_management`。Actrix 按账号计配额，并把 Realm 管理权绑定到
该账号。账号注册、登录方式与防滥用由 provider 决定，不属于本契约。provider 能关联同一
账号创建的全部实例（它本来就是签名方）；实例成员看不到账号，设备 key 仍按 Realm 独立。

provider 作为 authority 的 `/.well-known/actr` 同时承载它自己的 `services`（signaling、
identity、ice-config 等）与自有命名空间的 `realm_namespace`、`realms`，是同一份文档。
这是预期行为：同一个域名既是 provider 的服务入口，也是 provider 自有 Realm 的命名空间。

### 3.7 配额

按命名空间与按 Realm 设配额：命名空间可创建的 Realm 数（provider 自有命名空间再按账号
计）、连接数、注册速率、signaling 消息速率、TURN allocation 数与带宽。

## 4. 身份模型

Actor ID 是 Actor 实例身份，不是用户、Workspace 或逻辑服务名。它携带：

- Realm 引用（命名空间 + `local_name`）；
- 54-bit Snowflake serial；
- Actor 的出生类型 `publisher/program@version`，文本编码为 `publisher:program:version`。

目标文本形式：

```text
<serial_hex>@<local_name>~dns:<domain>/<publisher>:<program>:<version>
例：2af01@notes~dns:acme.com/notes.example:team-notes:1.4.2
```

早先草案中的 `2af01@42~dns:team.example/...`（数值 realm_id + authority）作废。legacy
形式 `<serial_hex>@<realm_id>/...` 导入时必须显式补足 Realm 引用并重写，不做推断。

`local_name` 默认由客户端随机生成（base32 小写短串，不含程序名），可读名称放在 Profile
的 `display_name`；Actrix 不从 `local_name` 推断任何语义，也不在对外接口中泄露同一
命名空间下其他 Realm 是否存在。

设备在每个 Realm 使用独立的设备 key，不跨 Realm 复用；设备注册与 membership 按 Realm
分别进行，Actrix 无法据设备 key 关联同一设备加入的不同 Realm。

Actrix 内部使用数值 `realm_id`（u32）作为数据库主键和索引句柄。它只在单个 provider
内有意义，不是身份：不得出现在对外 credential issuer、Profile、ActorId 或跨 provider
的比较中。

Worker ID 为显式配置的 0..31 值，不再由 PID/hostname 的 5-bit hash 猜测。单节点
默认 0；未来并发 issuer 必须由部署租约分配不同 worker ID。SQLite 的
`issued_actor_identity` 永久登记本实例已签发的 `(Realm, serial)`；进程重启或时钟
回拨导致 Snowflake 候选碰撞时只会继续分配，绝不覆盖原身份。跨 package version
升级视为新 Actor，稳定业务入口应使用 Service alias/ActorId 记录层表达，不能修改既有
Actor ID 的含义；升级产生的新 Service Actor 仍在同一 Realm 内。实例升级若必须保留
Actor ID，应重签同一出生类型。

Service Actor 可用 renewal possession proof 在重启后保持 Actor ID；Tab Actor 的
新建策略属于 Actro。Tab 与 Service 不由基础设施强制 1:1，同一 Service Actor
允许多个 Tab/窗口连接和重连。

## 5. 命名空间描述与 Realm Profile

### 5.1 命名空间描述：authority descriptor 的扩展

DNS 命名空间不另设发现文件，而是复用该域名的 Actr authority descriptor：

```text
GET https://<domain>/.well-known/actr
```

获取与校验规则以 `actr/protocols/authority-discovery` 为准。按该协议，版本 1 不定义
`/.well-known/actr` 下的任何子路径，因此 Realm 相关信息全部以 descriptor 成员与
service relation 表达：

- 可选成员 `realm_namespace`：命名空间 key（当前与历史，含 `key_id`）、轮换签名链、
  单调递增的 `sequence` 与 `signature`。`signature` 由某个 `active` 命名空间 key 对该成员
  （去掉 `signature` 字段后按 RFC 8785 JCS 规范化）签名，覆盖 `sequence`，为已固定
  key 的客户端和 Actrix 提供回滚保护；authority descriptor 其余部分仍是无签名的公开
  文档；
- core relation `realms`（protocol `actr-realm-profile/1`），`href` 指向 Realm Profile
  服务；
- descriptor 中已有的 `signaling`、`identity`、`ice-config` 等 service 是该 authority
  的默认 provider，只用于创建新 Realm 时的建议。某个 Realm 的委派只以其 Profile 为准：
  取不到、验不过或已退役的 Profile 时，客户端不得改用默认 provider 与该 Realm
  通信。

`realm_namespace` 成员与 `realms` relation 已在 Actr Authority Discovery §4.3–4.4 定义。

协议对象字段一律使用 snake_case（如 `realm_namespace`、`local_name`、`display_name`、
`public_key`、`key_id`、`credential_audience`）。

命名空间 key 轮换时，新 key 由旧 key 签名，形成轮换链（轮换项 payload 为
`{from, to, public_key, sequence}`）；没有有效轮换链的 key 变化
按身份变化处理：签名 key 不在已固定的链中时不做 `sequence` 比较，直接要求重新确认
信任，确认后以新成员的 `sequence` 为基线。authority descriptor 不列出 Realm 清单，
避免枚举。

### 5.2 Realm Profile

每个 Realm 一份，位置为：

```text
GET <realms href>/<local_name>
```

`realms` 的 `href` 可以指向域名自己的静态托管（例如 `https://acme.com/actr/realms`），
也可以指向 provider（例如 `https://realms.provider.example/acme.com`，可带显式端口）；
Profile 也可以随邀请内嵌。Profile 由命名空间 key 签名，因此托管它的 provider 不成为
信任根。`href` 不得含 query/fragment，拼接前去掉尾部 `/`；多条 `realms` link 按出现
顺序尝试，任一返回有效 Profile 即止；所有 link 均未产出有效 Profile（404、其他错误、
超时或无效文档）即视为不可用。

schema 是 `actr.realm/v1`，只包含非秘密信息，至少声明：

- `namespace`、`local_name`、`display_name`；
- `status`（`active` | `retired`）；
- 单调递增的 `sequence` 与 `issued_at`（Profile 不设过期时间）；
- issuer：`issuer` 标识、issuer 的 `public_key` 与 `key_id`、注册和验签端点；
- provider：provider ID、signaling/STUN/TURN 端点、`credential_audience`；
- 加入策略。

Profile payload 用 RFC 8785 JCS 规范化，附 `active` 命名空间 key 的 detached Ed25519
签名；由后来退役的 key 签发的既有 Profile 在被更高 `sequence` 取代前仍有效，但签名
key 被从 `keys` 移除时立即失效（见 3.3）。Profile
不过期，回滚保护只靠每个 Realm 单调递增的 `sequence`（相同 `sequence` 内容必须一致）；
`provider.id` 与已接受 Profile 不同的新 Profile 一律拒绝。已退役的 Profile 永久有效。

### 5.3 credential 与 TURN 约束

provider ID 是 provider 的规范化 DNS authority（例如 `provider.example`）。当前实现把它
固定为常量 `actrix-official`，目标移除（见第 9 节）。Actor credential 必须同时满足：

- `protocol_version = 1`
- `issuer = actr:realm:<realm-ref>:authority`，例如
  `actr:realm:k7m2q9xh~dns:acme.com:authority`
- `provider = <provider ID>`
- `audience = actr.signaling`

TURN 使用单一端口池。TURN credential 的 HMAC username 同时绑定 Realm 引用、
provider、`actr.turn` audience 和 Actor ID；TURN secret 按 Realm/provider 隔离，由
provider 主密钥经 HKDF 以 Realm 引用和 provider ID 派生。第三方 provider 在 v1 仅有
Profile 数据契约，没有 federation、跨 provider 委托或信任协商协议。

## 6. 多 Realm 隔离

同一进程承载多个 Realm 时，隔离不能依赖进程边界，必须由以下机制显式保证：

| 风险 | 控制 |
| --- | --- |
| credential 跨 Realm 重放 | claims 绑定 issuer（含 Realm 引用）/provider/audience；signaling 以连接绑定的 Realm 校验每条消息 |
| 缓存串 Realm | signaling presence、service registry、TURN authenticator cache、renewal token、ACL 的 key 均包含 Realm；禁止仅以 Actor serial 或类型做 key |
| Discovery 越界 | Discovery 与 RouteCandidates 只返回同 Realm 的 Actor；v1 不做跨 Realm 发现 |
| 管理越权 | 命名空间签名请求只作用于本命名空间；Realm 管理 token 作用域在服务端逐请求检查；列表接口只返回作用域内的 Realm |
| 未激活或已挂起的 Realm 被使用 | 只有 `active` Realm 签发 credential、接受连接与分配 TURN |
| 删除后残留 | 退役在单一事务中撤销 credential、renewal token、presence、registry 与 ACL，退役 issuer key，并写入 `(namespace, local_name)` tombstone；`realm_id` 由 AUTOINCREMENT 保证不复用 |
| 资源挤占 | 按命名空间与按 Realm 配额 |
| 进程失陷影响全部 Realm | 首版接受 all-in-one 部署的此项风险；生产建议把 Signer 放入独立安全边界，使主进程失陷不泄露 issuer 私钥 |

跨 Realm 混淆必须有专门的回归测试：两个 Realm 使用相同 Actor 类型、相同 serial
候选时，credential、presence、TURN、ACL 与管理接口均不得互通。

v1 的 ACL 只作用于同一 Realm 内的 Actor 类型。ACL 数据模型保留 `source_realm_id`
维度供后续跨 Realm 规则使用；v1 管理接口拒绝非空的 `source_realm_id`。

## 7. Publisher 发现

目标契约：Publisher Profile 是 `https://<publisher>/.well-known/actr` 中带签名的可选成员
`publisher`，schema 为 `actr.publisher/v1`，字段使用 snake_case，内容以产品规范
（`docs/product/actrium-v1` 05 §2.2、12 §2）为准：`domain` 必须等于 descriptor
`authority` 的主机名，Profile signing key 与 package signing key 分离，带轮换链、
`sequence`、有效期与必填的 `program_resolver`。成员的有效性只取决于自身签名与已固定的
key history。

Actrix 获取 descriptor 的重定向与 SSRF 规则与 3.1 相同。`register_publisher` 把一个
publisher 登记到本 provider 的 MFR 与包服务（此后本 provider 才为该 publisher 解析与
分发包）。它复用 3.2 的签名信封：以 `publisher` 字段取代 `namespace`/`local_name`，由
Profile 中 `active` 的 Profile signing key 签名，同样绑定 `provider_id`、5 分钟有效期与
nonce；公开文档中不放 challenge。Actrix 验证签名 key 属于当前 `publisher` 成员后登记
该 publisher，并持久化已接受的 Profile 与 `sequence`；key 轮换通过
发布新 Profile 完成，Actrix 周期刷新后归档旧 key、启用新 key。旧 key 默认只用于历史包
验签；泄露时由实例运营者显式 revoke。

当前实现：`loran-mfr` 从 `https://<publisher>/.well-known/actr/publisher.json` 读取
`actr.publisher/v1` descriptor，结构为单个 `key_id`/`public_key`、`registries`、
`origins` 与 `verification_challenge`，不签名，禁用重定向。这是预发布结构，与目标契约
不兼容，目标移除（见第 9 节）；目标 schema 名 `actr.publisher/v1` 由新结构占用。

程序身份写作 `publisher/program@version`。合法包可以由任意 Profile 允许的 HTTPS
origin/CDN 分发；消费端必须验证 manifest、二进制 digest、publisher key 签名和 key
revocation 状态。

## 8. 数据与升级

schema v1 使用 SQLite `PRAGMA user_version = 1`。Actrix 遇到旧版或未知 schema
直接拒绝启动，不在启动路径中执行「尝试 ALTER 后忽略错误」。

目标表结构：

- `realm_namespace`：`domain`、已接受的 authority descriptor digest、`realm_namespace`
  的 `sequence` 与命名空间 key 集合、最近校验时间；
- `realm`：内部 `realm_id`、`namespace`、`local_name`、状态（`pending` | `active` |
  `suspended` | `retired`）、已接受的 Profile digest 与 `sequence`、issuer key id、
  pending 到期时间、请求方（`requester` 或账号）、Realm 管理 token 哈希及其绑定的账号
  （provider 自有命名空间）；`(namespace, local_name)` 唯一，tombstone 与活跃 Realm 共用该
  约束；
- 签名信封的 `nonce` 记录，保留到对应请求过期；
- 其余按 Realm 分区的表（ACL、issued identity ledger、service registry、renewal
  state）以 `realm_id` 为外键。

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
创建一致性备份；迁移失败时不删除备份。旧 Realm 没有命名空间与本地名，迁移时必须
由实例运营者显式指定映射，并由对应命名空间 key 签署新 Profile 后才能激活，不做自动
推断。

## 9. 与当前实现的差异

当前代码仍按早先的单 Realm 契约实现，以下差异需要跟进：

- 移除 `idx_realm_singleton` 唯一索引与 `Realm::create` 中的单例检查，增加
  `max_realms` 配置；
- 新增 `realm_namespace` 表；`realm` 表增加 `namespace`、`local_name`、`pending`/
  `suspended`/`retired` 状态、Profile digest 与 `sequence`；
- 实现 authority descriptor 获取（含逐跳 SSRF 检查的重定向处理、限流与结果缓存）与
  `realm_namespace` 验签、签名信封（`create_realm`、`activate_realm`、
  `reissue_admin_token`、`register_publisher`；`provider_id` 绑定、5 分钟有效期、nonce
  防重放、含请求方的幂等键）、待签 Profile payload、Profile 验证激活与周期性校验（含
  key 移除后丢弃 Profile）；
- provider ID 从常量 `actrix-official` 改为 provider 的规范化 DNS authority；
- provider 自有命名空间的账号认证（OAuth 2.0 + PKCE）、按账号配额与账号绑定的管理权；
- 移除 Actrix 自身服务的 `/.well-known/actr/realm.json`（它与 authority-discovery
  「`/.well-known/actr` 下不定义子路径」冲突）；仅在 authority descriptor 的 `realms`
  relation 指向本 provider 时托管 `actr.realm/v1` Profile；
- Publisher 验证改为读取 authority descriptor 的 `publisher` 成员并按第 7 节的签名
  注册请求登记，移除 `/.well-known/actr/publisher.json`（`loran-mfr` 的
  `PUBLISHER_DESCRIPTOR_PATH`）与 `verification_challenge`，重定向处理与 Realm 相同
  （逐跳 SSRF 检查）；
- issuer 从 `actr:realm:<realm_id>:authority` 改为 `actr:realm:<realm-ref>:authority`；
  ActorId 文本与 wire 格式加入 Realm 引用；
- 实现 Realm 管理员作用域 token 及其一次性交付与重签、按命名空间与按 Realm 配额、
  由 retired Profile 触发的 Realm 退役事务与 tombstone；
- TURN secret 改为按 Realm 引用 HKDF 派生；
- 补跨 Realm 混淆回归测试；
- 控制面 `CreateRealm`/`DeleteRealm`/`ListRealms` 的语义与本契约对齐：创建需命名空间
  key 签名；`DeleteRealm` 只能作为实例运营者的本地挂起/清理，不能替代 retired Profile。

## 10. 明确不做

- 多节点 HA、自动 worker lease 或跨节点 SQLite；
- Realm federation、跨 Realm 发现与调用、跨 Realm Actor ID 迁移；
- 第三方 signaling/TURN provider 的委托协议；
- 对第三方域名代持命名空间 key；
- 由命名空间 key 签发的受限创建委派 key（限定 `local_name` 前缀、配额与有效期）；
- 命名空间级 SSO 或成员目录；
- 更换 provider（Realm 与 provider 永久绑定，这是模型而非首版限制）；
- Workspace 生命周期或 Workspace 密钥托管；
- 旧 credential、Actor ID、MFR 身份或协议的双轨兼容。

这些能力若进入后续版本，应先扩展 authority descriptor/Profile/protocol version，不得
削弱 v1 的 Realm 隔离不变量或默认拒绝策略。
