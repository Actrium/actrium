# Actrix Realm v1 术语

## Realm

一次应用实例部署的信任与治理域：Actor identity、ACL、成员与设备准入、issuer 和
provider 委派都以它为边界。标识为命名空间 + 本地名。Realm 不是应用（程序），也不是
Actrix 实例；目标契约下一个 Actrix 实例承载多个 Realm（当前实现仍恰好一个）。

## Realm 命名空间

Realm 的命名空间：`dns:<domain>`（域名所有者）或 `local:<installation-id>`（某个
Actro 安装，`installation-id` 由安装根 key 派生；本机 Realm 只有本地 record，不使用
Profile）。DNS 命名空间就是不含端口的 Actr authority。一个命名空间下可以有多个
Realm。

## 命名空间 key

声明在命名空间描述中的 Ed25519 key，是该命名空间下所有 Realm 的长期信任根，支持
多把与轮换链。由命名空间所有者持有；Actrix 不持有第三方命名空间 key。

## 命名空间描述

DNS 命名空间域名的 Actr authority descriptor（`https://<domain>/.well-known/actr`）中
的 `realm_namespace` 成员与 `realms` service relation，已在 Actr Authority Discovery
§4.3–4.4 定义。

Realm 命名空间与 linked workload 的 namespace（以及 SDK 中的 `manufacturer`）不是同一
概念：后者是宿主管理的、非 DNS 的程序类型命名空间，出现在 Actor 的出生类型中，用来
标识未经 Publisher Registry 的 linked/local workload；Realm 命名空间出现在 Actor ID 的
Realm 引用中，用来标识实例部署的归属。两者可以碰巧同名，但互不推导。

## 本地名（local_name）

Realm 在命名空间内的名字，DNS label 规则，创建后不可修改；退役后永不复用。

## Realm 引用

Realm 的文本形式：`<local_name>~dns:<domain>` 或 `<local_name>~local:<installation-id>`，
例如 `notes~dns:acme.com`。

## realm_id

Actrix 内部的数值 Realm 句柄（数据库主键）。目标契约下它不是身份，不出现在对外
issuer、Profile 或 ActorId 中；当前实现仍把它用在 issuer 与 Actor ID 里。

## Authority / AIS

每个 Realm 的 Actor Identity Service（一个 AIS 服务承载多个 Realm）。验证注册证明，分配 Actor 实例 ID，并通过 Signer
签发短期 credential。AIS 不是用户目录，也不拥有 Workspace。

## Signer

持有 Realm signing key（每个 Realm 一把 issuer key）的信任平面。公开文档和配置不再称其为 KS/Key Server。
Signer 没有下载私钥的 API。

## Actor ID

Actor 实例身份，包含 Realm 引用（当前实现为数值 realm_id）、Snowflake serial 和出生程序类型。它不是 Workspace、
用户或可变服务别名。

## Publisher

程序发布者。ID 是规范化 DNS 域名；域名用于 HTTPS 发现，Ed25519 key 用于长期签名
验证。「Manufacturer/MFR」仍可能出现在内部历史 crate、SQLite 名称以及尚未迁移的
Actr SDK/authoring-config 源码字段中，但不是 v1 的 Actrix HTTP/JSON、包清单、Actor ID
公开术语。

## Program identity

`publisher/program@version`。它表示不可变的程序版本，不表示某个 Actor 实例。

## Rendezvous / Signaling

帮助已认证 Actor 发现对方和交换 WebRTC 建连信息的服务。它不签发身份，不是
通信内容的最终信任根。

## STUN

Session Traversal Utilities for NAT。公开 Binding 服务让客户端得知服务端观测到的
网络地址。

## TURN

Traversal Using Relays around NAT。直连失败时中继流量；allocation 需要绑定
Realm/provider/audience/Actor/expiry 的短期 credential。配置中的 `[turn].realm` 是
TURN 认证域字符串（TURN REALM），与 Actr Realm 不是一回事。

## Realm Profile

每个 DNS 命名空间下 Realm 一份的 `actr.realm/v1` 元数据，由命名空间 key 签名，委派
issuer key 与 provider 端点，位于 `<realms href>/<local_name>`。托管 Profile 的
provider 不成为信任根。Profile 不过期，回滚保护只靠 `sequence`；`provider.id` 在 Realm
生命周期内不可变。

## provider 账号

用户在 provider 自有命名空间自助创建 Realm 时向该 provider 认证所用的账号，经 OAuth 2.0
授权码 + PKCE 登录（入口为 descriptor 的 `identity` service link）。配额按账号计，Realm
管理权绑定该账号，换机或丢设备只需重新登录。

## Realm descriptor（当前实现）

当前实现在 `/.well-known/actr/realm.json` 发布的单 Realm 非秘密元数据。目标契约中
由 Realm Profile 取代；该路径与 authority-discovery「`/.well-known/actr` 下不定义
子路径」冲突，将被移除。

## Publisher descriptor

Publisher 发布的域名身份、signing key 与程序解析信息。当前实现发布在
`https://<publisher>/.well-known/actr/publisher.json`（单 key、带 `verification_challenge`
的预发布结构）；目标契约中它是 authority descriptor `https://<publisher>/.well-known/actr`
的可选签名成员 `publisher`（schema `actr.publisher/v1`，snake_case，结构以产品规范为准），
旧路径与结构移除。

## Workspace

用户拥有的工作目录。它可被多个 Service Actor 使用，一个 Actor 也可使用多个
Workspace；Actor 删除不删除 Workspace。Workspace 不属于 Actrix Realm 基础设施。
