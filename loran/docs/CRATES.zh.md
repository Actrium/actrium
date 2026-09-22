# Actrix crate 导航

Actrix 位于 Actr workspace 内，crate 版本与 workspace 同步。本文只说明稳定职责，
不复制易失真的函数签名或行号。

| crate | package | 职责 |
|---|---|---|
| `actrixd` | `actrix` binary | 读取/验证配置，组合 listener、router、ICE 服务、管理面与关闭流程 |
| `platform` | `actrix-platform` | 配置、SQLite schema、Realm/ACL、credential validation、监控与公共基础设施 |
| `ais` | `actrix-ais` | Actor ID 分配、注册证明、identity/TURN credential、renewal |
| `signer` | `actrix-signer` | Realm signing key 生命周期、KEK 加密存储、签名和验签 key 查询 |
| `signaling` | `actrix-signaling` | 认证 WebSocket、presence、service discovery 与 signaling envelope 转发 |
| `stun` | `actrix-stun` | RFC 5389 Binding 与 NAT 地址发现 |
| `turn` | `actrix-turn` | scoped long-term credential、allocation、permission、relay 与配额 |
| `mfr` | `actrix-mfr` | Publisher Registry；`mfr` 仅为内部历史 crate 名 |
| `contracts` | `actrix-proto` | 管理/控制面的 protobuf 契约 |
| `control` | `actrix-control` | 管理控制服务实现 |
| `sdk` | `actrix-sdk` | Actrix 组件的内部聚合导出 |

## 依赖方向

```text
actrixd
 ├─ platform
 ├─ ais ── signer client
 ├─ signaling ── AIS/Signer clients
 ├─ stun
 ├─ turn
 ├─ actrix-mfr
 └─ control/contracts
```

`actrixd` 是 composition root。业务 crate 不应反向依赖 daemon，也不应通过全局默认值
偷偷构造 Realm、Publisher 或 credential scope。

## 关键不变量

### `platform`

- SQLite `user_version` 必须等于 schema v1；
- 当前实现：数据库层强制每实例最多一个 Realm。目标契约：移除单例约束，改为
  `(namespace, local_name)` 唯一（含 tombstone），所有按 Realm 分区的状态以内部
  `realm_id` 为外键；
- credential validation 检查 protocol version、issuer、provider、audience、Realm、
  Actor 和过期时间；目标契约下 issuer 带 Realm 引用，且 Realm 必须为 `active`；
- 配置未知字段和生产不安全值 fail closed。

### `ais`

- Actor ID 是实例身份；
- serial 由显式 worker ID 的 Snowflake 候选生成，并在 SQLite ledger 中永久预留；
- package workload 必须有 Publisher/package proof，linked workload 必须有授权证明；
- renewal 需要 possession proof，不能仅凭 Actor ID 续签；
- Realm registration capability 不得下发给 Guest；
- 目标契约：serial ledger 按 Realm 分区，只为 `active` Realm 签发。

### `signer`

- 私钥没有下载 API；
- `Sign` 认证 payload 绑定 key ID 和待签消息 digest；
- 生产 SQLite 私钥由独立 KEK 加密；
- tolerance 只影响历史验签/取 key 语义，不延长 credential 本身期限。

### `signaling`

- upgrade 身份、credential actor 和 envelope source 必须一致；
- 原生客户端用 headers，浏览器用限定 WebSocket subprotocol transport；
- ACL 默认拒绝；
- 帧大小、连接数、消息速率和内存状态有界。

### `turn` / `stun`

- STUN Binding 不要求身份，只返回观测到的 mapped address；
- TURN allocation 要求 scoped、短期 credential；
- auth cache、allocation、permission 与 relay port 使用必须有界；
- 畸形 datagram 不得让 listener 失去服务能力。

### Publisher Registry（内部 crate `mfr`）

- 公开身份只叫 Publisher，ID 是规范化 DNS 域名；
- well-known descriptor 请求拒绝重定向、私网目标、超限 body 与 DNS rebinding；
- key rotation 需要 Publisher 重新证明域名控制；
- 发布 nonce 一次性使用；
- package origin 不成为信任根。

## 公开与内部命名

v1 公开 TOML、HTTP JSON、descriptor 和文档统一使用 `publisher`、
`publisher_registry`、`signer`。SQLite 表名和 `actrix-mfr` crate 名属于内部历史
实现，不是协议标识，不允许出现在新的外部契约中。

## 测试入口

```bash
cargo test -p actrix-platform
cargo test -p actrix-ais
cargo test -p actrix-signer
cargo test -p actrix-signaling
cargo test -p actrix-stun
cargo test -p actrix-turn -- --test-threads=1
cargo test -p actrix-mfr
cargo test -p actrix --test actrix_fullstack
```

完整 workspace 仍须执行 `cargo test --workspace -- --test-threads=1`，crate 局部测试不能
替代跨协议/客户端兼容验证。
