# 历史安全设计说明

本文件曾描述旧版 Admin/Issuer Signer/Publisher Registry 的安全设想，现已被
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)替代。

旧文档中的以下结论不得再用于发布判断：

- 「只在内网所以可接受」；
- 可选 TLS 或固定默认 PSK；
- Signaling/TURN 与 Realm Issuer 没有 credential scope；
- 一个实例隐式承载多个 Realm（多 Realm 本身没有被否定，被否定的是没有按 Realm
  分区、没有显式隔离与校验的隐式承载）；
- Publisher Registry/GitHub 身份作为 Publisher 信任根；
- 只修复单个 P0 即「适合内部生产」。

Realm v1 的安全基线是 fail closed 配置、按 Realm 显式分区与隔离（当前实现仍以单
Realm 数据库约束代替，见架构契约第 9 节）、以 Realm 签名 key 为根的 Realm Profile、
短期 scoped Actor credential、Publisher 域名发现与签名、连接 ACL 默认拒绝、日志脱敏、有界资源、显式
迁移和 key 生命周期。生产声明还必须完成独立安全审查或渗透测试，并关闭全部
P0/P1。

保留本文件只为解释历史 issue/commit 中的引用；新设计、评审和测试不得在这里继续
追加。
