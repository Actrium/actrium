# Loran Realm v1 威胁模型

状态：代码审查基线，不等同于独立安全审计结论。本文按多 Realm 架构契约
（[REALM_V1_ARCHITECTURE.zh.md](REALM_V1_ARCHITECTURE.zh.md)）描述威胁与控制；尚未
实现的控制见架构契约第 9 节，本文对应行已标注。

## 资产与攻击者

关键资产包括 Issuer Signer 私钥（每个 Realm 一把 Realm issuer key）、Realm registration
secret、TURN 主密钥与派生 secret、renewal secret、Admin root、Realm 管理 token、Actor
credential、publisher key/revocation 状态和 SQLite 完整性。Realm 签名 key 由 Realm 签名
key 持有方持有，不是 Loran 资产（本 Realm Provider 即持有方时除外），但它的失陷直接影响
Loran 承载的该 Realm 域名下全部 Realm。

考虑互联网匿名攻击者、恶意 Actor、失陷 publisher、同一 Realm Provider 上其他 Realm
的恶意管理员或 Actor、失陷或恶意的 Realm 签名 key 持有方、能读取普通日志的低权限用户、
同机非特权进程及错误配置管理员。不声称抵抗已取得 root、Issuer Signer 主机完全失陷或
TLS CA 系统性失陷的攻击者。

## 信任边界

- Management：唯一可改变 Realm、publisher、key 和配置状态的平面；实例运营者、Realm
  管理请求与 Realm 管理 token 三类权限在此分别校验；
- Issuer/Issuer Signer：签发身份，Issuer Signer 是最小私钥边界；
- Rendezvous：传递信令但不获得签发权；
- Relay：只持有其 Realm/Realm Provider 的 TURN HMAC secret；
- Publisher HTTPS：证明域名控制并发现长期验签 key；
- 域名的 authority descriptor（`https://<domain>/.well-known/actr` 中的
  `realm_signing_keys`）与 Realm Profile：由 Realm 签名 key 持有方控制；Loran 只消费并验签，
  即使存放 Realm Profile 也不成为信任根；
- Realm 之间：同进程共享基础设施，隔离由按 Realm 分区的状态与校验显式保证；
- SQLite：单节点持久状态，文件权限和备份介质属于部署边界。

同进程部署减少运维组件，但不消除逻辑边界。生产可把 Issuer Signer 置于独立进程并使用
TLS/mTLS；Management 即使共享 listener 也必须经过独立认证中间件。

## 主要威胁与控制

| 威胁 | 控制 |
| --- | --- |
| 默认/硬编码 secret | 配置拒绝占位符和短 secret；缺失时 fail closed；默认值不再是固定生产 secret |
| 明文 Admin 密码泄露 | 配置仅存 Argon2id PHC hash；登录使用 Argon2id verifier |
| Sign 请求 key/message 被替换 | HMAC 覆盖 version、key_id 和 message digest；HTTP path/body key_id 必须一致 |
| 过期 key 继续签名 | Issuer Signer 与 Issuer 均以 `expires_at` 截止签名；tolerance 仅用于验签 |
| Actor credential 跨用途/跨 Realm Provider 重放 | claims 绑定 protocol/issuer/Realm/Realm Provider/audience；signaling 强校验 |
| signaling 冒充 source Actor | claims.actor_id 必须等于消息 source 和连接绑定 Actor |
| Actor credential 泄露到 URL/日志 | 原生 WebSocket 使用 Authorization header；浏览器使用只回选 `actr.v1` 的 subprotocol transport；trace 只记录 path，不记录 query |
| 无界队列/任务 DoS | signaling bounded channel、正确分钟配额、1 MiB WS 限制；STUN inline backpressure |
| TURN cache 延长过期或串 secret | cache 属于 authenticator 实例，key 使用完整 username+realm，命中前检查 expiry 与 Realm 是否可用 |
| TURN relay 端口错误/泄露 | 配置验证 1024..65535 且 start<=end；宣告地址与实际 relay 范围分离 |
| Publisher 验证 SSRF/DNS rebinding | 读取 authority descriptor `/.well-known/actr` 的 `publisher` 成员，重定向处理与 Realm 相同（最多 5 次、每跳 HTTPS、逐跳 SSRF 检查与 DNS pinning、64 KiB、始终校验原 `authority` 绑定），有效性以 `publisher` 成员签名为准；注册使用 Publisher Profile 签名 key 签名的 `register_publisher` 请求（Actr 管理信封，`provider_id`、5 分钟有效期、nonce），公开文档中不放 challenge；首个 Publisher Profile 固定签名 key，之后 `sequence` 只增 |
| 连接 ACL 更新中途失败产生半策略 | 目标 Actor type 的连接 ACL 在单一事务中整体替换；无规则默认拒绝 |
| 连接 ACL wildcard 覆盖不确定 | 精确 source Realm 优先于 wildcard；策略坐标唯一 |
| Guest workload 自行扩权 | Realm registration secret 只交给 Realm 管理员/可信 Actr Host；Guest 不得接触该能力 |
| 多 Realm 状态混淆 | presence、Actor Registry、TURN cache、renewal token、连接 ACL 均按 Realm 分区；claims 绑定 Realm 名；Discovery 只返回同 Realm；跨 Realm 混淆回归测试；未知 schema/status 均拒绝 |
| Realm 管理越权 | Realm 管理请求只作用于签名 key 所属 Realm 域名；Realm 管理 token 作用域为单个 Realm，服务端逐请求检查；错误信息不泄露其他 Realm 是否存在 |
| 抢注他人域名下的 Realm | 创建请求必须经 Realm 签名 key 持有方授权（Actr 管理信封由 `realm_signing_keys` 中的 key 签名，Loran 按 authority-discovery 获取验签）；只有 Realm Profile 验证通过、委派到本 Realm Provider 的 Realm 才转为 `active` |
| 获取 authority descriptor/Realm Profile 时 SSRF 或 DNS rebinding | 按 authority-discovery 最多跟随 5 次重定向，每跳必须 HTTPS 并始终用原 authority 校验 `authority` 绑定；每一跳执行 SSRF 检查：拒绝非公网 IP、逐跳 DNS pinning、响应限长 64 KiB |
| 未认证请求驱动 Realm Provider 外联或放大 | 验签前就要获取 descriptor，因此按来源与目标域名限流，并缓存获取结果（含失败） |
| Realm 管理请求重放到其他 Realm Provider 或重复执行 | Actr 管理信封含 `provider_id`（Realm Provider 的 DNS authority，不是各部署相同的常量），必须等于接收方；有效期不超过 5 分钟；Realm Provider 在有效期内记录 `nonce` 拒绝字节级重放；创建以 `(realm, 请求方)` 幂等，不同请求方命中同名 pending、或命中同名 active Realm/Realm Provider tombstone 一律冲突；payload 经 JCS 规范化并由 `active` Realm 签名 key 签名 |
| Realm 管理 token 泄露或丢失 | Realm 管理 token 只返回一次（激活响应；Realm Provider 域名下的 Realm 为 `create_realm` 响应），Realm Provider 只存哈希；丢失或泄露时重新签发并使旧 token 立即失效：持有方自己签名时以 Actr 管理信封发出 `reissue_admin_token`，Realm Provider 域名下的 Realm 由绑定的 Realm Provider 账号登录后完成 |
| Realm Provider 分配的域名被滥用批量创建 | 目前由实例运营者经 Admin API（Argon2id/JWT 认证）代办创建；按 `[realm]` 配额限制（`max_realms` 默认不限、`max_realms_per_domain` 默认不限、`max_pending_per_domain` 默认 10、`pending_ttl_secs` 默认 3600 秒）。Realm Provider 账号（OAuth 2.0 + PKCE）自助创建与按账号计配额尚未实现，见架构契约第 9 节 |
| Realm 签名 key 失陷 | 攻击者可签发新 Realm Profile（替换 Realm issuer key 或端点、退役 Realm）或创建新 Realm；已接受 Realm Profile 的客户端会拒绝改换 `provider.id` 的 Realm Profile；控制：持有方从 `keys` 中移除该 key（吊销其全部签名），并用 `active` key 重签受影响的 Realm Profile；客户端与 Loran 刷新 `realm_signing_keys` 后丢弃由该 key 签名的 Realm Profile 及其 `sequence` 基线（包括攻击者发布的超大 `sequence`）。残余风险：首次接触且尚未见到新 `realm_signing_keys` 的客户端无法区分；该风险属于 Realm 签名 key 持有方的信任边界，Loran 不能单独消除 |
| Realm 签名 key 被无轮换链替换 | 视为身份变化：不做 `sequence` 比较，Loran 挂起相关 Realm，客户端必须重新确认信任，确认后以新成员 `sequence` 为基线；不静默接受 |
| Realm Profile 回滚或重放旧 Realm Profile | 每个 Realm 的 Realm Profile sequence 单调递增，Loran 与客户端拒绝低于已接受值的 Realm Profile，相同 `sequence` 内容必须一致；Realm Profile 不过期，客户端与 Loran 定期刷新 |
| 回滚 `realm_signing_keys` 以恢复已撤换的 key | `realm_signing_keys` 带 `active` Realm 签名 key 对整个成员（含 `sequence`）的 JCS 签名；已固定 key 后拒绝更低 `sequence` |
| Realm Profile 不可用时降级到其他 Realm Provider | 取不到、验不过或已退役的 Realm Profile 时，客户端不得使用 Realm Profile 以外的任何 Realm Provider（包括 authority descriptor `services` 中的服务）与该 Realm 通信；攻击者即使阻断 Realm Profile 获取也只能造成不可用，不能把连接引向未委派的 Realm Provider |
| 存放 Realm Profile 的一方篡改或替换 | Realm Profile 由 Realm 签名 key 签名，存放方无法伪造；客户端与 Loran 均以签名而非获取位置判定有效性 |
| 被篡改的 Realm Profile 把 Realm 改指其他 Realm Provider | `provider.id` 在 Realm 生命周期内不可变，客户端与 Loran 拒绝 `provider.id` 不同的新 Realm Profile；Loran 周期性重新获取 Realm Profile，发现 `provider.id` 或 Realm issuer key 不属于本 Realm Provider 时本地挂起 |
| Realm 退役后仍被本 Realm Provider 使用 | 周期性重新获取 Realm Profile；`status = retired` 时执行退役事务，不再签发 Actor credential |
| 大量 pending Realm 或单个 Realm 域名挤占资源 | pending Realm 有 TTL；按 Realm 域名限制 Realm 数，按 Realm 限制连接、注册、signaling 消息与 TURN allocation/带宽 |
| Realm 删除后残留状态混入新 Realm | 退役在单一事务中撤销 Actor credential、renewal token、presence、Actor Registry 记录与连接 ACL，退役 Realm issuer key；Realm 名不复用 |
| 共享 Realm Provider 或实例管理员关联同一客户端 | 每个客户端安装在每个 Realm 使用独立的 Realm 客户端 key，不跨 Realm 复用，无法据 Realm 客户端 key 关联同一客户端加入的不同 Realm |
| TURN secret 在 Realm 之间串用 | TURN secret 由主密钥按 Realm 名与 Realm Provider ID 经 HKDF 派生；username 绑定 Realm 名与 ActorId |
| 续期/重签响应丢失后重试造成重复签发或身份失效 | `/renew`、`/reissue` 以客户端 `attempt_id` 幂等：状态变化与加密响应同一事务提交，同一 attempt 原样重放（重放前重查撤销），请求体不同 `409`、超过保留期 `410`、处理中 `425`；持有证明只能成功 reissue 一次 |
| Snowflake 重启/时钟回拨碰撞 | 显式 worker ID；SQLite 永久已签发身份登记以主键拒绝复用并重试 |
| Actor Registry 注册成功后立即重启丢记录 | Signaling 在 SQLite 写入成功后才完成 Actor Registry 注册；恢复项仅是离线元数据，原 Actor 通过认证重连后才重新进入发现/路由 |
| Actor Registry 注销成功后重启又复活 | Actor/Actor credential/连接三者匹配；SQLite 原子删除完成后才返回成功，失败时保留连接和内存注册 |
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
6. All-in-one 进程承载多个 Realm，被利用时影响该实例上的全部 Realm；首版以共享
   基础设施换取此风险，生产应把 Issuer Signer 放入独立安全边界。
7. 首次信任 Realm 域名的 Realm 签名 key 依赖 Web PKI 和当时 DNS；域名易主后，新所有者可以
   发布新的 `realm_signing_keys`。已固定旧 key 的客户端会因缺少轮换链而拒绝，但首次接触的客户端
   无法区分。
8. Realm Provider 持有 Realm 签名 key 时（其自有域名下分配的 Realm，或受托代持），该域名
   下 Realm 的信任根即该 Realm Provider。客户端只能按可判定规则（Realm 域名等于或属于
   Realm Provider ID）显示这一点；域名所有者委托 Realm Provider 代持时，没有公开字段声明，
   客户端仍显示信任根为 Realm 域名。这是已接受的限制。
9. Realm Profile 不过期：从未见过更高 `sequence` 的客户端，可能接受带外交付（Realm 邀请、
   缓存）的旧 Realm Profile，例如仍列有已轮换 Realm issuer key 的版本。客户端建立会话时或至少每
   24 小时按 `realms` 位置刷新以缩小窗口。
10. Realm 与 Realm Provider 永久绑定：Realm Provider 停服即实例不可用，只能在新 Realm
   Provider 上新建实例并做应用层数据迁移。

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
