# Actrix Realm v1 验收证据

状态：实现与自动验证清单，不是独立安全认证。评审者必须在报告中记录实际审查的
commit；本文件引用的 PR 状态会继续变化。

## 目标到证据的映射

| 目标 | 实现边界 | 自动证据 | 状态 |
|---|---|---|---|
| 单 Realm per instance | `platform/src/storage/db.rs` 的 singleton Realm 约束；Admin API 不提供在线硬删除/替换 | platform Realm/schema tests；Actrix full-stack 非预置 Realm 拒绝测试 | 已自动验证 |
| 单节点、SQLite 优先 | v1 默认只依赖 SQLite；worker ID 由管理员显式配置；不声明 HA | 配置验证、MSRV 全构建、SQLite schema gate/migration tests | 已自动验证 |
| 系统管理员是管理根 | Argon2id Admin 登录、受保护 Realm/Publisher/rotation API；Guest 不持有注册能力 | Admin API/config negative tests | 已自动验证 |
| Realm Descriptor | `/.well-known/actr/realm.json` 发布唯一 Realm、issuer、provider 和 endpoint | descriptor/config/full-stack tests | 已自动验证 |
| HTTPS 域名 Publisher | 规范化域名、WebPKI、DNS pinning、禁 redirect/私网地址；Ed25519 key、digest、rotation、revocation | `actrix-mfr` publisher/manager/package tests | 已自动验证 |
| Actor 实例身份 | Realm + Snowflake serial + 出生 `ActrType`；稳定 Service 通过 renewal possession proof 保持 ID | AIS issuer/renewal tests、持久 issuance ledger tests | 已自动验证 |
| Credential 生命周期 | version、issuer、Realm、provider、audience、Actor、endpoint 和 expiry 绑定；连接 source 再绑定 | Signer/AIS/Signaling negative tests；过期、错 Realm、错 source、错连接回归 | 已自动验证 |
| Authority/Rendezvous/Relay/Management 平面 | 逻辑边界分离；v1 all-in-one profile 同进程组合；Signer 可用 TLS/mTLS 拆出 | 配置组合测试、各 crate 测试、架构文档 | 已自动验证 |
| Service 注册/注销持久性 | 注册先写 SQLite 再激活；重启恢复项仅是离线元数据；原 Actor 重连后激活；注销先原子删除再成功 | `service_registry_persists_across_restart`、`service_unregister_success_is_durable_across_restart`、Signaling 单元测试 | 已自动验证 |
| 不兼容升级 | 旧配置、schema、credential 和客户端不走长期双轨；未知 schema fail closed | `scripts/migrate_schema_v1.py` 及 3 个迁移测试；独立必需 CI Gate | 已自动验证 |
| TURN/STUN | per-Realm/provider credential、expiry、bounded cache/backpressure、allocation/permission/relay | UDP interop、畸形包洪泛、cache bound、Chromium relay-only DataChannel fixture | 本机自动验证；外部矩阵待办 |
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
python3 -m unittest actrix/scripts/tests/test_migrate_schema_v1.py -v
(cd actrix/crates/actrixd/admin/web && pnpm run audit:dependencies && pnpm run build)
```

浏览器 relay-only fixture、外部 TURN/STUN 矩阵和原始证据保存方法见
`TURN_STUN_INTEROP_TESTING.zh.md`。

## 尚不能由作者自证关闭的门槛

以下项目未完成前，只能称为 production-oriented baseline：

1. 独立人员在固定 commit 上进行 trust-boundary review/渗透测试，并关闭全部 P0/P1；
2. 在目标部署网络上补齐 Firefox/Safari、原生 WebRTC client 和主流独立 TURN/STUN
   实现的互操作矩阵；
3. 完成持续资源耗尽、SQLite 备份/恢复/损坏、key rotation/revocation 运维演练；
4. 等待 GitHub 按新锁文件重算告警，并由非 Actrix manifest 的 owner 完成剩余
   npm/Rust 告警处置；Actrix 唯一例外及退出条件见
   `DEPENDENCY_RISK_REGISTER.zh.md`，仍需独立评审复核。

PostgreSQL Signer backend 及其需要外部数据库的 ignored tests 不属于 SQLite-first v1
默认生产 profile。硬件/宿主相关 metrics smoke test 也不作为安全通过证据。更广泛的
`manufacturer` 源码命名迁移属于全 SDK breaking change，不能用 Actrix 边界上的
Publisher 规范化来掩盖。
