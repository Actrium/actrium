# Actrix Realm v1 术语

## Realm

一组 Authority、Actor identity、ACL、provider 和服务 endpoint 共享的安全域。v1 每个
Actrix 实例恰好一个 Realm。

## Authority / AIS

Realm 的 Actor Identity Service。验证注册证明，分配 Actor 实例 ID，并通过 Signer
签发短期 credential。AIS 不是用户目录，也不拥有 Workspace。

## Signer

持有 Realm signing key 的信任平面。公开文档和配置不再称其为 KS/Key Server。
Signer 没有下载私钥的 API。

## Actor ID

Actor 实例身份，包含 Realm、Snowflake serial 和出生程序类型。它不是 Workspace、
用户或可变服务别名。

## Publisher

程序发布者。ID 是规范化 DNS 域名；域名用于 HTTPS 发现，Ed25519 key 用于长期签名
验证。“Manufacturer/MFR”仍可能出现在内部历史 crate、SQLite 名称以及尚未迁移的
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
Realm/provider/audience/Actor/expiry 的短期 credential。

## Realm descriptor

`/.well-known/actr/realm.json` 发布的非秘密 v1 元数据，包括 Authority 和官方
provider endpoint。

## Publisher descriptor

`https://<publisher>/.well-known/actr/publisher.json` 发布的域名身份、verifying key
和允许的 registry/origin。

## Workspace

用户拥有的工作目录。它可被多个 Service Actor 使用，一个 Actor 也可使用多个
Workspace；Actor 删除不删除 Workspace。Workspace 不属于 Actrix Realm 基础设施。
