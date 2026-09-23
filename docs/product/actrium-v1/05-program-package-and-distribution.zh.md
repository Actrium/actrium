# 05. Program Package 与 HTTPS 分发

## 1. 三层身份

分发设计必须把三件事分开：

| 层 | 标识 | 证明什么 |
|---|---|---|
| Publisher | 规范化域名 | 当前谁控制发布入口 |
| Program Release | `publisher/program@version` + digest | 要运行哪一组不可变字节 |
| Service | ActorId + Actor credential | 当前在与哪个运行实例通信 |

HTTPS 不能证明程序无恶意；Marketplace 审核不能证明网络中的 Actor 身份；Actor credential 也不能证明本地加载的 Guest 是正确版本。

## 2. Publisher 身份

### 2.1 域名即 Publisher ID

Publisher 约定使用域名是合理的，因为它直接复用：

- DNS 全局唯一性；
- 域名所有权管理；
- CA/TLS 证书；
- HTTPS；
- 证书透明度；
- CDN、缓存、代理和企业网络设施；
- 组织已有的安全响应能力。

Actro 不维护另一套全局 Publisher ID 注册中心。

### 2.2 Publisher Profile

Publisher Profile 作为 authority descriptor 的可选成员 `publisher` 发布在该域名唯一的 bootstrap 资源中：

```text
GET https://<publisher>/.well-known/actr
```

`publisher` 成员已在 Actr Authority Discovery §4.5 定义；按 authority-discovery 的规定，`/.well-known/actr` 之下不定义子路径，因此 Publisher 不另设 well-known 文件。同一份 descriptor 还可以承载 `realm_signing_keys`（若该域名下也有 Realm）与 `services`。

bootstrap URL 由 Publisher 身份域名直接导出，必须使用标准 HTTPS 443。

获取 descriptor 时遵循 authority-discovery 的 redirect 规则：最多 5 次、每跳必须 HTTPS，redirect 目标不成为新的 authority，仍须用原 Publisher 域名验证 `authority` 绑定。`publisher` 成员的有效性只取决于其自身签名与固定的 root/key history，不取决于从哪个 origin 取得。

`publisher` 成员字段使用 snake_case，至少包含：

- `schema`：`actr.publisher/v1`；
- `domain`：规范化 Publisher 域名，必须与 descriptor `authority` 的主机名一致；
- 当前和历史 Publisher Profile 签名 key（`profile_signing_keys`）；
- 当前和历史 package signing keys；
- key id、算法、有效期与撤销状态；
- 安全联系地址；
- 可选 Marketplace/透明日志声明。

第一次获取时，信任来自 URL hostname 的 Web PKI，或来自企业预置的 Publisher root pin；Publisher Profile 内携带的 root 不能循环地自证可信。接受首个 Publisher Profile 后，Actro 固定 root/key history，后续 Publisher Profile 必须：

- 按 RFC 8785 JCS 规范化签名 payload（`publisher` 成员去掉其 `signature` 字段），再验证 detached Ed25519 签名；
- 携带单调递增的 `sequence`、签发时间和到期时间；
- 通过旧 root 签名的轮换链引入新 root；
- 拒绝回滚、过期和未授权换根。

descriptor 中 `publisher` 以外的部分不签名，不参与 Publisher 信任判断。

offline root 是强烈推荐的 Publisher 运维方式，但是否要求硬件/离线保存可由 Marketplace 或企业策略提高；wire protocol 必须支持它。

### 2.3 域名过户

域名控制只决定未来发布入口，不得静默改写历史：

- 已安装 `Program Release + digest + signing key` 可继续离线运行；
- 新 key 与历史 key 无可验证轮换链时，视为 Publisher ownership change；
- 已安装版本不得因当前域名返回不同包而变化；
- 新安装或更新必须显示身份变化；
- 高风险策略可以要求 Marketplace/组织管理员再次批准。

## 3. 包从哪来

平台不负责「按程序名找包」。包只有两个来源：

- **打开实例**：Realm Profile 的 `package` 给出下载地址与 SHA-256（[Actr Realm Profile](../../../actr/protocols/realm-profile.zh.md)）；
- **新建实例**：用户在 Program Catalog、Marketplace、本地文件或一个包地址中选定。发现程序、查询新版本属于 Marketplace 与 Publisher 自己的渠道。

无论来源，验证都一样：核对 SHA-256，再用 Publisher Profile 的包签名 key 验证包签名（第 8 节）。下载地址放在哪里都不影响安全。

## 4. 一个 Program Release，一个 `.actr` 包

### 4.1 包的职责

Actro Program Release 直接复用 Actr `.actr` 包：

- 可执行主体是 Service workload；
- Keel Guest 作为签名包资源；
- Front–Service proto/contract 作为签名包资源；
- 图标、静态资源、默认配置进入资源区；
- Actro 扩展字段声明前台入口、权限、组件、Workspace 与兼容范围。

Actro 不再发明一个套在 `.actr` 外面的第二层安装包。

### 4.2 推荐目录

```text
package.actr
├── manifest.toml
├── manifest.sig
├── manifest.lock.toml
├── bin/
│   └── service.wasm
├── ui/
│   ├── guest.bundle.js
│   └── capability-manifest.json
├── proto/
│   └── primary-service.proto
└── assets/
    ├── icon.png
    └── ...
```

Manifest 签名必须绑定所有资源 hash，因此替换 Guest、contract 或图标都会改变 package digest。

### 4.3 包内不允许什么

第三方正式包不得包含：

- Front Host 二进制；
- Keel Host 替代实现；
- 自定义 Host Module 实现；
- React Native 原生插件；
- Front 动态库；
- 未在 manifest 中列出的可执行脚本；
- 运行时从网络替换的 Guest 入口。

Publisher 只能提供 Guest 与声明；Actr Host 实现永远随 Actro 客户端发布。

## 5. Service workload 政策

### 5.1 Marketplace 默认等级

v1 Marketplace 第三方 Service workload 必须是：

- WebAssembly Component Model；
- 目标 ABI 在 Actro 支持列表内；
- 通过 import/export allowlist；
- 带 fuel、wall-clock timeout、memory、table、stream 和 I/O 配额；
- 所有 Workspace、网络和 Actor 能力经 Service Host broker。

### 5.2 Native Service

Actr 已具备 native cdylib/linked workload 能力，但这不等于 Marketplace 应默认允许：

- native workload 与 Actro 进程具有更高风险；
- iOS 等平台禁止随意下载执行本机代码；
- 动态库存在 ABI、签名和供应链差异。

因此 Native Service 只适用于：

- Actro 第一方系统程序；
- 企业受管分发；
- 桌面开发模式；
- 用户明确接受的 elevated trust tier。

Native Service 不能改变 Front 仍只允许 Keel Guest 的规则。

Native workload 必须进一步区分：

- `NativeCdylib`：加载进 Actr Host 地址空间；不能证明出口闭合，第三方实例不得为绿；
- `NativeCapsule`：独立受限进程，只有在 OS sandbox、无原始网络/文件、sealed libraries 和 brokered IPC 全部生效时，才可能评为 `CONTROLLED`；
- `NativeTrusted`：Actro 自带或企业预置 TCB，不属于开放 Marketplace trust tier。

v1 Marketplace 仍只开放 WASM Service。Native Capsule 是桌面优先的后续能力，具体约束见[隐私标记与汇聚协议](13-privacy-state-and-aggregation.md)。

### 5.3 UI-only 程序

v1 不引入「没有 Service 的页面应用」特例。工具链为 UI-only 程序生成一个平台定义的最小 WASM Service Component：

- 它仍有与 Program Release 相同的 ActrType 和独立 ActorId；
- 默认没有 Workspace 和外部网络能力；
- 只实现 session、health 和可选的最小 key-value contract；
- 同样经过包签名、资源限制和 Service 生命周期；
- Publisher 不能用它绕过 Host API 或注入 native code。

因此所有 app Tab 仍统一 Attach 一个 Service，差别只在它是否包含自定义逻辑。

## 6. Actro Manifest 扩展

Actr 基础 manifest 继续提供：

- publisher；
- name/program；
- version；
- binary target/kind/hash；
- Ed25519 signature 和 key id；
- resources；
- proto files；
- dependency lock。

Actro 增加：

```toml
[actro]
schema_version = 1
min_actro_version = "1.0.0"
front_host_api = "^1.0"
service_host_api = "^1.0"

[actro.front]
guest_runtime = "keel-js"
guest_entry = "ui/guest.bundle.js"
keel_api = "^1.0"
contract_manifest = "ui/capability-manifest.json"
contract_digest = "sha256:..."

[actro.service]
primary_contract = "proto/primary-service.proto"
protocol_fingerprint = "sha256:..."

[actro.service.workspace]
requirement = "required"
provisioning = "user-selected"
access = "read-write"
providers = ["managed-store", "local-folder", "cloud-drive"]
lease = "exclusive-writer"

[actro.session]
attachment_policy = "exclusive"

[actro.privacy]
schema_version = 1
data_classes = ["public", "private", "secret"]
default_unknown_action = "block-private-and-secret"

[[actro.privacy.actor_egress]]
actr_type = "notes.example/index-worker@1.4.2"
required = false
data_classes = ["private"]
```

完整示例见[参考协议](12-reference-spec.md)。

## 7. 权限、组件和配额声明

包声明的是「申请」，不是实际授权：

```text
effective capabilities =
  package requested
  ∩ Actro Host supported
  ∩ user/admin granted
  ∩ current platform allowed
  ∩ current session scope
```

Manifest 至少声明：

- Front capability；
- Service capability；
- Keel 组件集合；
- outbound network domain；
- camera/microphone/clipboard/notification/location；
- Workspace access mode；
- Guest heap、单次执行时间、render op；
- Service memory/fuel/storage/network；
- 最低 Actr Host/SDK/contract 版本。
- 数据分类、预期 Actor 出口、terminal egress、required/optional 和最低节点结论。

新增高风险权限必须发布新版本，并在升级时重新确认。

Privacy Declaration 是静态 allowlist 和安装说明，不是运行时隐私图的权威来源。Actr Host 必须把 WASM imports、实际 capability 与实际 edge 对照声明；Publisher 把依赖标记为 optional 也不能覆盖 Actr Host 根据真实数据流做出的闭包判断。

## 8. 下载与验证流水线

```text
Realm Profile package / 用户选定的包
  → HTTPS download
  → package digest
  → package signature/key history
  → ZIP structural limits
  → manifest identity
  → every file hash
  → ABI/contract/import validation
  → policy + permission review
  → content-addressed cache
  → Service Runtime / verified Guest bytes
```

### 8.1 检查顺序

1. 校验 Publisher DNS/IDNA；
2. TLS 和 hostname；
3. 下载大小上限；
4. package SHA-256；
5. manifest 声明的 publisher 的 Publisher Profile；
6. Ed25519 key id 和签名；
7. ZIP path、文件数、压缩比和解压总量；
8. manifest 的 publisher/program/version；
9. binary kind/target；
10. 每个 resource/proto/lock hash；
11. Guest 静态依赖和 capability manifest；
12. Service WASM imports；
13. Host API、Keel API 与 contract 兼容性；
14. 本地/组织安全策略。

任何一步失败都不得把部分内容交给 Keel 或 Service runtime。

### 8.2 Keel 加载边界

Keel `Engine.loadBundle(URL)` 只是加载机制，不是安装信任机制。Actro 必须：

1. 自行下载并验证 package；
2. 从 content-addressed cache 读取已验证 Guest bytes；
3. 把本地 bytes/code 交给 Keel；
4. 禁止 Guest 在启动时指定替代 URL。

## 9. 版本不可变

Actro 本地以以下联合键保存：

```text
(publisher, program, version, package_digest)
```

规则：

- 第一次接受后固定 digest；
- 同一 Program Release 出现不同 digest = `PACKAGE_EQUIVOCATION`；
- 不能用「Publisher 更新了文件」覆盖缓存；
- 更新必须使用新 SemVer；
- Service 实例固定其 package digest；
- 缓存清理不能删除仍被 Service 引用的 digest；
- 重新下载必须再次验证后与固定 digest 比较。

本地 first-seen pin 只能保护当前安装；Marketplace 可以帮助多个客户端比对已见 digest。要对所有客户端提供全局 non-equivocation，需要后续引入公开 append-only transparency log。v1 不把单机 pin 夸大成全局透明性。

## 10. 签名与 HTTPS 的分工

### HTTPS 提供

- 当前域名控制证明；
- 传输机密性与完整性；
- Publisher Profile 的来源；
- 标准代理/CDN/缓存能力。

### Package 签名与 digest 提供

- CDN、镜像和落盘后的完整性；
- 离线验证；
- key rotation history；
- 精确版本不可变；
- 域名暂时不可用时继续运行；
- 检测同名异包。

两者必须同时使用，不以一方替代另一方。

## 11. Marketplace

Marketplace 可以：

- 搜索、分类和推荐；
- 记录 Publisher 验证；
- 静态/动态审核；
- 风险评级；
- 记录已见 Program Release/digest；
- 发布撤销和恶意软件情报；
- 提供缓存或镜像。

Marketplace 不可以：

- 为第三方抢注其域名；
- 静默替换 Publisher package；
- 用市场登录身份替代 Publisher 域名；
- 因「已审核」绕过 Actro 权限；
- 把下架与 Service/Workspace 自动删除等同。

## 12. 撤销政策

撤销分级：

| 等级 | 行为 |
|---|---|
| Advisory | 显示风险，允许继续 |
| Block new | 阻止新安装/新建，已有实例提示 |
| Quarantine | 默认停止并隔离已有实例，用户/管理员可审计处理 |
| Critical block | 阻止加载 Guest/Service，保留数据恢复入口 |

撤销记录必须包含：

- Program Release/digest/key；
- 原因；
- 发布时间；
- 发布者或市场签名；
- 建议操作；
- 是否影响仅 Guest、Service 或签名 key。

撤销不自动删除 Workspace。

## 13. 更新与升级

Program Catalog 可以提示新版本，但 Service 不自动原地升级。

v1 升级：

1. 安装新 Program Release；
2. 在同一 Realm 内创建新 Service，获得新 ActorId；
3. 旧 Service 生成迁移快照；
4. 显式交接 Workspace lease；
5. 新 Service 导入并验证；
6. 用户确认切换，随后以 `publish_profile` 发布 `package` 指向新包的 Realm Profile；
7. 旧 Service 保留回滚窗口后归档；回滚即再发布一份指回旧包、`sequence` 更高的 Realm Profile。

这样避免 ActorId 中版本与实际 workload 不一致，也让失败回滚可解释。

## 14. 本地开发与自建

支持：

- 本地 `.actr` 文件；
- localhost、回环地址或显式信任的私有 CA 上的包地址；
- 本地开发 signing key；
- 热更新 Guest；
- 未发布 Program Catalog entry。

限制：

- 每次代码变化产生新 dev digest；
- 未验证 key 不继承同域名正式 Publisher 权限；
- 本地包不进入正式 Marketplace；
- 私网、loopback、link-local URL 只在开发模式或企业策略中允许；
- 开发 package 不可用于绕过 Native Service 限制。

## 15. 缓存与离线

缓存按 package digest 内容寻址：

```text
cache/packages/sha256/<digest>/package.actr
cache/extracted/sha256/<digest>/...
```

要求：

- 原始 package 保留，用于重新验证；
- extracted 文件只从已验证 package 产生；
- 原子写入；
- 未完成下载隔离；
- 缓存元数据记录 Publisher key 与验证时间；
- 已有本地 Service 在 Publisher 离线时可以运行固定 digest；
- Realm 凭据/撤销过期时按离线安全政策处理，不等同于 package 验证失败。
