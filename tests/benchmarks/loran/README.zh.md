# Actrix Realm v1 基准测试

本目录用于保存可复现的容量证据，不预设“应达到”的营销数字。

至少覆盖：

1. Signer：并发签名、verifying-key 查询、SQLite/KEK 成本；
2. AIS：新实例注册、稳定实例 renewal、拒绝路径；
3. Signaling：连接/断线重连、presence、不同 frame 大小与消息率；
4. STUN：Binding PPS 与畸形 datagram；
5. TURN：allocation/permission、双向 relay、带宽、端口耗尽与 auth cache；
6. SQLite：增长、checkpoint、备份、恢复和磁盘压力。

每个结果必须注明 commit、命令、fixture、环境、原始输出与结论。当前发布门槛见
[`../docs/THREAT_MODEL_REALM_V1.zh.md`](../docs/THREAT_MODEL_REALM_V1.zh.md)。
