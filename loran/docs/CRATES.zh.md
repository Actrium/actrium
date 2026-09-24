# Loran crate 导航

Loran 位于 Actr workspace 内，crate 版本与 workspace 同步。本文只说明稳定职责，
不复制易失真的函数签名或行号。Cargo package 名仍沿用 `actrix-` 前缀（见[术语表](../../docs/GLOSSARY.zh.md)「历史名称对照」）。

| 目录 | package | 职责 |
|---|---|---|
| `crates/loran-server` | `actrix`（binary） | 读取/验证配置，组合 listener、router、ICE 服务、管理面与关闭流程 |
| `crates/loran-platform` | `actrix-platform` | 配置、SQLite schema、Realm/连接 ACL、Actor credential validation、监控与公共基础设施 |
| `crates/loran-issuer` | `actrix-issuer` | Issuer：Actor serial 分配、注册证明、Actor/TURN credential、renewal |
| `crates/loran-signer` | `actrix-signer` | Issuer Signer：Realm issuer key 生命周期、KEK 加密存储、签名和验签 key 查询 |
| `crates/loran-signaling` | `actrix-signaling` | 认证 WebSocket、presence、Actor Registry 与发现、signaling envelope 转发 |
| `crates/loran-stun` | `actrix-stun` | RFC 5389 Binding 与 NAT 地址发现 |
| `crates/loran-turn` | `actrix-turn` | scoped long-term credential、allocation、permission、relay 与配额 |
| `crates/loran-publisher-registry` | `actrix-publisher-registry` | Publisher Registry：publisher 身份、Publisher 签名权限与发布审计 |
| `crates/loran-packages` | `loran-packages` | `.actr` 包托管（Realm Profile 的 `package.href` 可指向它）：按 SHA-256 内容寻址的静态下载，不解析 Target |
| `crates/loran-proto` | `actrix-proto` | 管理/控制面的 protobuf 契约 |
| `crates/loran-control` | `actrix-admin` | 管理控制服务实现 |
| `crates/loran-sdk` | `actrix-sdk` | Loran 组件的内部聚合导出 |

## 依赖方向

```text
loran-server
 ├─ loran-platform
 ├─ loran-issuer ── Issuer Signer client
 ├─ loran-signaling ── Issuer/Issuer Signer clients
 ├─ loran-stun
 ├─ loran-turn
 ├─ loran-publisher-registry
 └─ loran-control/loran-proto
```

`loran-server` 是 composition root。业务 crate 不应反向依赖 daemon，也不应通过全局默认值
偷偷构造 Realm、publisher 或 Actor credential scope。

## 关键不变量

### `loran-platform`

- SQLite `user_version` 必须等于 schema v3；
- 当前实现：数据库层强制每实例最多一个 Realm。目标契约：移除单例约束，改为
  Realm 名唯一（含 Realm Provider tombstone），所有按 Realm 分区的状态以完整 Realm 名
  `realm` 为键；
- Actor credential validation 检查 protocol version、issuer、Realm Provider、audience、Realm、
  Actor 和过期时间；目标契约下 issuer 即 Realm 名，且 Realm 必须为 `active`；
- 配置未知字段和生产不安全值 fail closed。

### `loran-issuer`（Issuer）

- ActorId 是实例身份；目标契约下不含程序类型，类型由注册与 Actor credential 携带；
- Actor serial 由显式 worker ID 的 Snowflake 候选生成，并在 SQLite 已签发身份登记中永久预留；
- package workload 必须有 publisher/package proof，linked workload 必须有授权证明；
- renewal 需要 possession proof，不能仅凭 ActorId 续签；
- Realm registration capability 不得下发给 Guest；
- 目标契约：已签发身份登记按 Realm 分区，只为 `active` Realm 签发。

### `loran-signer`（Issuer Signer）

- 私钥没有下载 API；
- `Sign` 认证 payload 绑定 key ID 和待签消息 digest；
- 生产 SQLite 私钥由独立 KEK 加密；
- tolerance 只影响历史验签/取 key 语义，不延长 Actor credential 本身期限。

### `loran-signaling`

- upgrade 身份、Actor credential 中的 Actor 和 envelope source 必须一致；
- Actor Registry 按 Realm 分区（SQLite 表 `actor_registry`，启动时由旧表 `service_registry` 原地改名）；
- 原生客户端用 headers，浏览器用限定 WebSocket subprotocol transport；
- 连接 ACL 默认拒绝；
- 帧大小、连接数、消息速率和内存状态有界。

### `loran-turn` / `loran-stun`

- STUN Binding 不要求身份，只返回观测到的 mapped address；
- TURN allocation 要求 scoped、短期 TURN credential；
- auth cache、allocation、permission 与 relay port 使用必须有界；
- 畸形 datagram 不得让 listener 失去服务能力。

### `loran-publisher-registry`（Publisher Registry）

- 公开身份只叫 publisher，ID 是规范化 DNS 域名；
- 当前实现：well-known descriptor 请求拒绝重定向、私网目标、超限 body 与 DNS rebinding，
  key rotation 需要 publisher 重新证明域名控制；目标契约见 REALM_V1_ARCHITECTURE 第 7 节；
- 发布 nonce 一次性使用；
- package origin 不成为信任根。

## 公开与内部命名

v1 公开 TOML（如 `[services.publisher_registry]`、`[services.issuer]`、
`[services.signer]`）、HTTP JSON、descriptor 和文档统一使用 `publisher`、`issuer`、
`signer`。SQLite 表名与 Cargo package 名属于内部实现，不是协议标识，不允许
出现在新的外部契约中。

## 测试入口

```bash
cargo test -p actrix-platform
cargo test -p actrix-issuer
cargo test -p actrix-signer
cargo test -p actrix-signaling
cargo test -p actrix-stun
cargo test -p actrix-turn -- --test-threads=1
cargo test -p actrix-publisher-registry
cargo test -p actrix --test actrix_fullstack
```

完整 workspace 仍须执行 `cargo test --workspace -- --test-threads=1`，crate 局部测试不能
替代跨协议/客户端兼容验证。
