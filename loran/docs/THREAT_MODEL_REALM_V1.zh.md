# Actrix Realm v1 威胁模型

状态：代码审查基线，不等同于独立安全审计结论。本文按多 Realm 目标契约
（[REALM_V1_ARCHITECTURE.zh.md](REALM_V1_ARCHITECTURE.zh.md)）描述威胁与控制；当前
代码仍强制单 Realm，涉及命名空间、Profile 激活与多 Realm 分区的控制尚未实现，
见架构契约第 9 节。

## 资产与攻击者

关键资产包括 Signer 私钥（每个 Realm 一把 issuer key）、Realm registration secret、
TURN 主密钥与派生 secret、renewal secret、Admin root、Realm 管理 token、Actor
credential、publisher key/revocation 状态和 SQLite 完整性。命名空间 key 由命名空间
所有者持有，不是 Actrix 资产（provider 自有命名空间除外），但它的失陷直接影响
Actrix 承载的该命名空间下全部 Realm。

考虑互联网匿名攻击者、恶意 Actor、失陷 publisher、同一 provider 上其他 Realm 的
恶意管理员或 Actor、失陷或恶意的命名空间所有者、能读取普通日志的低权限用户、
同机非特权进程及错误配置管理员。不声称抵抗已取得 root、Signer 主机完全失陷或
TLS CA 系统性失陷的攻击者。

## 信任边界

- Management：唯一可改变 Realm、publisher、key 和配置状态的平面；实例运营者、命名空间
  签名请求与 Realm 管理 token 三类权限在此分别校验；
- Authority/Signer：签发身份，Signer 是最小私钥边界；
- Rendezvous：传递信令但不获得签发权；
- Relay：只持有其 Realm/provider 的 TURN HMAC secret；
- Publisher HTTPS：证明域名控制并发现长期验签 key；
- 命名空间 authority descriptor（`https://<domain>/.well-known/actr` 中的
  `realm_namespace`）与 Realm Profile：由命名空间所有者控制；Actrix 只消费并验签，
  即使托管 Profile 也不成为信任根；
- Realm 之间：同进程共享基础设施，隔离由按 Realm 分区的状态与校验显式保证；
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
| Publisher 验证 SSRF/DNS rebinding | 当前实现：固定 HTTPS path `/.well-known/actr/publisher.json`、禁 redirect、响应限长、拒绝非公网 IP、DNS pinning；目标契约：读取 authority descriptor `/.well-known/actr` 的 `publisher` 成员，重定向处理与 Realm 相同（最多 5 次、每跳 HTTPS、逐跳 SSRF 检查与 DNS pinning、始终校验原 `authority` 绑定），有效性以 `publisher` 成员签名为准；注册使用 Profile signing key 签名的 `register_publisher` 请求，公开文档中不放 challenge |
| ACL 更新中途失败产生半策略 | 目标 Actor type 的 ACL 在单一事务中整体替换；无规则默认拒绝 |
| ACL wildcard 覆盖不确定 | 精确 source Realm 优先于 wildcard；策略坐标唯一 |
| Guest workload 自行扩权 | Realm registration secret 只交给 Realm 管理员/可信 host；Guest 不得接触该能力 |
| 多 Realm 状态混淆 | presence、registry、TURN cache、renewal token、ACL 均按 Realm 分区；claims 绑定 Realm 引用；Discovery 只返回同 Realm；跨 Realm 混淆回归测试；未知 schema/status 均拒绝 |
| Realm 管理越权 | 命名空间签名请求只作用于本命名空间；Realm 管理 token 作用域为单个 Realm，服务端逐请求检查；错误信息不泄露其他 Realm 是否存在 |
| 抢注他人命名空间下的 Realm | 创建请求必须由命名空间 key 签名，Actrix 按 authority-discovery 获取 `realm_namespace` 验签；只有 Profile 验证通过、委派到本 provider 的 Realm 才转为 `active` |
| 获取命名空间描述/Profile 时 SSRF 或 DNS rebinding | 按 authority-discovery 最多跟随 5 次重定向，每跳必须 HTTPS 并始终用原 authority 校验 `authority` 绑定；每一跳执行 SSRF 检查：拒绝非公网 IP、逐跳 DNS pinning、响应限长 64 KiB |
| 未认证请求驱动 provider 外联或放大 | 验签前就要获取 descriptor，因此按来源与目标域名限流，并缓存获取结果（含失败） |
| 命名空间签名请求重放到其他 provider 或重复执行 | 签名信封含 `provider_id`（provider 的 DNS authority，不是各部署相同的常量），必须等于接收方；有效期不超过 5 分钟；provider 在有效期内记录 `nonce` 拒绝字节级重放；创建以 `(namespace, local_name, 请求方)` 幂等，不同请求方命中同名 pending、或命中同名 active/tombstone 一律冲突；payload 经 JCS 规范化并由 `active` 命名空间 key 签名 |
| Realm 管理 token 泄露或丢失 | token 只在激活响应中返回一次，provider 只存哈希；丢失或泄露时重新签发并使旧 token 立即失效：DNS 命名空间由命名空间 key 签名的 `reissue_admin_token` 请求完成，provider 自有命名空间由绑定的账号登录后完成 |
| provider 自有命名空间被滥用批量创建 | 自助创建以 provider 账号认证（OAuth 2.0 + PKCE），按账号计配额；账号注册与防滥用由 provider 决定。provider 可关联同一账号创建的实例（它本来就是签名方），实例成员看不到账号 |
| 命名空间 key 失陷 | 攻击者可签发新 Profile（替换 issuer key 或端点、退役 Realm）或创建新 Realm；已接受 Profile 的客户端会拒绝改换 `provider.id` 的 Profile；控制：命名空间所有者从 `keys` 中移除该 key（吊销其全部签名），并用 `active` key 重签受影响的 Profile；客户端与 Actrix 刷新 `realm_namespace` 后丢弃由该 key 签名的 Profile 及其 `sequence` 基线（包括攻击者发布的超大 `sequence`）。残余风险：首次接触且尚未见到新 `realm_namespace` 的客户端无法区分；该风险属于命名空间所有者的信任边界，Actrix 不能单独消除 |
| 命名空间 key 被无轮换链替换 | 视为身份变化：不做 `sequence` 比较，Actrix 挂起相关 Realm，客户端必须重新确认信任，确认后以新成员 `sequence` 为基线；不静默接受 |
| Profile 回滚或重放旧 Profile | 每个 Realm 的 `sequence` 单调递增，Actrix 与客户端拒绝低于已接受值的 Profile，相同 `sequence` 内容必须一致；Profile 不过期，客户端与 Actrix 定期刷新 |
| 回滚命名空间描述以恢复已撤换的 key | `realm_namespace` 带 `active` 命名空间 key 对整个成员（含 `sequence`）的 JCS 签名；已固定 key 后拒绝更低 `sequence` |
| Profile 不可用时降级到默认 provider | 取不到、验不过或已退役的 Profile 时，客户端不得改用 authority descriptor `services` 中的默认 provider 与该 Realm 通信；默认 provider 只作为创建新 Realm 的建议；攻击者即使阻断 Profile 获取也只能造成不可用，不能把连接引向未委派的 provider |
| provider 托管 Profile 时篡改或替换 | Profile 由命名空间 key 签名，托管方无法伪造；客户端与 Actrix 均以签名而非获取位置判定有效性 |
| 被篡改的 Profile 把 Realm 改指其他 provider | `provider.id` 在 Realm 生命周期内不可变，客户端与 Actrix 拒绝 `provider.id` 不同的新 Profile；Actrix 周期性重新获取 Profile，发现 `provider.id` 或 issuer key 不属于本 provider 时本地挂起 |
| Realm 退役后仍被本 provider 使用 | 周期性重新获取 Profile；`status = retired` 时执行退役事务，不再签发 credential |
| 大量 pending Realm 或单个命名空间挤占资源 | pending 有 TTL；按命名空间限制 Realm 数，按 Realm 限制连接、注册、signaling 消息与 TURN allocation/带宽 |
| Realm 删除后残留状态混入新 Realm | 退役在单一事务中撤销 credential、renewal token、presence、registry 与 ACL，退役 issuer key；`realm_id` 与 `(namespace, local_name)` 均不复用 |
| 共享 provider 或实例管理员关联同一设备 | 设备在每个 Realm 使用独立的设备 key，不跨 Realm 复用，无法据设备 key 关联同一设备加入的不同 Realm |
| TURN secret 在 Realm 之间串用 | TURN secret 由主密钥按 Realm 引用与 provider ID 经 HKDF 派生；username 绑定 Realm 引用与 Actor ID |
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
6. All-in-one 进程承载多个 Realm，被利用时影响该实例上的全部 Realm；首版以共享
   基础设施换取此风险，生产应把 Signer 放入独立安全边界。
7. 首次信任命名空间依赖 Web PKI 和当时 DNS；域名易主后，新所有者可以发布新的
   `realm_namespace`。已固定旧 key 的客户端会因缺少轮换链而拒绝，但首次接触的客户端
   无法区分。
8. provider 自有命名空间由 provider 运营方托管命名空间 key，该命名空间下 Realm 的
   信任根即 provider 运营方。
9. Realm Profile 不过期：从未见过更高 `sequence` 的客户端，可能接受带外交付（邀请、
   缓存）的旧 Profile，例如仍列有已轮换 issuer key 的版本。客户端建立会话时或至少每
   24 小时按 `realms` 位置刷新以缩小窗口。
10. Realm 与 provider 永久绑定：provider 停服即实例不可用，只能在新 provider 上新建
   实例并做应用层数据迁移。

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
