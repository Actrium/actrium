# Actrix Realm v1 服务与运维

## 服务矩阵

| 服务 | 传输 | 默认路径/端口来源 | 身份要求 |
|---|---|---|---|
| Realm descriptor | HTTPS | `/.well-known/actr/realm.json` | 公开、只含非秘密信息 |
| AIS | HTTPS + protobuf | `/ais` | Realm capability + workload proof |
| Signaling | WSS | `/signaling/ws` | Actor credential |
| Signer | gRPC/HTTP2 | `/signer.v1.Signer/*` | nonce-auth；建议内网或 mTLS |
| Publisher Registry | HTTPS | `/registry` | 查询公开；发布需 Publisher nonce/signature |
| Admin | HTTPS | `/admin`、`/admin/api` | Argon2id login + Bearer session |
| STUN | UDP | `bind.ice` | Binding 公开 |
| TURN | UDP | `bind.ice` + relay range | scoped 短期 TURN credential |

HTTP 服务在一个 listener 上组合。STUN/TURN 使用独立 UDP socket；TURN relay 会从配置
范围分配额外 UDP 端口。

## 生命周期

启动顺序的安全前提：

1. 解析配置并执行 fail-closed 校验；
2. 打开 SQLite，验证 `PRAGMA user_version = 1`；
3. 初始化 recording、监控与控制面；
4. 构建 Signer、AIS、Signaling、Publisher Registry 和 descriptor router；
5. 绑定 HTTPS 与 ICE listener；
6. 报告 readiness。

没有 provision Realm 时，进程可以运行以便管理员初始化，但 Realm descriptor/AIS
不会伪造默认 Realm。任何 schema 或生产安全校验失败都会在监听前退出。

关闭时先停止接受新请求，取消后台 renewal/cleanup task，关闭 ICE listener，再等待
已有 task 的有界退出。服务重启不得清空 Actor identity ledger 或 renewal 状态。

## 健康与 readiness

“进程存活”不等于“Realm 可用”。生产探针至少区分：

- listener/liveness；
- SQLite 可读写；
- 恰好一个 Active Realm；
- Signer 可签名/取 verifying key；
- AIS、Signaling 依赖就绪；
- ICE advertised endpoint 与 relay range 可用。

`/metrics` 和健康接口可能暴露部署拓扑，应使用 allowlist 或 htpasswd，避免直接公网
开放。

## 热重载

HTTP router、证书和部分配置支持 reload，但以下变更不应无停机热改：

- Realm 替换；
- SQLite schema；
- `worker_id`；
- TURN、renewal、Realm、Signer KEK 等 secret；
- relay port range 与公网网络拓扑。

这些变更需要维护窗口、备份、显式迁移/轮换和回滚验证。热重载不得让旧 router 的
后台任务继续无限运行，也不得重置 Signaling 身份状态。

## 备份与恢复

SQLite 备份必须使用 SQLite backup API 或停机一致性副本，不能只复制正在写入的
database 文件。恢复演练应验证：

- schema version；
- 唯一 Realm；
- Realm/Signer/renewal/TURN secret 是否与备份匹配；
- 已有 Service Actor 能以 possession proof 恢复同一 ID；
- Publisher 历史 key、revocation 和 package metadata 未丢失；
- identity ledger 不会重用 serial。

迁移脚本会自行创建备份，但这不能代替正式灾备。

## Key rotation 与 revocation

- Realm signing key：新旧 verifying key 有明确生效/撤销窗口，credential 期限不被
  tolerance 延长。
- Publisher key：先由 Registry 发 challenge，再通过 Publisher HTTPS descriptor
  证明新 key；正常退休 key 仍可验历史包，泄露 key 必须显式 revoke。
- TURN secret：轮换会影响未过期凭证，需协调 AIS 与 TURN 并观察 allocation。
- renewal secret：直接替换会破坏稳定 Actor ID 恢复，必须按迁移方案处理。
- Signer KEK：应通过受测 rewrap 流程，不允许把明文私钥作为中间状态落盘。

所有轮换与撤销动作必须进入 audit channel。

## 容量边界

v1 不承诺 HA。单节点容量取决于：

- Signaling 并发连接与 frame/message rate；
- TURN allocation、permission、bandwidth 与 relay ports；
- auth/credential cache 上限；
- SQLite 写入延迟、WAL/磁盘空间；
- Publisher descriptor 外联超时与大小上限。

在目标硬件和网络上压测后再设置限流。提高上限不能以移除边界或关闭默认拒绝为代价。

## 故障排查原则

1. 先检查配置校验和 schema version，不要让进程自动“修复”生产数据。
2. 对照 Realm descriptor 的实际 endpoint 与证书/SAN/防火墙。
3. credential 错误按 issuer、provider、audience、Realm、Actor、expiry 逐项核对。
4. Signaling 连接成功但无法发现时检查 ACL；v1 默认拒绝是预期。
5. STUN 成功而 WebRTC 失败时检查 TURN credential、relay range 和对称 NAT。
6. Publisher 失败时检查 DNS 规范化、公网解析、well-known body、key ID 和 nonce。
7. 日志中若出现 secret/credential，先按泄露事件处置，再修复日志。

互操作与滥用测试见
[`TURN_STUN_INTEROP_TESTING.zh.md`](TURN_STUN_INTEROP_TESTING.zh.md)，发布条件见
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
