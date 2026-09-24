# Loran Realm v1 验收证据

状态：实现与自动验证清单，不是独立安全认证。评审者必须在报告中记录实际审查的
commit；本文件引用的 PR 状态会继续变化。

说明：下表证据针对当前实现（一个实例一个 Realm）。架构契约已改为一个实例承载多个
Realm（Realm 名 `<realm_id>@<domain>`，见 `REALM_V1_ARCHITECTURE.zh.md`）。标注「已被新契约取代」
的行保留为当前代码的证据，但不再是 v1 目标；新契约的验收项列在「多 Realm 契约待验收」
一节，尚无自动证据。

## 目标到证据的映射

| 目标 | 实现边界 | 自动证据 | 状态 |
|---|---|---|---|
| 单 Realm per instance | `crates/loran-platform/src/storage/db.rs` 的 singleton Realm 约束；Admin API 不提供在线硬删除/替换 | platform Realm/schema tests；full-stack 非预置 Realm 拒绝测试 | 已自动验证；已被新契约取代 |
| 单节点、SQLite 优先 | v1 默认只依赖 SQLite；worker ID 由管理员显式配置；不声明 HA | 配置验证、MSRV 全构建、SQLite schema gate/migration tests | 已自动验证 |
| 系统管理员是管理根 | Argon2id Admin 登录、受保护 Realm/Publisher/rotation API；Guest 不持有注册能力 | Admin API/config negative tests | 已自动验证；「唯一管理根」部分已被新契约的三类管理权限取代 |
| Realm Descriptor | `/.well-known/actr/realm.json` 发布唯一 Realm、issuer、Realm Provider 和 endpoint | descriptor/config/full-stack tests | 已自动验证；已被新契约取代（该路径与 authority-discovery 冲突，改为 Realm 签名 key 签名的 Realm Profile） |
| HTTPS 域名 Publisher | 规范化域名、WebPKI、DNS pinning、禁 redirect/私网地址；Ed25519 key、digest、rotation、revocation | Publisher Registry（`crates/loran-publisher-registry`）publisher/manager/package tests | 已自动验证 |
| Actor 实例身份 | Realm 名 + Snowflake serial（类型在 credential）；稳定 Service 通过 renewal possession proof 保持 ID | Issuer（`crates/loran-issuer`）issuer/renewal tests、已签发身份登记持久化 tests | 已自动验证 |
| Actor credential 生命周期 | version、issuer、Realm、Realm Provider、audience、Actor、endpoint 和 expiry 绑定；连接 source 再绑定 | Issuer Signer/Issuer/Signaling negative tests；过期、错 Realm、错 source、错连接回归 | 已自动验证 |
| Issuer/Rendezvous/Relay/Management 平面 | 逻辑边界分离；v1 all-in-one profile 同进程组合；Issuer Signer 可用 TLS/mTLS 拆出 | 配置组合测试、各 crate 测试、架构文档 | 已自动验证 |
| Service 注册/注销持久性 | 注册先写 SQLite 再激活；重启恢复项仅是离线元数据；原 Actor 重连后激活；注销先原子删除再成功 | `actor_registry_persists_across_restart`、`service_unregister_success_is_durable_across_restart`、Signaling 单元测试 | 已自动验证 |
| 不兼容升级 | 旧配置、schema、credential 和客户端不走长期双轨；未知 schema fail closed | `scripts/loran/migrate_schema_v1.py`、`migrate_schema_v2.py` 及其迁移测试；独立必需 CI Gate | 已自动验证 |
| TURN/STUN | per-Realm/Realm Provider TURN credential、expiry、bounded cache/backpressure、allocation/permission/relay | UDP interop、畸形包洪泛、cache bound、Chromium relay-only DataChannel fixture | 本机自动验证；外部矩阵待办 |
| 依赖风险 | Rust 审计无 ignore；Admin UI high/critical 默认 fail closed；唯一 RSC-only 例外绑定版本与静态 SPA 不变量 | `cargo audit`、`pnpm run audit:dependencies`、锁文件和生产 bundle | 已自动验证；例外待独立复核 |
| 威胁模型与文档 | 资产、信任边界、P0/P1 控制、配置、API、运维和独立评审模板 | 文档索引、CI drift/config tests | 已提交 |
| 上游交付 | Draft PR [Actrium/actr#460](https://github.com/Actrium/actr/pull/460) | PR 的 CI Gate、Rust、MSRV、nextest、各语言 checks | 持续验证 |

## 可复现命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features --no-fail-fast
cargo test --doc --workspace --all-features

cargo test -p actrix --test actrix_fullstack
cargo test -p actrix-signaling
cargo test -p actrix-turn --test udp_interop -- --test-threads=1
cargo test -p actrix-stun -- --test-threads=1
python3 -m unittest scripts/loran/tests/test_migrate_schema_v1.py -v
python3 -m unittest scripts/loran/tests/test_migrate_schema_v2.py -v
(cd crates/loran-server/admin/web && pnpm run audit:dependencies && pnpm run build)
```

浏览器 relay-only fixture、外部 TURN/STUN 矩阵和原始证据保存方法见
`TURN_STUN_INTEROP_TESTING.zh.md`。

## 多 Realm 契约待验收

以下目标来自架构契约第 3、5、6 节，代码尚未实现，均无自动证据：

| 目标 | 需要的证据 | 状态 |
|---|---|---|
| 一个实例承载多个 Realm | 移除单例约束后的 schema tests；Realm 名唯一与 Realm Provider tombstone 不复用 | 未实现 |
| Realm 签名 key 授权创建 | 未签名、签名 key 不在 `realm_signing_keys` 轮换链中、他人域名的创建请求均被拒绝 | 未实现 |
| Realm Profile 激活与持续校验 | 只有委派到本 Realm Provider 的 Realm Profile 才激活；`provider.id` 改变、签名 key 被移除或 `sequence` 回滚时丢弃相应 Realm Profile 并挂起；退役 Realm Profile 触发退役事务 | 未实现 |
| authority descriptor 获取安全 | 重定向超过 5 次、非 HTTPS 跳转、`authority` 绑定不符被拒绝；每跳 SSRF/DNS rebinding、超限 body 负向测试 | 未实现 |
| Realm 管理请求 | `provider_id` 不符、过期、超过 5 分钟有效期、nonce 重放的请求被拒绝；同一请求方重复创建返回原 pending Realm，不同请求方同名或命中 active Realm/Realm Provider tombstone 返回冲突；`publish_profile` 首次提交验证签名 Realm Profile 后返回一次性 Realm 管理 token，之后只接受 sequence 更高的版本 | 未实现 |
| Realm Provider 持有 Realm 签名 key 时的自助创建 | Realm Provider 账号 OAuth 2.0 + PKCE 认证、按账号计配额、Realm 管理权绑定账号、登录后重签 Realm 管理 token | 未实现 |
| Publisher 签名注册 | 读取 descriptor `publisher` 成员，`register_publisher` 请求验签，拒绝非 `active` 的 Publisher Profile 签名 key | 未实现 |
| 跨 Realm 隔离 | 两个 Realm 使用相同 Actor 类型与相同 serial 候选时，credential、presence、TURN、连接 ACL 与管理接口互不相通 | 未实现 |
| issuer 与 ActorId 使用 Realm 名 | credential issuer 为 Realm 名；ActorId `<serial_hex>.<realm_id>@<domain>` 文本/wire 格式（不含类型、类型由 credential 携带）的正负向测试 | 已自动验证：actr-protocol `actor_id_text_vectors`、`actor_id_text_rejects_invalid`；validator `check_binds_claims_to_the_requested_realm`；Issuer `test_end_to_end_credential_flow`；直连 WebSocket `verify_credential_realm_or_issuer_mismatch_returns_none` |
| 管理作用域 | 单 Realm 的 Realm 管理 token 越权访问被拒绝且不泄露其他 Realm 是否存在；重签后旧 token 立即失效 | 未实现 |
| 按 Realm 派生 TURN secret | 不同 Realm 的 TURN credential 不能互用 | 未实现 |
| 配额 | 按域名与按 Realm 的配额与 pending TTL | 未实现 |

## 尚不能由作者自证关闭的门槛

以下项目未完成前，只能称为 production-oriented baseline：

1. 独立人员在固定 commit 上进行 trust-boundary review/渗透测试，并关闭全部 P0/P1；
2. 在目标部署网络上补齐 Firefox/Safari、原生 WebRTC client 和主流独立 TURN/STUN
   实现的互操作矩阵；
3. 完成持续资源耗尽、SQLite 备份/恢复/损坏、key rotation/revocation 运维演练；
4. 等待 GitHub 按新锁文件重算告警，并由非 Loran manifest 的 owner 完成剩余
   npm/Rust 告警处置；Loran 唯一例外及退出条件见
   `DEPENDENCY_RISK_REGISTER.zh.md`，仍需独立评审复核。

PostgreSQL Issuer Signer backend 及其需要外部数据库的 ignored tests 不属于 SQLite-first v1
默认生产 profile。硬件/Actr Host 相关 metrics smoke test 也不作为安全通过证据。更广泛的
发布者字段源码命名迁移（见[术语表](../../docs/GLOSSARY.zh.md)「历史名称对照」）属于全 SDK breaking change，
不能用 Loran 边界上的 publisher 规范化来掩盖。
