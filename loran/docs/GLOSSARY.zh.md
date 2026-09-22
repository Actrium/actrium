# Loran Realm v1 术语

写作规则：正文中凡不是专有名词的普通词（provider、label、token、domain、key、credential、
request、registry、invitation 等）必须带限定前缀，前缀为其所属专名（Realm、Actor、
Publisher、Workspace、Issuer、Actr）；有歧义处一律写全称。schema 字段名、JSON 成员、
配置键、`op` 与状态枚举值保持简短，正文引用时用反引号，首次出现带全称（如「Realm label
（`label`）」）。状态词、sequence、rotation 附着宿主（active Realm、active key、Realm
Profile sequence）。

## Realm

一次应用实例部署的信任与治理域：Actor identity、连接 ACL、Realm membership、Realm
客户端准入与 Realm Provider 委派都以它为边界。Realm 不是应用（程序），也不是 Loran
实例；目标契约下一个 Loran 实例承载多个 Realm（当前实现仍恰好一个）。

## Realm 名

Realm 的完整名字：`<label>@<domain>`，例如 `k7m2q9xh@acme.com`。每个 Realm 都在某个
Realm 域名下：普通 DNS 域名（即不含端口的 Actr authority），没有域名就不分配 Realm。
唯一例外是本机 Realm，其域名为本机伪域名 `<installation-id>.actr.alt`，只有本地记录，
不使用 Realm Profile、不经过 Loran。一个 Realm 域名下可以有多个 Realm。

## Realm label

Realm 名中 `@` 之前的部分（`label`），DNS label 规则，默认随机生成，创建后不可修改，
退役后永不复用。可读名称放在 Realm Profile 的 `display_name`。

## installation-id

Actro 安装时随机生成的 128 位标识（base32 小写 26 字符），只用于本机伪域名。

## Realm 签名 key

Realm 域名所有者在 authority descriptor（`https://<domain>/.well-known/actr`）的
`realm_signing_keys` 成员中公布的 Ed25519 key，是该域名下所有 Realm 的长期信任根，签署
Realm Profile 与 Realm 管理请求，支持多把 key 与 Realm 签名 key 轮换链。
`realm_signing_keys` 与 `realms`、`realm-management` relation 定义于 Actr Authority
Discovery §4.3–4.4。

## Realm 签名 key 持有方

持有某 Realm 域名 Realm 签名 key 的一方：即域名所有者；域名所有者可委托某个 Realm
Provider 代持。没有自己域名的个人可由 Realm Provider 在其域名下分配 Realm，此时该 Realm
Provider 即域名所有者与持有方，也是这些 Realm 的信任根。Loran 默认不持有第三方域名的
Realm 签名 key，只在受托时代持。

## realm_id

Loran 内部的数值 Realm 句柄（数据库主键）。目标契约下它不是身份，不出现在对外
Actor credential、Realm Profile 或 ActorId 中；当前实现仍把它用在 credential issuer
与 ActorId 里。

## Realm Provider

一套正在运营的共享基础设施部署（例如某家运营的 Loran），为多个 Realm 提供 signaling、
TURN 与 Issuer。Realm Provider ID 是其 DNS authority（如 `provider.example`）。Realm 与
创建时的 Realm Provider 永久绑定，Realm Profile 中的 `provider.id` 不可更改。与
Workspace 的存储 Workspace Provider 不是同一概念。

## Issuer

为 Realm 签发 Actor 身份的服务（一个 Issuer 服务承载多个 Realm，代码 `crates/loran-issuer`，
路由 `/issuer`，配置 `[services.issuer]`）：验证注册证明，分配 Actor serial，并通过
Issuer Signer 签发短期 Actor credential。Actor credential 的 issuer 字段即 Realm 名。
Issuer 不是用户目录，也不拥有 Workspace。

### Issuer Signer（Issuer 的签名后端）

Issuer 的组成部分（代码 `crates/loran-signer`，配置 `[services.signer]`，其他服务经
`dependencies.signer` 访问，gRPC 包 `signer.v1`）：持有各 Realm 的 Realm issuer key 私钥
（每个 Realm 一把），为 Issuer 签名，并向 signaling 提供 Realm issuer key 公钥用于验证
Actor credential。没有下载私钥的 API。可与 Issuer 同进程，生产环境建议独立部署在安全
边界或 HSM 内，使主进程失陷不泄露私钥；存储加密 KEK 经 `ACTRIX_SIGNER_KEK` 提供。

## ActorId

Actor 实例身份，目标形式 `<serial_hex>.<label>@<domain>`。它不含程序类型，不是 Workspace、
用户或可变服务别名。当前实现为数值 realm_id + Actor serial + 程序类型。

## Actor 类型（Program identity）

`publisher/program@version`，表示不可变的程序版本，不表示某个 Actor 实例。由注册请求
声明、写入 Actor credential 与 Actor Registry，供连接 ACL 与发现使用。linked/local
workload 可使用 Actr Host 管理的非 DNS publisher 值，不能借此进入 Publisher Registry。

## Actor Registry

Loran 按 Realm 记录在线 Actor 及其 Actor 类型、端点的注册表，供发现与连接 ACL 使用。
SQLite 表为 `actor_registry`（启动时由旧表 `service_registry` 原地改名）。

## Actr Host / Guest

Actr Host 是运行 Actor 的宿主程序（实现 Actr 运行时），与在其中运行的 Guest（Workload
guest）相对。Actro 是一款产品，提供 Actr Host 能力及更多功能，不是 Actr Host 的同义词。

## Publisher

程序发布者。Publisher ID 是规范化 DNS 域名（Publisher 域名）；Publisher Profile 是其
authority descriptor 中带签名的可选成员 `publisher`（schema `actr.publisher/v1`），由
Publisher Profile 签名 key 签署。

## Publisher Registry

Loran 中管理 publisher 身份、Publisher 签名权限与发布审计的服务（代码
`crates/loran-publisher-registry`，公开路由 `/registry/*`，管理 API
`/admin/api/publishers`，配置 `[services.publisher_registry]`），也是
`register_publisher` 的接收方。Publisher key id 形如 `actr-pub-<16 hex>`。

## 连接 ACL

Actr 运行时与 signaling 按 Actor 类型与 Realm 决定谁可发现、连接谁的规则（`AclRule`）。
与实例内的应用权限（读写、管理、分享，由应用决定）不是同一概念。

## Rendezvous / Signaling

帮助已认证 Actor 发现对方和交换 WebRTC 建连信息的服务。它不签发身份，不是
通信内容的最终信任根。

## STUN

Session Traversal Utilities for NAT。公开 Binding 服务让客户端得知服务端观测到的
网络地址。

## TURN

Traversal Using Relays around NAT。直连失败时中继流量；allocation 需要绑定
Realm/Realm Provider/audience/Actor/expiry 的短期 TURN credential。配置中的 `[turn].realm`
是 TURN 认证域字符串（TURN REALM），与 Actr Realm 不是一回事。

## Realm Profile

每个 Realm 一份的 `actr.realm/v1` 元数据，由 Realm 签名 key 签名，委派 Realm issuer key
与 Realm Provider 端点，存放于 `<realms href>/<label>`。存放 Realm Profile 的一方不成为
信任根。Realm Profile 不过期，回滚保护只靠 Realm Profile sequence；`provider.id` 在 Realm
生命周期内不可变。status 为 `retired` 的即退役 Realm Profile。

## Realm 管理请求

发往 Realm Provider `realm-management` 端点的请求：`create_realm`、`activate_realm`、
`reissue_admin_token`，须经 Realm 签名 key 持有方授权。持有方自己签名时使用 Actr 管理
信封（JCS + detached Ed25519，绑定 `provider_id`、5 分钟有效期与 nonce）；持有方是 Realm
Provider 时，请求以 Realm Provider 账号的 bearer token 授权。

## Actr 管理信封

Realm 管理请求与 `register_publisher` 共用的签名请求格式（Actr management envelope）。

## Realm Provider 账号

在 Realm Provider 域名下的 Realm（Realm 域名等于或属于 Realm Provider ID）创建与管理时的
认证主体，经 OAuth 2.0 授权码 + PKCE 登录（入口为 Realm Provider 自身 descriptor 的
`identity` service link，固定公共 client ID `actr-host`；bearer 请求忽略 `requester`）。配额按
账号计，Realm 管理权绑定该账号，换机或丢失客户端只需重新登录。

## Realm 管理 token

单个 Realm 的管理凭证（管 Realm membership、Realm 客户端等），激活时一次性返回；与
Realm Provider 的实例运维权限分开。

## Realm 客户端 key

某成员的某个客户端安装在某个 Realm 中使用的 key，按 Realm 独立，可单独吊销；不跨
Realm 复用，防止跨实例关联同一客户端。对应 Realm 客户端注册。

## Realm membership

客户端已在该 Realm 注册其 Realm 客户端 key 且未被撤销。没有单独的 membership credential：
Issuer 只对已注册、未撤销的 Realm 客户端 key 签发 Actor credential。

## Realm Provider tombstone

Realm Provider 本地以 Realm 名为键的已退役/已用记录，防止 Realm label 复用。与退役
Realm Profile（公开、签名）不同。

## Realm descriptor（当前实现）

当前实现在 `/.well-known/actr/realm.json` 发布的单 Realm 非秘密元数据。目标契约中
由 Realm Profile 取代；该路径与 authority-discovery「`/.well-known/actr` 下不定义
子路径」冲突，将被移除。

## Workspace

用户拥有的工作目录。它可被多个 Service Actor 使用，一个 Actor 也可使用多个
Workspace；Actor 删除不删除 Workspace。Workspace 不属于 Loran Realm 基础设施。

## 历史名称对照

正文统一使用左列规范名；右列历史名只出现在本表、数据库迁移代码、旧文档，以及仍沿用
旧名的 `actrix*` 包名/二进制名中。

| 规范名 | 历史名 | 说明 |
| --- | --- | --- |
| Loran | Actrix | 服务端套件旧名；发布的 Rust 包与二进制仍名 `actrix*`（如 `crates/loran-server`、`actrix-issuer`、`actrix-publisher-registry`），默认数据库文件仍为 `actrix.db`，示例配置为 `actrix.example.toml`，指标前缀 `actrix_` |
| Issuer | Authority、AIS（Actor Identity Service） | 目录 `crates/loran-ais` → `crates/loran-issuer`；包 `actrix-ais` → `actrix-issuer`；路由 `/ais` → `/issuer`；配置 `[services.ais]` → `[services.issuer]`；Actr.toml `ais_endpoint` → `issuer_endpoint`；表 `ais_renewal_token`、`ais_reissue_response_cache`、`ais_manufacturer_auth_nonce` → `issuer_renewal_token`、`issuer_reissue_response_cache`、`issuer_publisher_auth_nonce`（schema v2 离线迁移）；管理路由 `/admin/api/services/ais/keys` → `/admin/api/services/issuer/keys`；key 存储 `ais_keys.db` → `issuer_keys.db`（启动时迁移） |
| Issuer Signer | KS、Key Server | gRPC 包 `ks.v1` → `signer.v1`；配置 `[services.ks]`、`dependencies.ks` → `[services.signer]`、`dependencies.signer`；环境变量 `ACTRIX_KS_KEK` → `ACTRIX_SIGNER_KEK`；示例 `ks.db` → `signer.db`；Postgres 默认库 `actrix_ks` → `actrix_signer`；指标 `KS_*` → `SIGNER_*` |
| publisher | manufacturer | 包清单 `actr-package/1` 与 proto `ActrType` 曾用此字段名；`actr-package/2` 起统一为 `publisher`（proto 字段编号不变），旧键被拒绝并提示改名；Actor 类型字符串旧形式 `publisher:name:version`（曾写作 `manufacturer:name:version`）→ `publisher/program@version`，旧形式被拒绝，已存数据由 schema v2 离线工具改写；CLI 用户配置 `[mfr] manufacturer` → `[publisher] name` |
| Publisher Registry | MFR（源自 manufacturer） | 目录 `crates/loran-mfr` → `crates/loran-publisher-registry`；包 `actrix-mfr` → `actrix-publisher-registry`；路由保持 `/registry/*` 与 `/admin/api/publishers`；配置 `[services.mfr]` → `[services.publisher_registry]`；表 `mfr*` → `publisher*`、列 `mfr_id`/`manufacturer` → `publisher_id`/`publisher`（schema v2 离线迁移）；key id 格式 `actr-pub-<16 hex>` 不变；GitHub 验证仓库 `actr-mfr-verify` → `actr-publisher-verify` |
| Actor Registry | service registry | 类型 `ServiceRegistry` → `ActorRegistry`；SQLite 表 `service_registry` → `actor_registry`、列 `actor_manufacturer` → `actor_publisher`（启动时原地改名） |
| Realm Provider ID（Realm Provider 的 DNS authority） | `actrix-official` | 当前实现中固定的 provider ID 常量 |
| Realm 名 | 数值 `realm_id`、`actr:realm:<realm_id>:authority` | 当前实现中的 Realm 标识与 credential issuer 字符串 |
| Realm 签名 key、`realm_signing_keys` | 命名空间 key、`realm_namespace` | 本契约早期草稿用语 |
| Realm label、Realm 名 | `local_name`、Realm 引用、`dns:`/`local:` 前缀 | 本契约早期草稿用语 |
| Realm 签名 key 持有方 | 组织域名、托管域名、provider 自有命名空间、组织签名服务 | 本契约早期草稿用语（域名不再分类）；「组织签名服务」仅作为域名分类的说法退役，持有方运行签名服务签发 Actr 管理信封的机制仍在 |
| Actr 管理信封 | 签名信封 | 本契约早期草稿用语 |
| Realm 客户端 key | 设备 key | 本契约早期草稿用语 |
| Actor credential（含 Realm membership 证明） | Realm membership credential | 本契约早期草稿用语：并入 Actor credential，membership 即已注册未撤销的 Realm 客户端 key |
| Realm Provider | provider | 本契约早期草稿用语（泛词） |
