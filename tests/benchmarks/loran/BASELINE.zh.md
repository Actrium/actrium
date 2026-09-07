# Actrix Realm v1 性能基线

当前没有经过目标生产硬件、网络与数据规模验证的正式性能基线。历史文档中的固定
RPS/P99/内存数字已删除，避免把愿望写成证据。

发布候选必须记录：

- commit、Rust 版本、构建 profile；
- CPU、内存、磁盘、内核和网络；
- SQLite journal/synchronous 配置与数据规模；
- Signaling 并发连接、消息大小和发送率；
- TURN allocation、permission、relay bandwidth 和端口范围；
- AIS 注册/renewal 与 Signer 操作比例；
- P50/P95/P99、错误率、资源峰值和持续时间；
- 限流、连接洪泛、畸形流量和恢复后的行为。

结果只能与相同 fixture/环境的历史结果比较。任何生产容量声明必须链接原始数据和
可复现命令。
