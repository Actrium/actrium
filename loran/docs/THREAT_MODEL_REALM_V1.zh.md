# Actrix Realm v1 威胁模型

状态：代码审查基线，不等同于独立安全审计结论。

## 资产与攻击者

关键资产包括 Signer 私钥、Realm registration secret、TURN secret、renewal secret、
Admin root、Actor credential、publisher key/revocation 状态和 SQLite 完整性。

考虑互联网匿名攻击者、恶意 Actor、失陷 publisher、能读取普通日志的低权限用户、
同机非特权进程及错误配置管理员。不声称抵抗已取得 root、Signer 主机完全失陷或
TLS CA 系统性失陷的攻击者。

## 信任边界

- Management：唯一可改变 Realm、publisher、key 和配置状态的平面；
- Authority/Signer：签发身份，Signer 是最小私钥边界；
- Rendezvous：传递信令但不获得签发权；
- Relay：只持有其 Realm/provider 的 TURN HMAC secret；
- Publisher HTTPS：证明域名控制并发现长期验签 key；
- SQLite：单节点持久状态，文件权限和备份介质属于部署边界。

同进程部署减少运维组件，但不消除逻辑边界。生产可把 Signer 置于独立进程并使用
TLS/mTLS；Management 即使共享 listener 也必须经过独立认证中间件。

## 主要威胁与控制

| 威胁 | 控制 |
| --- | --- |
| 默认/硬编码 secret | 配置拒绝占位符和短 secret；缺失时 fail closed；默认值不再是固定生产 secret |
| 明文 Admin 密码泄露 | 配置仅存 Argon2id PHC hash；登录使用 Argon2id verifier |
| Sign 请求 key/message 被替换 | HMAC 覆盖 version、key_id 和 message digest；HTTP path/body key_id 必须一致 |
| 过期 key 继续签名 | Signer 与 AIS 均以 `expires_at` 截止签名；tolerance 仅用于验签 |
| credential 跨用途/跨 provider 重放 | claims 绑定 protocol/issuer/Realm/provider/audience；signaling 强校验 |
| signaling 冒充 source Actor | claims.actor_id 必须等于消息 source 和连接绑定 Actor |
| credential 泄露到 URL/日志 | 原生 WebSocket 使用 Authorization header；浏览器使用只回选 `actr.v1` 的 subprotocol transport；trace 只记录 path，不记录 query |
| 无界队列/任务 DoS | signaling bounded channel、正确分钟配额、1 MiB WS 限制；STUN inline backpressure |
| TURN cache 延长过期或串 secret | cache 属于 authenticator 实例，key 使用完整 username+realm，命中前检查 expiry |
| TURN relay 端口错误/泄露 | 配置验证 1024..65535 且 start<=end；宣告地址与实际 relay 范围分离 |
| Publisher 验证 SSRF/DNS rebinding | 固定 HTTPS path、禁 redirect、响应限长、拒绝非公网 IP、DNS pinning |
| ACL 更新中途失败产生半策略 | 目标 Actor type 的 ACL 在单一事务中整体替换；无规则默认拒绝 |
| ACL wildcard 覆盖不确定 | 精确 source Realm 优先于 wildcard；策略坐标唯一 |
| Guest workload 自行扩权 | Realm registration secret 只交给系统管理员/可信 host；Guest 不得接触该能力 |
| 多 Realm 状态混淆 | 数据库 singleton unique index；未知 schema/status 均拒绝 |
| Realm 删除后残留状态混入新 Realm | 在线管理 API 不提供 Realm 硬删除；重新 provision 必须停机并显式处理数据 |
| Snowflake 重启/时钟回拨碰撞 | 显式 worker ID；SQLite 永久身份 ledger 以主键拒绝复用并重试 |
| 服务注册成功后立即重启丢记录 | Signaling 在 SQLite 写入成功后才完成内部注册；恢复项仅是离线元数据，原 Actor 通过认证重连后才重新进入发现/路由 |
| 服务注销成功后重启又复活 | Actor/credential/连接三者匹配；SQLite 原子删除完成后才返回成功，失败时保留连接和内存注册 |
| 启动时隐式迁移破坏数据 | schema version 严格检查；独立离线脚本、备份、事务、FK/integrity check |
| 热重载丢失连接状态 | signaling runtime 在 router reload 间复用；改变 server/rate 语义的配置要求重启 |
| 降权失败后继续运行 | privilege drop 失败直接终止启动 |

## 残余风险

1. 单节点/SQLite 没有 HA；磁盘故障和节点故障需要外部备份恢复。
2. 5-bit worker ID 首版由管理员分配，尚无自动 lease；错误复用会破坏全局唯一性。
3. Admin 登录尚无内建 MFA、硬件 key 或账号级速率/锁定策略，应由反向代理和网络
   ACL 补强；后续可升级为 WebAuthn/签名挑战。
4. Publisher 域名控制验证依赖 WebPKI 和当时 DNS；长期真实性仍依赖已绑定 Ed25519
   key。key rotation/recovery 需要独立操作规程和审计。
5. 已完成本机 Chromium relay-only DataChannel、UDP TURN allocation/permission/
   双向 relay 和有界畸形 UDP 流量回归；仍需在其他浏览器、主流独立 TURN/STUN
   实现、目标 NAT 与移动网络矩阵中做持续互操作/滥用压测。
6. All-in-one 进程被利用时爆炸半径大于独立平面部署；首版以简单部署换取此风险。

## 发布门槛

标记为 production-ready 前必须满足：

- P0/P1 回归测试和全 workspace CI 通过；
- 在本地 Chromium fixture 之外，使用其他真实浏览器、原生 WebRTC client、主流
  TURN/STUN 实现和目标网络完成互操作及持续 abuse test；
- 完成配置、日志、备份恢复、key rotation/revocation 演练；
- 至少一次独立安全审查或渗透测试；
- README 的 production/performance 声明附有可复现证据。

在这些外部验证完成前，本实现只能称为 production-oriented baseline，不能把本威胁
模型当作安全认证。
