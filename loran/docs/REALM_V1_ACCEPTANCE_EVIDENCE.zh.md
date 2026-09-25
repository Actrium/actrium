# Loran Realm v1 验收证据

状态：实现与自动验证清单，不是独立安全认证。评审者必须在报告中记录实际审查的
commit；本文件引用的 PR 状态会继续变化。

说明：下表证据针对当前实现——一个 Loran 实例承载多个 Realm（Realm 名
`<realm_id>@<domain>`，见 `REALM_V1_ARCHITECTURE.zh.md`）。历史的单 Realm per instance
约束与 `/.well-known/actr/realm.json` 已被移除，行保留在下表作历史对照。尚未实现的项在
「多 Realm 契约验收」一节标注。

## 目标到证据的映射

| 目标 | 实现边界 | 自动证据 | 状态 |
|---|---|---|---|
| 一个实例承载多个 Realm | `crates/loran-platform/src/storage/db.rs`：`realm.name` 主键、`status` CHECK、`realm_name_immutable`/`realm_retired_is_final` 触发器；`Realm::insert`/`insert_requested` 按 `max_realms`/`max_realms_per_domain` 原子计数 | `fresh_database_is_versioned_and_hosts_many_realms`、`test_many_realms_and_max_realms`、`test_retired_name_is_never_reused`、`test_retire_removes_realm_scoped_state` | 已自动验证（取代历史的 singleton Realm 约束） |
| 单节点、SQLite 优先 | v1 默认只依赖 SQLite；worker ID 由管理员显式配置；不声明 HA | 配置验证、MSRV 全构建、SQLite schema gate/migration tests | 已自动验证 |
| 系统管理员是管理根 | Argon2id Admin 登录、受保护 Realm/Publisher/rotation API；Guest 不持有注册能力 | Admin API/config negative tests | 已自动验证；管理根现分实例运营者、Realm 管理请求、Realm 管理 token 三类权限，见下方「Realm 签名 key 授权创建」「管理作用域」 |
| Realm Profile（取代历史 Realm Descriptor） | `realm_signing_keys` 成员 pin 与验签（`crates/loran-platform/src/realm/domain.rs`）、`realm_profile` 存储与签发（`profile.rs`、`crates/loran-issuer/src/realm_profile.rs`）、`GET /actr/realms/{domain}/{realm_id}` | `pins_first_member_and_checks_successors`、`verifies_member_self_signature`、`stores_profiles_and_retires_with_the_retired_one`、fullstack `authority_descriptor_publishes_signed_realm_signing_keys`、`provider_domain_realm_profiles_are_signed_served_and_retired` | 已自动验证（历史 `/.well-known/actr/realm.json` 已移除） |
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

## 多 Realm 契约验收

以下目标来自架构契约第 3、5、6 节；除标注「未实现」外均已落地并有自动证据：

| 目标 | 需要的证据 | 状态 |
|---|---|---|
| Realm 签名 key 授权创建 | 未签名、签名 key 不在 `realm_signing_keys` pin 中、他人域名的创建请求均被拒绝 | 已自动验证：`crates/loran-platform/src/realm/domain.rs` `verifies_member_self_signature`、`reads_member_from_a_valid_descriptor_only`、`pins_first_member_and_checks_successors`、`follows_a_rotation_chain`；issuer `rejects_unauthorized_or_malformed_requests`；轮换链与 test vector：actr-protocol `tests/realm_signing_keys_rotation.rs`。身份变化的重新确认未实现，见架构契约第 9 节 |
| Realm issuer key 轮换 | 新 key 预发布满 24 小时才签发，旧 key 以 `retired` 列到最后 credential 过期 +5 分钟后移除；其他域名的 Realm 在持有方发布含新 key 的 Realm Profile 前不切换；轮换后 credential 带新 `key_id` 且新旧 key 均按 `(realm, key_id)` 验证 | 已自动验证：issuer `rotates_the_issuer_keys_of_provider_domain_realms`、`creates_activates_and_retires_other_domains_realms`（`draft_profile`）、`credentials_are_signed_with_the_realm_issuer_key`；actr-protocol `finds_issuer_keys_by_credential_key_id` |
| 包签名 key 重叠轮换 | Publisher Profile 至多 2 把 `active` 包签名 key，重叠期两把都可发布；切换后旧 key 只验旧包，移除即吊销；Publisher Profile 签名 key 跟随轮换链 | 已自动验证：publisher-registry `two_active_package_keys_overlap_during_rotation`、`register_publisher_pins_the_first_profile`；actr-protocol `publisher_profile_signing_keys_rotate_along_a_chain` |
| Realm Profile 激活与持续校验 | 只有委派到本 Realm Provider 的 Realm Profile 才激活；`provider.id` 改变、签名 key 被移除或 `sequence` 回滚时丢弃相应 Realm Profile 并挂起；退役 Realm Profile 触发退役事务 | 已自动验证：platform `stores_profiles_and_retires_with_the_retired_one`；issuer `signs_verifiable_profiles_for_the_provider_domain`、`refreshes_other_domains_profiles_under_the_key_pin`；fullstack `provider_domain_realm_profiles_are_signed_served_and_retired` |
| authority descriptor 获取安全 | 重定向超过 5 次、非 HTTPS 跳转、`authority` 绑定不符被拒绝；每跳 SSRF/DNS rebinding、超限 body 负向测试 | 已自动验证：`crates/loran-platform/src/net/fetch.rs` `rejects_private_addresses`、`rejects_unsafe_urls_before_connecting`、`follows_checked_redirects_and_limits_documents`、`caches_results_and_limits_each_host` |
| Realm 管理请求 | `provider_id` 不符、过期、超过 5 分钟有效期、nonce 重放的请求被拒绝；同一请求方重复创建返回原 pending Realm，不同请求方同名或命中 active Realm/Realm Provider tombstone 返回冲突；`publish_profile` 首次提交验证签名 Realm Profile 后返回一次性 Realm 管理 token，之后只接受 sequence 更高的版本 | 已自动验证：actr-protocol `signed_request_round_trips_and_verifies`、`rejects_unsigned_requests_and_bad_windows`；issuer `creates_activates_and_retires_other_domains_realms`、`rejects_unauthorized_or_malformed_requests`；fullstack `realm_management_requests_are_checked_before_any_state_changes` |
| Realm Provider 持有 Realm 签名 key 时的自助创建 | Realm Provider 账号 OAuth 2.0 + PKCE 认证、按账号计配额、Realm 管理权绑定账号、登录后重签 Realm 管理 token | 未实现：目前 Realm Provider 域名下的 Realm 由实例运营者经 Admin API 代办创建，见架构契约第 9 节 |
| Publisher 签名注册 | 读取 descriptor `publisher` 成员，`register_publisher` 请求验签，拒绝非 `active` 的 Publisher Profile 签名 key | 已自动验证：actr-protocol `register_publisher_names_the_publisher_instead_of_a_realm`；publisher-registry `register_publisher_pins_the_first_profile`、`register_publisher_never_reinstates_a_used_key`、`register_publisher_refuses_a_package_key_outside_its_validity`、`concurrent_register_publisher_requests_do_not_fail_internally`；issuer `registers_publishers_from_their_publisher_profile`、`register_publisher_accepts_only_publisher_profile_signing_keys`。周期刷新未实现，见架构契约第 9 节 |
| 跨 Realm 隔离 | 两个 Realm 使用相同 Actor 类型与相同 serial 候选时，credential、presence、TURN、连接 ACL 与管理接口互不相通 | 已自动验证：fullstack `realms_with_the_same_actor_type_and_serial_are_isolated`、`signaling_discovery_cross_realm_isolated`、`signaling_route_candidates_cross_realm_isolated`、`signaling_get_service_spec_cross_realm_isolated`、`signaling_relay_cross_realm_is_denied`；turn `realm_credentials_are_not_interchangeable` |
| issuer 与 ActorId 使用 Realm 名 | credential issuer 为 Realm 名；ActorId `<serial_hex>.<realm_id>@<domain>` 文本/wire 格式（不含类型、类型由 credential 携带）的正负向测试 | 已自动验证：actr-protocol `actor_id_text_vectors`、`actor_id_text_rejects_invalid`；validator `check_binds_claims_to_the_requested_realm`；Issuer `test_end_to_end_credential_flow`、`credentials_are_signed_with_the_realm_issuer_key`；直连 WebSocket `verify_credential_realm_or_issuer_mismatch_returns_none` |
| 退役清理 | Realm 退役后其 Actor Registry 记录（内存与缓存）、presence 订阅与在线连接被清除，并发注册/订阅被拒绝，旧 credential 不能再连接；配置热重载前建立的连接同样被关闭，与退役竞争的注册不留下 ACL 与待登记记录、续期与重签不提交；该 Realm 的 issuer key 在 Signer 中退役、不再签名；不可用 Realm 的 TURN credential 被拒绝 | 已自动验证：platform `test_state_changes_are_announced`；signer `a_retired_named_key_never_signs_again`；signaling `retiring_a_realm_drops_its_state_and_refuses_it_afterwards`、`a_replaced_server_still_closes_connections_of_retired_realms`；issuer `retiring_a_realm_retires_its_issuer_key`、`registration_state_is_not_kept_for_a_realm_retired_meanwhile`、`work_prepared_before_a_realm_retired_does_not_commit`；signer（postgres）`test_migration_keeps_retired_and_v1_rows`；turn `test_auth_handle_refuses_inactive_realm_even_when_cached`、`follows_suspension_resumption_and_retirement`；fullstack `retiring_a_realm_closes_its_connections_and_forgets_its_actors` |
| 注册不泄露 Realm 状态 | 未通过 Realm secret 的注册对未知、停用、退役 Realm 返回同一 `403` | 已自动验证：issuer `test_register_does_not_reveal_realm_state_before_authentication`；fullstack `issuer_register_rejects_non_preprovisioned_realm` |
| 续期/重签幂等 | 同一 `attempt_id` 重放首次响应且只变更一次状态；请求体不同 `409`、超过响应保留期 `410`、处理中 `425` 带 `Retry-After`；重放不绕过撤销、重放的拒绝不透露 Actor 状态；持有证明只能成功 reissue 一次 | 已自动验证：issuer `test_renew_route_replays_exact_response_and_single_state_change`、`test_renew_attempt_id_body_mismatch_returns_conflict`、`test_reissue_route_is_idempotent_across_lost_responses`、`test_reissue_attempt_id_body_mismatch_returns_conflict`、`test_reissue_replay_after_response_expiry_returns_gone`、`test_reissue_attempt_in_progress_returns_too_early`、`test_reissue_replay_requires_the_original_request`、`test_reissue_proof_is_consumed_by_one_attempt`、`test_renew_replay_rechecks_only_issued_credentials` |
| 管理作用域 | Realm 管理 token 越权访问被拒绝且不泄露其他 Realm 是否存在；重签后旧 token 立即失效 | 已自动验证：platform `admin_token_is_stored_hashed_and_reissue_replaces_it`；fullstack `admin_api_manages_many_realms_by_name` |
| 按 Realm 派生 TURN secret | 不同 Realm 的 TURN credential 不能互用 | 已自动验证：platform `each_realm_and_provider_gets_its_own_secret`；turn `realm_credentials_are_not_interchangeable` |
| 配额 | 按域名与按 Realm 的配额与 pending TTL | 已自动验证：platform `requested_realms_stay_within_domain_quotas`、`expired_pending_realms_are_discarded`；issuer `rejects_unauthorized_or_malformed_requests`（pending 配额 429）。按连接/消息/TURN 配额尚未实现，见架构契约第 9 节 |

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
