# 历史开发笔记

本文件原先记录的是旧版 Issuer Signer/Publisher Registry、隐式（未分区）多 Realm 和「内部使用安全」等结论，它们已不再是开发准则。

当前开发入口：

- [`../../README.md`](../../README.md)
- [`../REALM_V1_ARCHITECTURE.zh.md`](../REALM_V1_ARCHITECTURE.zh.md)
- [`../THREAT_MODEL_REALM_V1.zh.md`](../THREAT_MODEL_REALM_V1.zh.md)
- [`../DEVELOPMENT.md`](../DEVELOPMENT.md)
- [`../CRATES.md`](../CRATES.md)

任何改动都必须保持按 Realm 隔离（一个实例承载多个 Realm，每个 Realm 名为
`<label>@<domain>`；当前代码仍强制单 Realm，见架构契约第 9 节）、credential 作用域、
默认拒绝、日志中不出现 secret，以及离线显式迁移。不得借旧文档中的默认 key、明文密码、
可选认证或「仅限内网」假设恢复已移除的路径。
