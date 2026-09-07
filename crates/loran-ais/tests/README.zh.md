# AIS 测试

集成测试在进程内启动临时 Signer，并使用独立 SQLite 数据库验证：

- 单 Realm 注册；
- Publisher/package 与 linked workload proof；
- Actor ID 分配和 collision ledger；
- credential issuer/provider/audience/expiry；
- renewal possession、rotation、replay 与 revocation；
- Signer 不可用和非法配置的 fail-closed 行为。

这些测试不依赖外部 Signer，也不能替代独立安全审查。
