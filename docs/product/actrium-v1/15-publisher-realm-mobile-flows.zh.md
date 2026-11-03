# Publisher 注册、Realm 创建与移动端连接 Service

> Actrium v1 流程梳理。示例：Askaway 部署并运营 Loran，以 askaway.chat 作为 Realm Infra Provider ID，同时使用 askaway.chat 作为 Realm 域名。

## 1. 核心概念

| 概念 | 含义 | 示例 |
|---|---|---|
| Publisher | 程序发布者身份 | notes.example |
| Program Release | 不可变程序版本 | notes.example/team-notes@1.4.2 |
| Realm | 一个应用实例的逻辑信任和治理域 | k7m2q9xh@askaway.chat |
| Realm Infra Provider | 运营 Issuer、Signaling、STUN/TURN 等基础设施的一方 | askaway.chat |
| Authority Descriptor | 域名级公开元信息 | https://askaway.chat/.well-known/actr |
| Realm Profile | 单个 Realm 的签名配置，包含 package、Issuer、Provider 和 policy | k7m2q9xh@askaway.chat |

Askaway 部署并运营 Loran，提供 Issuer、Signaling、STUN/TURN、Realm 管理和 Membership 服务，因此是 Realm Infra Provider。它同时控制 askaway.chat 这个 Realm 域名，可以创建多个独立实例：

~~~text
notes@askaway.chat
chat@askaway.chat
crm@askaway.chat
k7m2q9xh@askaway.chat
~~~

这些 Realm 共享 Provider 基础设施，但各自拥有独立的 Realm Profile、Service Actor、成员关系和 Workspace。

Publisher 域名回答“谁发布程序”；Realm 域名回答“谁命名并信任实例”；Provider ID 回答“谁运营基础设施”。三者可以相同，也可以不同。

## 2. 密钥体系与密码学边界

| 密钥 | 算法示例 | 用途 |
|---|---|---|
| Publisher package signing key | Ed25519 | 签署 .actr 包 / manifest |
| Publisher Profile signing key | Ed25519 | 签署 Publisher Profile |
| Realm signing key | Ed25519 | 签署 Realm Profile、Realm 管理信封 |
| Realm issuer key | Ed25519 | 签发 Actor credential |
| Realm Client Key | Ed25519 | 邀请兑换、证明客户端持有权、申请 / 续期 Actor credential |
| Actor long-term key | Ed25519 | 证明 Actor 身份 |
| Ephemeral key agreement key | X25519 等 | 协商会话密钥 |

数字签名用于身份和完整性验证；X25519 等密钥协商算法用于协商共享秘密；会话数据通常使用派生出的对称密钥进行 AEAD 加密。Ed25519 是签名算法，不能把 Ed25519 签名公钥直接当作加密公钥。

## 3. Authority Discovery：域名级公开发现

每个 Actr authority 在固定位置发布公开 descriptor：

~~~http
GET https://<domain>/.well-known/actr
~~~

客户端先验证 HTTPS 证书，再验证 descriptor 的 authority 是否与请求域名一致。Descriptor 可包含 authority、services、realm_signing_keys、publisher 和 links。

必须区分：

- Authority Descriptor 是域名级公开发现文档。
- Publisher Profile 是 descriptor 内独立签名的 publisher member。
- Realm Profile 通过 descriptor 的 realms relation 获取，不是 /.well-known/actr 的子路径。
- realm_signing_keys 与 publisher 各自独立签名，一个成员的签名不能认证其他成员。
- 连接 Realm 必须使用已验证 Realm Profile 指定的 Provider，不能随意改用 descriptor 中其他域名级 endpoint。

## 4. 流程一：Publisher 注册与程序发布

Publisher 控制 notes.example 域名，在 https://notes.example/.well-known/actr 发布 Authority Descriptor。descriptor 的 authority 必须为 actr://notes.example，并发布签名的 publisher member，即 Publisher Profile。

Publisher Profile 至少包含 schema、domain、profile_signing_keys、包签名 keys、key id、算法、状态、sequence、签名及轮换信息。profile_signing_keys 用于验证 Publisher Profile；包签名 keys 用于验证 .actr 包，二者职责不同。

### 4.1 时序图：注册 Publisher

~~~mermaid
sequenceDiagram
    autonumber
    actor P as Publisher
    participant D as Publisher 域名
    participant A as Authority Descriptor
    participant L as Loran / Provider
    participant R as Provider Registry

    P->>D: 配置 DNS、HTTPS、签名密钥
    P->>A: 发布 /.well-known/actr
    Note over A: authority + publisher Profile<br/>profile_signing_keys + package signing keys
    P->>L: actr publisher register
    P->>L: 提交 Publisher ID 与签名注册请求
    Note over P,L: 使用 active Publisher Profile signing key 签名
    L->>D: GET /.well-known/actr
    D-->>L: Authority Descriptor
    L->>L: 校验 HTTPS、authority、Profile 签名与 sequence
    L->>L: 验证注册请求签名与 key 授权
    L->>R: 保存 Publisher 元信息与 key history
    R-->>L: 注册成功
    L-->>P: Publisher registered
~~~

### 4.2 时序图：构建和发布程序包

~~~mermaid
sequenceDiagram
    autonumber
    actor Dev as 开发者
    participant CLI as Actr CLI
    participant PK as .actr Package
    participant CDN as CDN / HTTPS 托管

    Dev->>CLI: build / validate
    CLI->>CLI: 检查 manifest、资源、ABI、contract
    CLI->>PK: 生成 .actr
    CLI->>PK: package signing key 签名
    CLI->>PK: 计算 SHA-256
    Dev->>CDN: 上传程序包
    CDN-->>Dev: 返回下载地址
    Dev->>Dev: 记录 href + sha256 + Program Release
~~~

Provider 自己抓取 descriptor，不信任调用方提交的未验证 Profile。注册请求必须使用有效的 Publisher Profile signing key 签名；Provider 验证 authority 绑定、Profile 签名、sequence、key 轮换链和撤销状态。下载地址不是信任根；下载时仍需校验 SHA-256 和 package signature。

## 5. 流程二：在 askaway.chat 下创建 Realm

Askaway 在 https://askaway.chat/.well-known/actr 发布 authority、realm_signing_keys、realms relation 和 realm-management relation。Descriptor 不枚举 Realm；客户端通过 realms relation 加 RealmId 获取特定 Realm Profile。

Realm Profile 的核心字段包括：

| 字段 | 用途 |
|---|---|
| realm.name / display_name | Realm 完整名称与展示名称 |
| realm.status | active / retired |
| package.href / sha256 | 程序包地址和摘要 |
| package.type | publisher/program@version |
| issuer.url / issuer.keys | Issuer endpoint 与 Actor credential 验签公钥 |
| provider.id | Realm Infra Provider ID |
| provider.signaling / provider.ice | Signaling、STUN/TURN 配置 |
| policy | Realm 加入策略 |
| sequence | Realm Profile 单调递增版本 |
| signature | Realm signing key 对 Profile 的签名 |

Realm Profile 由 Realm signing key 签名，不是由 Realm issuer key 签名。

### 5.1 创建入口与授权规则

Admin 后台由运营者填写 realm_id、display_name、package href、SHA-256、package.type、Workspace 和 policy；Actr CLI 提供对应管理入口。

当前 v1 有两种路径：

1. Realm 域名属于 Provider 自己，例如 askaway.chat：由 Provider 运营者创建并签署 Realm Profile，用户通过 Provider 交付的 Owner 邀请加入。
2. Realm 域名属于其他组织，例如 acme.com，但使用 askaway.chat 的 Loran：Realm 域名持有方授权，向 Provider 的 realm-management endpoint 提交签名管理信封；Provider 创建 pending Realm，返回待签 Profile，持有方签名后提交 publish_profile 激活。

### 5.2 时序图：创建 Realm

~~~mermaid
sequenceDiagram
    autonumber
    actor O as Askaway 运营者
    participant UI as Admin 后台 / Actr CLI
    participant L as Loran / Provider
    participant K as Realm Signing Key
    participant RP as Realm Profile Store
    participant I as Issuer

    O->>UI: 新建 Realm
    O->>UI: 填写 Realm 名、package href、sha256、type
    UI->>L: 创建 pending Realm
    L->>L: 分配 Realm issuer key 与端点
    L-->>UI: 返回待签名 Realm Profile
    UI->>K: 请求签署 Realm Profile
    K-->>UI: 返回 Profile signature
    UI->>L: 提交已签名 Profile / 激活
    L->>L: 验证签名、Realm 名、sequence、provider.id
    L->>RP: 保存并发布 Realm Profile
    L->>I: 激活 Realm Issuer
    L-->>UI: 激活成功 + 一次性 owner_invite
    UI-->>O: 展示 / 导出 Owner 邀请
~~~

Owner 邀请包含 Realm 绑定信息和随机 secret，可选内嵌 Realm Profile。secret 是一次性 bearer secret：持有者可以发起兑换，必须通过安全渠道交付，不得放进 Realm URL 或普通日志。

## 6. 流程三：手机端兑换 Owner 邀请

手机为每个 Realm 单独生成一对 Ed25519 Realm Client Key。私钥保存在 OS 安全存储中，公钥提交给该 Realm 的 Membership endpoint 登记。不同 Realm 不复用同一把 key。

C_O 是手机客户端的 Realm Client Key，不是 Realm signing key，也不是 Front Actor 的 Actor key。

### 6.1 时序图：兑换邀请并成为 Owner

~~~mermaid
sequenceDiagram
    autonumber
    actor U as 用户
    participant M as 手机 Actrium
    participant A as askaway.chat Authority
    participant RP as Realm Profile
    participant I as Issuer / Membership
    participant K as 手机安全存储

    U->>M: 导入 Owner 邀请（realm + secret）
    M->>A: GET /.well-known/actr
    A-->>M: authority + realm_signing_keys + realms relation
    M->>RP: 获取该 Realm Profile
    RP-->>M: Realm Profile + signature
    M->>M: 验证 Realm signing key、Profile、sequence、provider.id
    M->>K: 生成并保存 Realm Client Key C_O
    K-->>M: C_O 公钥与私钥签名能力
    M->>I: Membership join（realm、invite secret、C_O 公钥、签名证明）
    I->>I: 验证邀请、Realm 绑定、新公钥持有证明
    I->>I: 原子消费邀请，建立 owner member
    I->>I: 登记 C_O 公钥
    I-->>M: membership established（member_id / role）
    M-->>U: 已加入 Realm，角色 owner
~~~

Membership endpoint 必须验证邀请未过期、未使用、未撤销，邀请绑定的 Realm 与请求一致，且 C_O 公钥持有证明有效。验证通过后，原子消费邀请、建立 owner 成员记录并登记 C_O 公钥。

Membership 没有单独的 bearer membership token。后续以已登记且未撤销的 C_O 私钥证明客户端资格。Owner 邀请其他成员时，流程相同，但兑换角色由邀请指定为 editor 或 viewer。

## 7. 手机打开 App 并连接 Service

### 7.1 连接步骤

1. Actrium 从已验证 Realm Profile 的 package.href 下载程序包。
2. 根据 package 中的 Publisher 域名获取 Publisher Authority Descriptor 和 Publisher Profile。
3. 使用 profile_signing_keys 验证 Publisher Profile。
4. 使用 Publisher Profile 中的 package signing key 验证 .actr 包签名。
5. 核对 package SHA-256、manifest、资源 hash、package.type 与 Realm Profile 一致。
6. 手机 Actrium 创建固定 Native Front Actor，生成或恢复其 Actor key。
7. Front 使用 C_O 私钥证明 Realm membership，向 Realm Issuer 申请 Front Actor credential。
8. Issuer 检查成员 active 状态、客户端 key、Actor key、Actor 类型和 claim policy，签发短期 Actor credential。
9. Front 使用短期 signaling ticket 连接 Realm Profile 指定的 Signaling endpoint。
10. 在该 Realm 内按 package.type 发现 Service；结果必须恰好一个。
11. 建立传输通道，双方验证 Actor credential、Actor key proof 和 fresh challenge / nonce。
12. 执行 Front–Service Attach，校验程序版本、package digest、protocol fingerprint、成员角色、权限与 lease。
13. Attach 和 contract 成功后，Front 才向 Keel Guest 提供固定目标的 Service capability，并加载 Guest。

### 7.2 时序图：从手机连接到 Service

~~~mermaid
sequenceDiagram
    autonumber
    actor U as 用户
    participant M as 手机 Actrium / Front
    participant A as askaway.chat Authority
    participant RP as Realm Profile
    participant PA as Publisher Authority
    participant CDN as Package CDN
    participant I as Realm Issuer
    participant SG as Signaling
    participant S as Service
    participant G as Keel Guest

    U->>M: 打开 Realm 链接
    M->>A: 获取 Authority Descriptor
    A-->>M: realm_signing_keys + realms relation
    M->>RP: 获取并验证 Realm Profile
    RP-->>M: package + issuer + provider + policy
    M->>CDN: 下载 package.href
    CDN-->>M: .actr package bytes
    M->>PA: 获取 Publisher Descriptor / Profile
    PA-->>M: Publisher Profile + package signing keys
    M->>M: 校验 Profile、SHA-256、包签名、manifest、type
    M->>I: 使用 C_O 证明 membership，申请 Front credential
    I-->>M: Front Actor credential
    M->>SG: 使用短期 signaling ticket 连接
    M->>SG: 按 package.type 发现 Service
    SG-->>M: 唯一 Service ActorId
    M->>S: 建立传输并进行双向 Actor 身份认证
    S-->>M: 对端身份与 challenge 验证通过
    M->>S: Attach（package digest、protocol fingerprint、nonce）
    S->>S: 校验身份、角色、权限、lease、策略
    S-->>M: sessionId + lease + contract + effective capabilities
    M->>G: 注入固定 Service capability，加载 Guest
    G->>M: service.call(route, input)
    M->>S: 固定目标 Actor RPC + session metadata
    S-->>M: 业务响应
    M-->>G: schema 校验后的响应
~~~

## 8. 必须保留的安全边界

1. Realm 邀请 secret 是一次性 bearer secret，必须安全交付，不进入 Realm URI、普通日志或遥测。
2. Realm Client Key、Actor key、Realm signing key、Realm issuer key 彼此独立，不能混用。
3. Realm membership 只决定能否进入实例网络；owner/editor/viewer 等应用权限最终由 Service 执行。
4. Service 发现限定在当前 Realm，且按 package.type 必须恰好找到一个目标；不能随机挑选同类型 Actor。
5. Attach 失败不得自动创建替代 Service、切换 Realm 或降级到其他 Provider。
6. 信令和 TURN 负责控制面协商与流量中继，不持有 Actor 应用 payload 的解密密钥。
7. Actor credential 证明 Actor key 与身份 claims，不单独证明远端实际运行的代码；需要代码 / runtime 真实性时，必须使用独立 attestation。

## 9. 与现有规范的对应关系

- Authority Discovery：actr/protocols/authority-discovery.zh.md
- Realm Profile：actr/protocols/realm-profile.zh.md
- Realm Membership：actr/protocols/realm-membership.zh.md
- URI：actr/protocols/uri.zh.md
- 产品运行时：docs/product/actrium-v1/06-runtime-and-protocol.zh.md
- Realm / 网络 / 身份：docs/product/actrium-v1/07-realm-network-and-identity.zh.md
- 产品验收：docs/product/actrium-v1/10-requirements-and-acceptance.zh.md

> 本文中的 CLI 命令形式与部分管理 UI 操作是流程表达；正式参数、wire schema 和端点行为仍以 Actr 协议文件及其 schema 为准。