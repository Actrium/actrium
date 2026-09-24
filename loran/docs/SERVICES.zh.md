# Loran Realm v1 服务与运维

## 服务矩阵

| 服务 | 传输 | 默认路径/端口来源 | 身份要求 |
|---|---|---|---|
| Realm descriptor（当前实现） | HTTPS | `/.well-known/actr/realm.json` | 公开、只含非秘密信息；目标契约中移除，仅在 authority descriptor 的 `realms` relation 指向本 Realm Provider 时提供 Realm Profile |
| Issuer | HTTPS + protobuf | `/issuer` | Realm capability + workload proof |
| Signaling | WSS | `/signaling/ws` | Actor credential |
| Issuer Signer | gRPC/HTTP2 | `/signer.v1.Signer/*` | nonce-auth；建议内网或 mTLS |
| Publisher Registry | HTTPS | `/registry` | 查询公开；发布需 Publisher nonce/signature |
| Admin | HTTPS | `/admin`、`/admin/api` | Argon2id login + Bearer session |
| STUN | UDP | `bind.ice` | Binding 公开 |
| TURN | UDP | `bind.ice` + relay range | scoped 短期 TURN credential |

HTTP 服务在一个 listener 上组合。STUN/TURN 使用独立 UDP socket；TURN relay 会从配置
范围分配额外 UDP 端口。

## 生命周期

启动顺序的安全前提：

1. 解析配置并执行 fail-closed 校验；
2. 打开 SQLite，验证 `PRAGMA user_version = 4`；
3. 初始化 recording、监控与控制面；
4. 构建 Issuer Signer、Issuer、Signaling（含 Actor Registry）、Publisher Registry 和
   descriptor router；
5. 绑定 HTTPS 与 ICE listener；
6. 报告 readiness。

没有 provision Realm 时，进程可以运行以便管理员初始化，但 Realm descriptor/Issuer
不会伪造默认 Realm。目标契约下实例本身不需要 provision 任何 Realm；请求所属 Realm
不存在或不是 `active` 时一律拒绝。任何 schema 或生产安全校验失败都会在监听前退出。

关闭时先停止接受新请求，取消后台 renewal/cleanup task，关闭 ICE listener，再等待
已有 task 的有界退出。服务重启不得清空已签发 Actor 身份登记或 renewal 状态。

## 健康与 readiness

「进程存活」不等于「Realm 可用」。生产探针至少区分：

- listener/liveness；
- SQLite 可读写；
- 当前实现：恰好一个 Active Realm；目标契约：Realm 状态按 Realm 报告，单个 Realm
  挂起或未激活不影响实例整体 readiness；
- Issuer Signer 可签名/取 verifying key；
- Issuer、Signaling 依赖就绪；
- ICE advertised endpoint 与 relay range 可用。

`/metrics` 和健康接口可能暴露部署拓扑，应使用 allowlist 或 htpasswd，避免直接公网
开放。

## 热重载

HTTP router、证书和部分配置支持 reload，但以下变更不应无停机热改：

- Realm 替换（当前实现）；
- SQLite schema；
- `worker_id`；
- TURN、renewal、Realm、Issuer Signer KEK（`ACTRIX_SIGNER_KEK`）等 secret；
- relay port range 与公网网络拓扑。

这些变更需要维护窗口、备份、显式迁移/轮换和回滚验证。热重载不得让旧 router 的
后台任务继续无限运行，也不得重置 Signaling 身份状态。

## 备份与恢复

SQLite 备份必须使用 SQLite backup API 或停机一致性副本，不能只复制正在写入的
database 文件。恢复演练应验证：

- schema version；
- Realm 集合与状态（当前实现：唯一 Realm）；
- Realm/Issuer Signer/renewal/TURN secret 是否与备份匹配；
- 已有 Service 能以 possession proof 恢复同一 ID；
- Publisher 历史 key、revocation 和 package metadata 未丢失；
- 已签发身份登记不会重用 serial。

迁移脚本会自行创建备份，但这不能代替正式灾备。

## Key rotation 与 revocation

- Realm issuer key：每个 Realm 一把（Signer 中的 `issuer:<realm>`），不过期；轮换须由
  Realm 签名 key 签署新 Realm Profile 委派新 key（尚未实现）。Issuer 原有的轮换 key 不再签发
  Actor credential。
- Realm 签名 key：由 Realm 签名 key 持有方轮换，不在 Loran 内完成（持有方就是本 Realm
  Provider 时除外）；Loran 周期性重新获取 `realm_signing_keys`，Realm Profile 的签名 key 被
  移除时丢弃该 Realm Profile，找不到有效 Realm Profile 的 Realm 会被挂起；新 Realm Profile
  不再以 `active` 列出该 Realm 的 issuer key 时，Realm 同样被挂起，不再签发 Actor credential。
- Publisher key：当前实现先由 Publisher Registry 发 challenge，再通过 Publisher HTTPS
  descriptor 证明新 key；目标契约中由 Publisher 发布新的签名 Publisher Profile，Publisher
  Registry 周期刷新后启用；正常退休 key 仍可验历史包，泄露 key 必须显式 revoke。
- TURN secret：轮换会影响未过期凭证，需协调 Issuer 与 TURN 并观察 allocation。目标
  契约下它是 Realm Provider 主密钥，轮换会同时影响全部 Realm 的派生 secret。
- renewal secret：直接替换会破坏稳定 ActorId 恢复，必须按迁移方案处理。
- Issuer Signer KEK：应通过受测 rewrap 流程，不允许把明文私钥作为中间状态落盘。

所有轮换与撤销动作必须进入 audit channel。

## 容量边界

v1 不承诺 HA。单节点容量取决于：

- Signaling 并发连接与 frame/message rate；
- TURN allocation、permission、bandwidth 与 relay ports；
- auth/Actor credential cache 上限；
- SQLite 写入延迟、WAL/磁盘空间；
- Publisher Profile 外联超时与大小上限；
- 目标契约下承载的 Realm 数、authority descriptor/Realm Profile 周期性校验的外联量，以及
  按 Realm 域名与按 Realm 的配额。

在目标硬件和网络上压测后再设置限流。提高上限不能以移除边界或关闭默认拒绝为代价。

## 故障排查原则

1. 先检查配置校验和 schema version，不要让进程自动「修复」生产数据。
2. 对照 Realm descriptor（目标契约：Realm Profile）的实际 endpoint 与证书/SAN/防火墙。
3. Actor credential 错误按 issuer、Realm Provider、audience、Realm、Actor、expiry 逐项核对；目标
   契约下再核对 Realm 状态是否为 `active`。
4. Signaling 连接成功但无法发现时检查连接 ACL；v1 默认拒绝是预期。
5. STUN 成功而 WebRTC 失败时检查 TURN credential、relay range 和对称 NAT。
6. Publisher 失败时检查 DNS 规范化、公网解析、well-known body、key ID 和 nonce。
7. 日志中若出现 secret/Actor credential，先按泄露事件处置，再修复日志。

互操作与滥用测试见
[`TURN_STUN_INTEROP_TESTING.zh.md`](TURN_STUN_INTEROP_TESTING.zh.md)，发布条件见
[`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)。
