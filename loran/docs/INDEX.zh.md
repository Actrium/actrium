# Actrix 文档索引

## 建议阅读顺序

1. [`README`](../README.md)：产品边界、启动和发布门槛。
2. [`REALM_V1_ARCHITECTURE.zh.md`](REALM_V1_ARCHITECTURE.zh.md)：规范性架构契约。
3. [`THREAT_MODEL_REALM_V1.zh.md`](THREAT_MODEL_REALM_V1.zh.md)：资产、威胁、缓解与发布门槛。
4. [`CONFIGURATION.md`](CONFIGURATION.zh.md)：单节点生产配置。
5. [`API.md`](API.zh.md)：公开传输与路由边界。

## 开发与运维

- [`ARCHITECTURE.md`](ARCHITECTURE.zh.md)：架构和代码入口。
- [`CRATES.md`](CRATES.zh.md)：crate 职责与依赖方向。
- [`SERVICES.md`](SERVICES.zh.md)：服务生命周期、运维和恢复。
- [`DEVELOPMENT.md`](DEVELOPMENT.zh.md)：构建与贡献。
- [`CONFIG_EXAMPLES.md`](CONFIG_EXAMPLES.zh.md)：生产与本地 profile。
- [`KS_GUIDE.md`](KS_GUIDE.zh.md)：Signer 信任边界（文件名为历史名称）。
- [`TURN_STUN_INTEROP_TESTING.zh.md`](TURN_STUN_INTEROP_TESTING.zh.md)：ICE 验证。
- [`REALM_V1_ACCEPTANCE_EVIDENCE.zh.md`](REALM_V1_ACCEPTANCE_EVIDENCE.zh.md)：目标到自动证据和外部门槛的映射。
- [`DEPENDENCY_RISK_REGISTER.zh.md`](DEPENDENCY_RISK_REGISTER.zh.md)：依赖告警处置、例外和退出条件。
- [`INDEPENDENT_SECURITY_REVIEW.zh.md`](INDEPENDENT_SECURITY_REVIEW.zh.md)：独立评审范围与报告模板。
- [`prometheus-metrics.md`](prometheus-metrics.zh.md)：指标。
- [`GLOSSARY.md`](GLOSSARY.zh.md)：术语。

## 历史资料

以下文件用于保留问题背景，不是 Realm v1 的规范：

- `TODO-cross-realm.zh.md`
- `realm-flow-audit-2026-02-24.zh.md`
- `reissue-grace-threat-model.zh.md`
- `security-design.zh.md`
- `development/CLAUDE.md`

当历史文档与 Realm v1 架构、威胁模型或当前代码冲突时，以 v1 文档与代码测试为准。
