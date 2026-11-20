# Loran 基础服务

**AFP（Actr Facility Provider，Actr 设施提供方）**泛指 Actr 生态中承担平台职责的各类提供方和运营方。平台职责包括提供基础设施、分配 Realm／应用实例等。

**Loran** 是提供部分 Actr 平台基础设施的示例软件。

| 服务 | 提供的能力 | 部署与依赖 |
| --- | --- | --- |
| 域服务发现与 ICE 配置 | 公布本域的服务入口和 STUN/TURN 地址。 | 启用 Authority，按实际启用的服务公布。 |
| Realm 管理与 Profile | 创建、激活和退役 Realm，提供签名 Profile，支持更新 Front 和切换主 Service 版本。 | 本机需启用 Authority 和 Issuer，Issuer 依赖 Signer。 |
| Realm 成员管理（Membership） | 处理邀请和开放加入，管理成员、角色和客户端密钥。 | 由 Issuer 提供。 |
| Actor 身份签发（Issuer） | 注册 Actor、分配 ActorId，签发、续期或重签 Actor 凭据。 | 依赖本机或远程 Signer。 |
| 密钥与签名（Signer） | 管理签名密钥，提供签名和验签公钥查询。 | 可独立于 Issuer 部署。 |
| 信令（Signaling） | 提供 Actor 发现、在线状态和 WebRTC 建连信令转发。 | 需配置 Issuer 和 Signer，接入时检查 Realm 状态、验签公钥和撤销记录。 |
| STUN | 获取 NAT 映射地址和端口，供客户端尝试直连。 | 可独立部署。 |
| TURN | 为 WebRTC 提供 UDP 中继。 | 静态账号或共享密钥模式可独立部署，Realm 模式依赖 Issuer。 |
| WSS Relay | 中继 Actor RPC 和数据流。 | 静态令牌模式可独立部署，Actr 身份模式依赖 Issuer。 |
| 发布方登记（Publisher Registry） | 登记 Publisher 并提供包签名公钥查询。 | 查询依赖数据库，新增登记走 Realm 管理接口。 |
| 程序包托管（Packages） | 按 SHA-256 摘要提供 `.actr` 文件下载。 | 可与 Realm 服务分开部署。 |
| 节点管理（Admin / Control） | 通过 Web 后台或 gRPC 管理节点和 Realm。 | Web 和 gRPC 可分别启用。 |
| 健康检查与指标 | 提供服务健康状态和 Prometheus 指标。 | 随 HTTP 监听提供。 |

按 Actrium 主线 `106a8d829` 整理。接口与配置详见 [Loran 服务与运维](SERVICES.zh.md)和[配置说明](../config/README.md)。
