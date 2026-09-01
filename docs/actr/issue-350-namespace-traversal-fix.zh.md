# Issue #350 修复实施方案：阻止签名清单身份逃逸 Actor 存储命名空间

- 日期：2026-07-30
- 关联 Issue：[Actrium/actr#350 — security(hyper): prevent signed manifest identity from escaping actor storage namespaces](https://github.com/Actrium/actr/issues/350)
- CWE：CWE-22 / CWE-73
- 严重度：Medium（需要可信签名者或泄露的签名密钥，不是匿名远程利用）
- 状态：待实施（按“无需保留旧 ActorStore 数据”前提修订）

---

## 一、结论

漏洞真实存在：`.actr` 签名只证明 manifest 的来源和完整性，不证明
`manufacturer`、`name`、`version` 适合参与宿主机文件路径。当前 Hyper 将这三个
未经语义校验的字符串插入 `storage_path_template`，最终由
`ActorStore::open()` 创建目录和 SQLite 数据库，签名者因此可以通过 `..`、绝对路径
或平台路径语义逃出预期 Actor 命名空间。

本文采用全新的哈希存储布局，其 layout version 为 v1，不再保留旧路径兼容：

```text
{storage_root}/actors/v1/{namespace_hash}/store.sqlite3
```

其中 `namespace_hash` 是对已验证 Actor 身份做域分隔、字段标记和长度前缀编码后计算
的 SHA-256。原始身份永远不再作为文件或目录名。

本方案同时做出以下决定：

1. 哈希命名空间是本次修复的必做项，不再拆成可选 PR-B。
2. 文件系统存储必须配置明确的 `storage_root`；移除任意 Actor 路径模板。
3. `PlatformProvider` 改为接收强类型 `StorageNamespace`，用编译错误强制自定义 provider 适配，避免裸字符串静默改变语义。
4. manufacturer/name 复用协议层 `Name` 规则，version 使用明确且唯一的协议规则；所有入口共用 `ActrType::validate()`。
5. 身份校验必须接入内置 verifier、自定义 `TrustProvider`、Registry key lookup 和最终存储入口。
6. 保持现有「开库 → AIS 注册 → 写入公钥」顺序，不把开库延迟到 AIS 成功之后。
7. symlink 防护在“storage root 由 Actr 独占、攻击者不能并发改写”的威胁模型内完成；更强的同 UID 本地攻击不在本 Issue 内宣称完全解决。
8. 公开安全入口改为 `ActorStore::open_under()`；接收任意路径的 raw open 不再作为 public API。
9. 复用现有 `HyperError::InvalidManifest`，不新增公开 enum 变体。

### 1.1 本轮设计审查结论

| 审查项 | 判断 | 本文收口方式 |
|---|---|---|
| P1：storage 身份规则与 `Name` / `ActrType` 不自洽 | 存在；数字开头、超过 32 UTF-8 字节等值会形成“storage 合法、协议非法”。`x..y` 和满足 `Name` 的 Unicode 本身仍合法 | manufacturer/name 复用 `Name`，version 单独定义；所有入口调用 `ActrType::validate()` |
| P2：“协议分隔符”未明确 | 存在；`:` 会破坏 `ActrType` 三段表示，`@`、`/` 还参与 `ActrId` 表示 | 明确三个字段统一禁止 `:`、`@`、`/`，并加入 round-trip 测试 |
| P3：`PlatformProvider` 签名不变会造成静默语义破坏 | 存在；旧 provider 可能继续把 opaque ID 当路径 | 改为强类型 `StorageNamespace`，有意让旧实现编译失败并显式迁移 |
| P4：public `ActorStore::open(path)` 可绕过安全入口 | 存在；仅加 `open_under()` 或 `#[doc(hidden)]` 都不能关闭 bypass | public API 只保留 `open_under()`；raw open 删除或降为 crate-private |
| P5：移除模板后留下 `NamespaceResolver` 死代码 | 存在；直接删配置字段还可能被 Serde 静默忽略 | 完整删除 resolver/模板实现和测试，同时保留只用于明确报错的旧配置 tombstone |

### 1.2 本方案的前提

- 不迁移、不读取旧 ActorStore 路径；
- 升级后允许从空的 hashed-v1 存储目录开始；
- 接受 `storage_path_template` 配置语义被替换；
- 外置磁盘通过 `storage_root` 指定，而不是通过绝对路径模板指定；
- 接受 `PlatformProvider::secret_store()` 参数类型变化，自定义 provider 必须重新编译并适配；
- 接受原公开 `ActorStore::open(path)` 被移除或降为 crate-private；
- 包格式本身不变，已有 `.actr` 包只要身份满足新的协议校验仍可使用。

“不保留旧数据”只消除了数据迁移成本，不代表没有配置或 API 影响；相关影响见第六节。

---

## 二、问题与威胁模型

### 2.1 漏洞本质

恶意或泄露密钥的签名者可以构造：

```toml
manufacturer = "evilco"
name = "Loader"
version = "../../victim"
```

当前默认 `{actr_type}` 通过以下方式形成：

```text
evilco/Loader/../../victim
```

`PathBuf::from()` 不会安全消除 `..`；后续 `create_dir_all()` 和 SQLite open 由操作系统
解析路径，最终可能在另一个 Actor 的目录中创建或打开数据库。

这是：

```text
CWE-73：外部输入控制路径
    ↓
CWE-22：路径穿越到预期命名空间之外
```

### 2.2 签名不是路径授权

签名验证只能回答：

> 这份 manifest 是否由对应密钥签署，内容是否被篡改？

它不能回答：

> 这些字符串是否可以安全地作为宿主机目录名？

签名者可以被信任发布代码，但不应自动获得任意宿主机文件路径选择权。该区别对
native-hosted WASM 尤其重要：WASM guest 的沙箱能力不应被 manifest 路径插值绕过。

### 2.3 当前攻击链

| 环节 | 当前代码 | 问题 |
|---|---|---|
| Manifest 类型 | `crates/actr-pack/src/manifest.rs:5-8` | 三个身份字段都是无约束 `String` |
| TOML 解析 | `PackageManifest::from_toml()` | 只反序列化，无身份语义校验 |
| 协议身份解析 | `crates/actr-protocol/src/actr_ext.rs:55-82` | `from_string_repr()` 对 manufacturer/name 使用 `Name`，package 路径未复用，存在两套合法性规则 |
| 内置验签 | `crates/actr-pack/src/verify.rs:111-268` | 验签、哈希后原样返回身份 |
| Registry key lookup | `crates/actr-hyper/src/verify/trust.rs:193-220` | 验签前使用原始 `manufacturer` 获取公钥 |
| 自定义 verifier | `TrustProvider::verify_package*()` | provider 可以直接返回未经身份校验的 `VerifiedPackage` |
| Workload attach | `load_workload_package_inner()` | provider 返回后立即进入 binary 检测和 workload 加载 |
| Namespace 解析 | `crates/actr-hyper/src/config.rs:735-820` | 字符串替换后直接 `PathBuf::from` |
| ActorStore | `crates/actr-hyper/src/storage/db.rs:37-85` | `create_dir_all` 后 `Connection::open` |
| ActorStore public API | `crates/actr-hyper/src/lib.rs:197,265` | 公开重导出的 `ActorStore::open(path)` 可绕过未来 root-aware 安全入口 |
| Native platform | `crates/actr-platform-native/src/platform.rs:87-104` | 把 namespace 当绝对路径透传 |

只在 `resolve_storage_path_for()` 增加校验虽然可以保护当前落盘点，但不能建立
“`VerifiedPackage` 的身份已经安全”的信任边界，也不能阻止自定义 provider 的危险返回值
先进入 workload 加载阶段。

### 2.4 symlink 是独立风险

词法 containment 只能判断字符串：

```text
/var/lib/actr/actors/v1/hash/store.sqlite3
```

是否以 root 开头。如果其中某个已存在目录是：

```text
/var/lib/actr/actors -> /tmp/outside
```

操作系统仍可能把实际文件打开到 root 之外。因此身份哈希、词法 containment 和
symlink 防护必须分层处理。

---

## 三、目标架构

### 3.1 固定文件系统布局

Native Hyper 的唯一布局：

```text
{storage_root}/actors/v1/{64-char-lowercase-sha256}/store.sqlite3
```

示例：

```text
/var/lib/actr/
└── actors/
    └── v1/
        └── 4f5a7b3d...e921/
            ├── store.sqlite3
            ├── store.sqlite3-wal
            └── store.sqlite3-shm
```

路径中只有：

- 由框架写死的 `actors/v1/store.sqlite3`；
- 由框架生成的 64 位小写十六进制 hash。

manifest、环境变量、hostname 和用户模板都不能再贡献文件系统路径分量。

### 3.2 强类型 opaque platform namespace

`PlatformProvider::secret_store()` 改为接收 `StorageNamespace`，而不是裸 `&str`。该类型
可以格式化为版本化 opaque identifier：

```text
actr-storage:v1:4f5a7b3d...e921
```

Platform provider 决定如何映射：

```text
Native filesystem  → provider_root/actors/v1/<hash>/store.sqlite3
Apple Keychain     → service/account namespace
Android storage    → encrypted-store namespace
Web                → IndexedDB/localStorage prefix
Test provider      → HashMap key
```

Hyper 不再把 canonical filesystem path 传给自定义 provider，也不对 provider 自己的
存储根执行本地 `data_dir` containment。

这里有意制造 Rust 编译期 breaking change：旧 provider 若仍把 namespace
`Path::new(...)`，必须在升级时修改，不能让“绝对路径 → opaque ID”的语义变化静默进入
生产环境。

### 3.3 修复后的数据流

```text
原始 .actr bytes
  │
  ├─ RegistryTrust：未认证 lookup 字段预检 + URL 安全编码
  │
  ├─ TrustProvider：签名/哈希/供应链策略验证
  │
  ├─ Hyper：对 provider 返回值强制身份校验
  │
  ├─ 计算 StorageNamespaceId
  │
  ├─ 加载 WASM / DynClib workload
  │
  ├─ 注册阶段重新校验并解析固定 namespace
  │
  ├─ 安全打开 ActorStore / platform secret store
  │
  ├─ AIS 注册
  │
  └─ 持久化 AIS signing key
```

---

## 四、详细实现

### 4.1 协议唯一身份规则与 namespace ID

身份合法性首先是协议约束，不应由 storage 层另起一套规则。当前
`ActrType::from_string_repr()` 已通过 `Name::new()` 校验 manufacturer/name，因此新增
唯一的 `ActrType::validate()`，让字符串解析、package verification、Registry lookup 和
AIS registration 共用。

在 `crates/actr-protocol/src/actr_ext.rs` 增加：

```rust
pub const MAX_ACTR_VERSION_BYTES: usize = 64;

impl ActrType {
    pub fn validate(&self) -> Result<(), ActrIdError> {
        Name::new(self.manufacturer.clone()).map_err(|error| {
            ActrIdError::InvalidComponent(format!("Invalid manufacturer: {error}"))
        })?;
        Name::new(self.name.clone()).map_err(|error| {
            ActrIdError::InvalidComponent(format!("Invalid type name: {error}"))
        })?;
        validate_version(&self.version)
    }
}

fn validate_version(version: &str) -> Result<(), ActrIdError> {
    if version.is_empty() {
        return Err(ActrIdError::InvalidComponent(
            "Invalid version: version cannot be empty".into(),
        ));
    }
    if version.len() > MAX_ACTR_VERSION_BYTES {
        return Err(ActrIdError::InvalidComponent(format!(
            "Invalid version: exceeds {MAX_ACTR_VERSION_BYTES} UTF-8 bytes"
        )));
    }

    let first = version.chars().next().expect("checked non-empty");
    let last = version.chars().next_back().expect("checked non-empty");
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return Err(ActrIdError::InvalidComponent(
            "Invalid version: must start and end with an ASCII alphanumeric character".into(),
        ));
    }
    if let Some(ch) = version.chars().find(|ch| {
        !ch.is_ascii_alphanumeric() && !matches!(ch, '.' | '_' | '-' | '+')
    }) {
        return Err(ActrIdError::InvalidComponent(format!(
            "Invalid version character: {ch:?}"
        )));
    }
    Ok(())
}
```

协议规则明确为：

- manufacturer/name 完全复用现有 `Name`：
  - 非空；
  - 当前实现按 UTF-8 **字节数**限制为 32；
  - 字母开头、字母数字结尾；
  - 只允许 Unicode alphanumeric 与 `-`、`_`、`.`；
- version：
  - 1–64 UTF-8 字节；
  - 首尾必须是 ASCII 字母数字；
  - 中间只允许 ASCII 字母数字与 `.`、`_`、`-`、`+`；
  - 因此明确禁止 `:`、`/`、`@`、`\`、空白和控制字符。

注意 `Name` 的 32 上限按 UTF-8 **字节**计、不是字符：一个 CJK 字符占 3 字节，因此
manufacturer/name 实际只能容纳约 10 个汉字。发布前的身份扫描必须按字节核算（见第八节），
避免把「32 字符」误读为可写 32 个汉字而漏判不合规包。

协议分隔符必须在实现和测试中钉死，不能只写成“拒绝协议分隔符”：

- `ActrType` 的字段分隔符是 `:`，因此三个字段都禁止 `:`；
- `ActrId` 还使用 `@` 和 `/`。当前 parser 对 version 中的 `@`、`/` 在部分场景可以
  round-trip，但本方案不保留这种偶然行为，三个字段统一禁止 `@`、`/`；
- manufacturer/name 由 `Name` 的 allowlist 自然拒绝上述字符，version 由自己的
  allowlist 拒绝；`\` 同样在三个字段中非法。

这里不强制 SemVer，以保留 `v1` 等既有协议表示；如果产品决定 package version 必须是
SemVer，应在协议层单独升级，而不是在 storage 层隐式收紧。

`ActrType::from_string_repr()` 在拆出三个字段后构造 `ActrType`，再调用同一个
`validate()`。对所有已经通过 `validate()` 的值，这保证：

```text
ActrType 可注册
  ⇔ 可 to_string_repr/from_string_repr round-trip
  ⇔ 可作为 package identity
  ⇔ 可计算 storage namespace
```

`Name` 本身允许合法 Unicode alphanumeric，也允许 `x..y`；本方案不额外拒绝它们。
数字开头和超过 32 UTF-8 字节的 manufacturer/name 则在所有入口一致拒绝。

`ActrType` 是 protobuf 数据类型，字段仍然公开，Rust 代码可以绕过 parser 手工构造非法
值。因此 `to_string_repr()` 成功不能被当作“已经校验”的证明；所有安全敏感入口必须先
调用 `validate()`，或只接收 `ValidatedActorIdentity`。尤其是 AIS 生成 ACL key 前的
`to_string_repr()` 调用，必须由注册入口校验保护。可以为新调用方补充
`try_to_string_repr()`，但不能用它替代入口校验。

在 `crates/actr-pack/src/identity.rs` 新增：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidatedActorIdentity {
    manufacturer: String,
    name: String,
    version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StorageNamespaceId([u8; 32]);

impl StorageNamespaceId {
    pub fn into_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}
```

`ValidatedActorIdentity::from_manifest()` 构造 `ActrType` 并调用
`ActrType::validate()`，成功后才保存三个字符串。`actr-pack` 增加
`actr-protocol.workspace = true`；当前 `actr-protocol` 不依赖 `actr-pack`，不会形成
Cargo dependency cycle。

namespace 编码必须避免拼接歧义：

```rust
use sha2::{Digest, Sha256};

impl ValidatedActorIdentity {
    pub fn namespace_id(&self) -> StorageNamespaceId {
        let mut hasher = Sha256::new();
        hasher.update(b"actr-storage-namespace\0v1");
        update_field(&mut hasher, 1, self.manufacturer.as_bytes());
        update_field(&mut hasher, 2, self.name.as_bytes());
        update_field(&mut hasher, 3, self.version.as_bytes());
        StorageNamespaceId(hasher.finalize().into())
    }
}

fn update_field(hasher: &mut Sha256, tag: u8, bytes: &[u8]) {
    hasher.update([tag]);
    hasher.update((bytes.len() as u32).to_be_bytes());
    hasher.update(bytes);
}
```

规则：

- hash 输入使用签名 manifest 中校验后的原始 UTF-8；
- 不做隐式 lowercase；
- 不做隐式 Unicode NFC/NFD 转换；
- `Evil` 与 `evil` 得到不同 hash；
- hash 只对通过 `ActrType::validate()` 的身份计算；若两个合法 Unicode 身份的原始
  UTF-8 不同，其 hash 也不同；
- 输出统一为小写 hex，因此大小写不敏感文件系统不会折叠两个 namespace。

### 4.2 内置 verifier：验签后校验

在 `crates/actr-pack/src/verify.rs` 中，签名验证和 manifest 解析完成后、返回
`VerifiedPackage` 之前执行：

```rust
let manifest = PackageManifest::from_toml(manifest_str)?;
validate_manifest_integrity_fields(&manifest)?;

ValidatedActorIdentity::from_manifest(&manifest).map_err(|error| {
    PackError::InvalidPackage(format!("invalid actor identity: {error}"))
})?;
```

这样所有使用 `actr_pack::verify()`、`verify_bounded()` 和
`verify_with_limits()` 的内置调用方都不会获得身份非法的已验证包。

不建议把此逻辑直接塞进 `PackageManifest::from_toml()`：纯解析 API 仍可用于诊断和展示
不可信 manifest；“解析成功”与“满足运行时信任策略”应保持区分。

### 4.3 RegistryTrust：获取公钥前安全预检

`RegistryTrust` 必须在验签前读取 `manufacturer` 和 `signing_key_id` 来选择公钥，这是
必要的 locator，不代表该 manifest 已经可信。

在 `get_or_fetch()` 前：

1. 对未认证 manifest 执行完整 `ValidatedActorIdentity` 预检；
2. 对 key ID 做独立的长度、控制字符和 lookup 格式预检；
3. 使用 URL API 添加 path segment 和 query pair；
4. 不再用 `format!()` 拼 URL；
5. 证书缓存 key 改为结构化 tuple，而不是 `manufacturer:key_id` 字符串拼接。

示意：

```rust
let mut url = reqwest::Url::parse(&self.ais_endpoint)?;
url.path_segments_mut()
    .map_err(|_| HyperError::Config("AIS endpoint cannot be a base URL".into()))?
    .extend(["mfr", manufacturer, "verifying_key"]);
url.query_pairs_mut().append_pair("key_id", key_id);
```

这次完整身份预检只表示“字段满足运行时语法，可以继续做安全的 key lookup”，不表示
manifest 已可信；真实性仍由随后对同一份 package bytes 的签名验证建立。

### 4.4 Hyper：封住自定义 TrustProvider 边界

新增唯一的内部入口：

```rust
async fn verify_package_for_hyper(
    inner: &HyperInner,
    bytes: &[u8],
) -> HyperResult<VerifiedPackage> {
    let max_bytes = inner.config.resolved_wasm_limits().max_component_bytes;
    let verified = inner
        .config
        .trust_provider
        .verify_package_bounded(bytes, max_bytes)
        .await?;

    ValidatedActorIdentity::from_manifest(&verified.manifest)
        .map_err(|error| HyperError::InvalidManifest(error.to_string()))?;

    Ok(verified)
}
```

以下两个入口都必须改为调用该函数：

- `Hyper::verify_package()`；
- `load_workload_package_inner()`。

这保证自定义 provider 即使没有调用 `actr_pack::verify()`，其返回值也会在
`detect_binary_kind()` 和 workload 加载之前被 Hyper 拒绝。

AIS 是独立服务端信任边界，不能只依赖 Hyper 已经校验。AIS register handler 在处理
外部请求、生成 ActrId 或写 ACL 前也必须调用 `request.actr_type.validate()`；当前
`crates/loran-ais/src/handlers.rs` 中以 `to_string_repr()` 生成 ACL key 的代码只能位于
该校验之后。这样即使请求绕过 Hyper 直接到达 AIS，也不会写入以后无法由
`from_string_repr()` 解析的 key。

### 4.5 Storage 配置：移除任意路径模板

`HyperConfig` 增加明确的 filesystem root，并废弃/移除
`storage_path_template`：

```rust
pub struct HyperConfig {
    pub data_dir: PathBuf,
    pub storage_root: PathBuf,
    pub trust_provider: Arc<dyn TrustProvider>,
    // ...
}
```

建议构造语义：

```rust
HyperConfig::new(data_dir, trust_provider)
    // 默认 storage_root = data_dir.join("storage")
    .with_storage_root("/mnt/ssd/actr");
```

外置磁盘只需要：

```text
data_dir     = /var/lib/actr
storage_root = /mnt/ssd/actr
```

不再支持：

```text
storage_path_template = /mnt/ssd/{manufacturer}/{name}/{version}
```

旧配置必须显式报错，不能静默忽略。`HyperSection` 当前没有
`#[serde(deny_unknown_fields)]`；若直接删除字段，Serde 可能把旧
`storage_path_template` 当成未知字段忽略，程序随后悄悄切到默认 root。

保留一个只用于迁移报错的 tombstone 字段：

```rust
#[serde(default, rename = "storage_path_template")]
removed_storage_path_template: Option<String>,
```

解析时：

```rust
if hyper_section.removed_storage_path_template.is_some() {
    return Err(HyperError::Config(
        "`storage_path_template` was removed; configure `storage_root` instead".into(),
    ));
}
```

从 `actr.toml` 读取的 `storage_root` 若为相对路径，应相对于该配置文件所在目录解析；
程序化调用 `with_storage_root()` 若传相对路径，则相对于 `config.data_dir` 解析。两种
入口都不能在开库时再依赖当时的进程工作目录。

Hyper 初始化时：

1. 若 root 不存在，创建 root；
2. 拒绝 root 本身为 symlink/reparse point；
3. canonicalize root；
4. 构造 `SecureStorageRoot` 并存入 `HyperInner` 私有字段；
5. 不修改公开的 `config.data_dir` 或 `config.storage_root` 字符串。

`storage_root` 只约束 Hyper 直接使用 filesystem storage 的分支；
`Hyper::with_platform()` 下由 provider 持有并保护自己的真实 root。

身份到 namespace 的转换和固定路径解析：

```rust
fn storage_namespace_for(
    manifest: &PackageManifest,
) -> HyperResult<StorageNamespace> {
    let identity = ValidatedActorIdentity::from_manifest(manifest)
        .map_err(|error| HyperError::InvalidManifest(error.to_string()))?;
    Ok(StorageNamespace::actor_v1(
        identity.namespace_id().into_bytes(),
    ))
}

impl SecureStorageRoot {
    fn path_for(&self, namespace: &StorageNamespace) -> HyperResult<PathBuf> {
        if namespace.version() != 1 {
            return Err(HyperError::Storage(format!(
                "unsupported storage namespace version: {}",
                namespace.version(),
            )));
        }

        let component = hex::encode(namespace.digest());
        debug_assert_eq!(component.len(), 64);
        let candidate = self
            .canonical_path()
            .join("actors")
            .join("v1")
            .join(component)
            .join("store.sqlite3");

        if !candidate.starts_with(self.canonical_path()) {
            return Err(HyperError::Storage(
                "derived storage path escaped storage root".into(),
            ));
        }
        Ok(candidate)
    }
}

fn resolve_storage_path_for(
    inner: &HyperInner,
    manifest: &PackageManifest,
) -> HyperResult<PathBuf> {
    let namespace = storage_namespace_for(manifest)?;
    // platform-backed 节点没有 SecureStorageRoot（由 provider 持有真实 root），
    // 因此 secure_storage_root 是 Option，此诊断方法仅服务 direct-filesystem 分支。
    let root = inner.secure_storage_root.as_ref().ok_or_else(|| {
        HyperError::Config(
            "resolve_storage_path is only available for direct-filesystem storage".into(),
        )
    })?;
    root.path_for(&namespace)
}
```

`starts_with` 在这里属于防御性 invariant：候选路径只包含固定字符串和 hex，本身已经
没有外部路径分量。

`HyperInner.secure_storage_root` 的类型是 `Option<SecureStorageRoot>`：仅当 Hyper 使用
direct-filesystem 存储分支（非 `with_platform()`）时才初始化为 `Some`；platform 分支为
`None`，namespace 直接交给 provider。公开的 `Hyper::resolve_storage_path()` 在 platform
节点上返回上述明确错误，不会误报一个并不存在的本地 root。

固定相对布局必须只在 `SecureStorageRoot::path_for(namespace)` 中实现一次；
`resolve_storage_path_for()` 和 `ActorStore::open_under()` 都调用它，避免诊断路径与真实
开库路径发生漂移。bootstrap 不再先解析裸路径再传给两个分支，而是先调用
`storage_namespace_for()`，然后把同一个强类型 namespace 交给 platform provider 或
`ActorStore::open_under()`。

移除模板后，`NamespaceResolver` 不再有生产用途，必须在同一个改动中完整删除：

- `DEFAULT_STORAGE_TEMPLATE`；
- `HyperConfig::storage_path_template` 和 Debug 输出；
- `HyperConfig::with_storage_template()`；
- `HyperSection` 原有模板配置逻辑（只保留上述报错 tombstone）；
- `NamespaceResolver`、`HashMap` 变量表和 `{env.VAR}` 展开逻辑；
- `resolve_storage_path_for()` 对 resolver 的调用；
- `config_tests.rs` 中全部模板解析测试；
- 示例配置和用户文档中的模板变量说明。

不能把 `NamespaceResolver` 留作“以后可能复用”的死代码；若未来有其他 namespace
模板需求，应重新设计不接收 manifest 路径分量的专用 API。

### 4.6 PlatformProvider：传递 opaque namespace

在 `crates/actr-platform-traits` 定义强类型：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageNamespace {
    version: u8,
    digest: [u8; 32],
}

impl StorageNamespace {
    pub fn actor_v1(digest: [u8; 32]) -> Self {
        Self { version: 1, digest }
    }

    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn opaque_key(&self) -> String {
        format!("actr-storage:v{}:{}", self.version, hex::encode(self.digest))
    }
}
```

`StorageNamespace` 与 `crates/actr-pack` 的 `StorageNamespaceId` 是两个刻意分开的类型：
`StorageNamespaceId`（在 `crates/actr-pack`）是身份哈希的计算结果，只含 32 字节 digest；
`StorageNamespace`（在 `crates/actr-platform-traits`）是传给 provider 的传输类型，额外携带 layout
version。二者分层不同——pack 负责「算出来」，platform-traits 负责「传出去」——Hyper 在
`storage_namespace_for()` 中通过 `into_bytes()` 显式桥接。不要因字段相似就合并它们。

修改 trait 签名：

```rust
pub trait PlatformProvider: Send + Sync {
    async fn secret_store(
        &self,
        namespace: &StorageNamespace,
    ) -> Result<Arc<dyn KvStore>, PlatformError>;
}
```

platform 分支不调用 filesystem path resolver，而是：

```rust
let namespace = storage_namespace_for(manifest)?;
let store = platform.secret_store(&namespace).await?;
```

接口文档必须明确：

- namespace 不是路径；
- provider 不得假定 namespace 是绝对路径；
- provider 自己拥有并保护真实 storage root；
- namespace 格式带版本，未来升级不得静默复用不同语义。

这里不提供把新类型自动转回旧 `&str` API 的默认适配器。所有自定义 provider 都应收到
编译错误并显式迁移，避免原实现继续执行 `Path::new(namespace)`：

- filesystem provider 只使用 `namespace.digest()` 的小写 hex 作为路径分量；
- key-value provider 可使用 `namespace.opaque_key()` 作为逻辑 key；
- 日志可使用 opaque key，但不得把它作为 filesystem path。

`NativePlatform::secret_store()` 不再 `Path::new(namespace_string)` 透传，而是持有自己
校验过的 `SecureStorageRoot`，并调用 `ActorStore::open_under(root, namespace)`。digest
到路径的转换仍只发生在 `SecureStorageRoot::path_for()` 内部。这样
`actr-storage:v1:...` 中的冒号不会进入 Windows 文件路径，也不会在 Unix 上变成意外的
相对路径。`path_for()` 必须匹配并处理 namespace version，遇到不支持的版本显式报错。

### 4.7 ActorStore：root-aware 打开与 symlink 防护

新增 public `ActorStore::open_under(root: &SecureStorageRoot, namespace:
&StorageNamespace)`。

`ActorStore` 当前经 `crates/actr-hyper/src/lib.rs:197,265` 公开重导出，
`ActorStore::open(path)` 也是 public；如果只新增 `open_under()` 而保留 raw open，外部
调用方仍可绕过 root、hash、祖先检查和 NOFOLLOW。

本次 breaking change 明确处理为：

```rust
impl ActorStore {
    pub async fn open_under(
        root: &SecureStorageRoot,
        namespace: &StorageNamespace,
    ) -> HyperResult<Self> {
        let db_path = root.prepare_database_path(namespace).await?;
        Self::open_unchecked(&db_path).await
    }

    // 只打开已存在的父目录，并始终带 SQLITE_OPEN_NOFOLLOW。
    // 仅供本 crate 的已校验实现和窄范围单元测试复用，不是 public 安全边界。
    pub(crate) async fn open_unchecked(db_path: &Path) -> HyperResult<Self> {
        // SQLite implementation
    }
}
```

Hyper 的 direct filesystem 分支调用：

```rust
let namespace = storage_namespace_for(manifest)?;
let store = ActorStore::open_under(&inner.secure_storage_root, &namespace).await?;
```

要求：

- 删除或降级现有 public `ActorStore::open(path)`；
- `#[doc(hidden)]` 不够，它只隐藏文档，仍允许外部绕过；
- `platform-native` 改用 public `open_under()`；
- `lib_tests.rs`、`storage/db_tests.rs` 统一构造临时 `SecureStorageRoot` 后调用
  `open_under()`；
- 如果某个单元测试确实需要任意路径，只能在 crate 内调用 `open_unchecked()`；
- `ActorStore` 类型可以继续公开，但 public API 不再接收任意数据库路径。

`SecureStorageRoot` 初始化和开库流程：

1. 用 `symlink_metadata` 检查配置 root 本身；
2. 创建 root，并在 Unix 设置为进程私有权限（目标 `0700`）；
3. canonicalize root，保存在对象内部；
4. 只按固定的 `actors/v1/<hex>` 逐级创建目录；
5. 每一级创建前后检查不是 symlink；Windows 同时拒绝 reparse point；
6. canonicalize 数据库父目录，按路径分量确认仍位于 canonical root 下；
7. 数据库文件若已存在，用 `symlink_metadata` 再次拒绝 symlink/reparse point；
8. SQLite 使用 `open_with_flags()` 和 `SQLITE_OPEN_NOFOLLOW`；
9. WAL、SHM、journal 与主库放在同一个私有 hash 目录。

`path_for()` 只做无副作用的固定路径推导，供诊断 API 使用；
`prepare_database_path()` 必须逐级执行第 4–7 步，不能退回一次
`create_dir_all(candidate.parent())`。`open_under()` 只能在 prepare 成功后调用 SQLite，
从而让祖先检查真正处于开库信任边界内。

SQLite 打开示意：

```rust
let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
    | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
    | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW;

let conn = rusqlite::Connection::open_with_flags(&db_path, flags)?;
```

不能使用一次独立的 `std::fs::OpenOptions + O_NOFOLLOW` 代替，因为 SQLite 会按路径重新
打开文件，不会复用那个 `File`。

#### symlink 安全声明

以上保证建立在：

> storage root 及其父目录由受信任管理员/Actr 创建，非特权攻击者不能并发改写。

如果威胁模型包含能以同一 UID 并发替换目录的本地攻击者，canonicalize 后再 open 仍有
TOCTOU，需要 Unix `openat/openat2`、Windows handle-relative API 或 SQLite 自定义
VFS。该能力应单独跟踪，本文不声称已完全覆盖。

### 4.8 Bootstrap 顺序保持不变

保持：

```text
1. 身份校验并计算 namespace
2. 安全打开 ActorStore / platform secret store
3. 构造 RegisterRequest
4. AIS 注册
5. 检查 AIS 响应
6. store.batch(signing_pubkey, signing_key_id)
7. 返回 RegisterOk
```

原因：

- 开库、目录权限、SQLite、WAL 任一失败时，不向 AIS 创建状态；
- AIS 成功后的本地失败点仍只有 `store.batch`；
- 若把开库移动到 AIS 之后，AIS 已签发 ActrId/renewal token 后又可能因目录或 SQLite
  失败而返回整体失败；重试会产生新的 ActrId/token，因为当前 AIS 注册不是幂等的。

AIS 明确拒绝时可能留下一个空的 hashed-v1 store。这不是 Issue #350 的越权风险；在
“不保留旧数据”的前提下，可由诊断/清理工具删除未包含有效初始化标记的空 store。

原验收项“AIS 拒绝不建库”应从本 Issue 移除或改为单独需求。如果必须保留，必须先给
AIS 注册增加稳定 idempotency key，并保证重试返回同一逻辑 ActrId，再考虑延迟开库。

### 4.9 错误处理

Hyper 统一返回：

```rust
HyperError::InvalidManifest(format!("invalid actor identity: {error}"))
```

原因：

- `HyperError` 是公开且未标记 `#[non_exhaustive]`；
- 新增 enum 变体会让外部穷举 `match` 源码编译失败；
- 路径安全身份本来就是 manifest 运行时合法性的一部分。

Pack 层复用现有 `PackError::InvalidPackage`，不为本修复扩大公开错误枚举。

---

## 五、测试方案

### 5.1 身份与 namespace hash

- manufacturer/name 与 `Name::new()` 一致：
  - 接受 `acme`、`EchoService`、`x..y` 和满足 `Name` 规则的 Unicode；
  - 拒绝 `1acme`、空字符串、超过 32 UTF-8 字节、非法首尾字符；
- version 接受 `1.0.0`、`1.0.0-rc.1`、`1.0.0+build`、`v1`；
- version 拒绝空、超过 64 字节、`: / @ \`、空白、控制字符和非法首尾字符；
- `ActrType::validate()` 与 `to_string_repr()/from_string_repr()` 对所有接受值 round-trip；
- package、Registry 和 AIS 对同一组合法/非法身份给出一致结果；
- 相同 identity 每次生成相同 hash；
- `("ab", "c")` 与 `("a", "bc")` 不同，证明长度前缀无拼接歧义；
- `Evil` 与 `evil` 不同；
- 对所有通过 `Name` 的 Unicode 身份，最终路径都只含 ASCII hex；
- hash 输出严格为 64 位小写十六进制。

### 5.2 信任边界

- 合法签名但身份非法的包被 `actr_pack::verify*()` 拒绝；
- 自定义 `TrustProvider` 返回非法 `VerifiedPackage` 时，
  `Hyper::verify_package()` 拒绝；
- 同一 fake provider 经过 `Node::attach()` 时，在 workload 加载前拒绝；
- `RegistryTrust` 遇到任一非法身份字段或非法 key ID 时不发 HTTP 请求；
- Registry URL 测试覆盖 `/`、`?`、`&`、`#` 等编码字符；
- 证书 cache key 不因字符串拼接产生碰撞。

### 5.3 存储布局与 containment

- 默认 root 生成固定的 `actors/v1/<hash>/store.sqlite3`；
- 自定义外置 `storage_root=/mnt/ssd/actr` 正常工作；
- 配置仍含 `storage_path_template` 时返回明确迁移错误，不能静默忽略；
- TOML 中的相对 `storage_root` 按 `actr.toml` 所在目录解析，builder 中的相对值按
  `config.data_dir` 解析；
- 删除 `NamespaceResolver` 后生产代码和测试中无残留引用；
- manifest 中的 `/`、`\`、`..`、Windows drive/UNC 文本不会出现在最终路径；
- 大小写和 Unicode 身份在大小写不敏感文件系统上仍映射到不同 hex 路径；
- `resolve_storage_path()` 和 bootstrap 使用同一个 namespace 计算函数；
- public/direct manifest 调用仍会重新校验，不依赖调用方先执行 verify。

### 5.4 Symlink

在 Unix 临时目录测试：

- storage root 本身是 symlink：拒绝；
- `actors` 是 symlink：拒绝；
- hash 目录是 symlink：拒绝；
- `store.sqlite3` 是 symlink：SQLite NOFOLLOW 拒绝；
- 正常目录可创建数据库、WAL、SHM；
- canonical parent 必须位于 canonical root 下。

Windows 增加 symlink/junction/reparse-point 对应测试；无权限创建 symlink 的 CI 环境应
显式 skip，而不是把拒绝逻辑视为已覆盖。

### 5.5 Bootstrap 与 platform

- storage root 不可写/SQLite open 失败：断言 AIS 未收到请求；
- AIS 明确拒绝：允许留下空 store，但不得写入 signing key；
- AIS 成功：signing key 正确持久化；
- `store.batch` 失败：明确记录 AIS 部分成功状态；
- fake platform provider 使用新的 `StorageNamespace` 方法签名并检查 version/digest；
- 自定义 provider fixture 必须修改后才能编译，证明不存在裸 `&str` 静默兼容；
- `StorageNamespace::opaque_key()` 输出 `actr-storage:v1:<hash>`，但 native provider
  不得把该字符串当路径；
- native provider 将 opaque namespace 映射到自己的 root 下；
- Hyper、platform-native 和 storage 测试都不再调用 public `ActorStore::open(path)`。

---

## 六、兼容性与行为影响

### 6.1 明确接受的 breaking change

| 方面 | 影响 |
|---|---|
| 旧 ActorStore | 不读取、不迁移；升级后使用全新 hashed-v1 路径 |
| 默认存储路径 | 改为 `{storage_root}/actors/v1/<hash>/store.sqlite3` |
| 旧 `storage_path_template` 配置 | 启动时显式报错；外置存储必须改配 `storage_root` |
| `HyperConfig` struct literal | 新增/移除公开字段可能导致外部 Rust 代码重新编译失败 |
| package 身份规则 | manufacturer/name 必须满足 `Name`；version 必须满足新的 1–64 字节 ASCII allowlist；不合规的已有 `.actr` 包会被拒绝 |
| 自定义 `PlatformProvider` | `secret_store(&str)` 改为 `secret_store(&StorageNamespace)`；实现方必须重新编译并显式适配 |
| provider-backed 旧数据 | namespace key 改为 hashed-v1；本文不要求 provider 回读或迁移旧 key 下的数据 |
| `ActorStore` 调用方 | public `ActorStore::open(path)` 被移除或降为 crate-private；外部代码改用 `open_under()` |
| 手工读取旧数据库的程序 | 不再自动找到旧路径 |

因此提交和发布说明应使用 breaking 标记，例如：

```text
fix!(hyper): isolate actor storage with verified hashed namespaces
```

如果项目版本策略不允许安全修复使用 `fix!`，应拆成：

1. 核心 verifier/sink 拒绝；
2. storage layout breaking change。

但两部分必须在同一发布版本完成，不能把 hash 留成“可选后续”，否则大小写/Unicode
冲突验收仍未闭环。

### 6.2 不应再声称的内容

文档和 PR 描述不得声称：

- “对现有程序零影响”；
- “零迁移且路径不变”；
- “WASM 不受影响”；
- “`SQLITE_OPEN_NOFOLLOW` 单独解决全部 symlink”；
- “AIS 拒绝一定没有任何文件系统对象”；
- “自定义 PlatformProvider 不需要重新编译或行为适配”；
- “已有 `.actr` 包只要签名有效就一定继续可用”；
- “保留 public raw open 不影响安全边界”。

Native host 加载的 WASM 包仍经过 Hyper 的 package verification 和 ActorStore 路径，
因此属于本修复影响范围；只有编译目标本身为 `wasm32` 的 host 代码路径可能被 cfg 排除。

### 6.3 运行时升级行为

- 已运行节点在进程退出前不改变已打开连接；
- 重启后使用新 namespace，并看到空的 hashed-v1 store；
- 注入 provider 的节点同样使用新 namespace key，不回退到旧 key；
- 没有旧数据 fallback，避免不安全旧路径重新进入系统；
- 发布前必须确认所有部署都接受该数据重置前提。

---

## 七、验收标准

| # | 验收项 | 实现 |
|---|---|---|
| 1 | 协议与存储只有一套身份合法性规则 | `ActrType::validate()` 复用 `Name` 并校验 version |
| 2 | `:`、`@`、`/` 的处理没有解释空间 | 三字段统一拒绝并覆盖 round-trip 测试 |
| 3 | 内置包验证强制身份校验 | `actr_pack::verify*()` 返回前校验 |
| 4 | 自定义 TrustProvider 不能绕过 | Hyper 统一 provider 后置校验 |
| 5 | 非法身份在 Registry lookup、workload、AIS 和文件系统操作前拒绝 | lookup 预检 + `verify_package_for_hyper()` + 注册入口复检 |
| 6 | manifest 身份无法控制文件路径分量 | 文件路径只使用固定文本和 SHA-256 hex |
| 7 | Unix/Windows 路径穿越无效 | 原始身份不进入路径 |
| 8 | 大小写/Unicode 文件系统折叠不共享 namespace | 原始 UTF-8 身份映射为独立 hash |
| 9 | 旧模板不能静默继续或被忽略 | 删除模板能力；旧配置通过 tombstone 显式报错 |
| 10 | 自定义外置存储仍可配置 | 通过显式 `storage_root` |
| 11 | PlatformProvider 语义变化不会静默发生 | trait 接收强类型 `StorageNamespace`，旧实现编译失败 |
| 12 | raw ActorStore open 不能绕过 root-aware 入口 | public API 只提供 `open_under()` |
| 13 | symlink 在声明的 root 独占威胁模型内被拒绝 | secure root + ancestor check + SQLite NOFOLLOW |
| 14 | 开库失败不产生 AIS 注册状态 | 保持开库先于 AIS |
| 15 | 错误 API 不新增公开枚举分支 | 复用 `InvalidManifest` |

原 Issue 若仍把“AIS 拒绝不建库”列为硬验收，合并前必须：

- 修改该验收项；或
- 先完成 AIS 注册幂等化。

不能仅通过交换开库和 AIS 调用顺序来宣称满足一致性要求。

对于自定义 `TrustProvider`，Hyper 无法约束 provider 在返回前是否自行发起网络请求；
Hyper 的保证从 provider 返回边界开始。内置 `RegistryTrust` 则通过验签前完整身份预检
保证坏身份不会触发证书 HTTP lookup。

---

## 八、落地清单

### crates/actr-protocol

- [ ] 在 `ActrType` 上新增唯一的 `validate()`
- [ ] manufacturer/name 复用 `Name::new()`，不复制规则
- [ ] 为 version 实现 1–64 UTF-8 字节、ASCII allowlist 的唯一 validator
- [ ] `ActrType::from_string_repr()` 构造后调用同一个 `validate()`
- [ ] 明确记录 `:`、`@`、`/` 在三个字段中都非法
- [ ] 修正 `Name` 文档和错误文本：当前实现的 32 上限是 UTF-8 字节数，不是字符数
- [ ] 增加 `ActrType` 合法/非法矩阵和 `to_string_repr()/from_string_repr()` round-trip 测试

### crates/actr-pack

- [ ] 增加 `actr-protocol.workspace = true` 依赖并确认无 dependency cycle
- [ ] 新增 `crates/actr-pack/src/identity.rs`
- [ ] `ValidatedActorIdentity::from_manifest()` 委托 `ActrType::validate()`
- [ ] 实现 `StorageNamespaceId`、域分隔 hash、`into_bytes()` 和 `to_hex()`
- [ ] 在 `crates/actr-pack/src/lib.rs` 导出所需类型
- [ ] 在 `verify()`、`verify_bounded()`、`verify_with_limits()` 公共实现路径接入校验
- [ ] 增加 identity/hash 单元测试

### crates/actr-hyper trust

- [ ] 新增统一 `verify_package_for_hyper()`
- [ ] `Hyper::verify_package()` 改走统一入口
- [ ] `load_workload_package_inner()` 改走统一入口
- [ ] `RegistryTrust` 在 key lookup 前做 locator 预检
- [ ] `MfrCertCache` 使用 URL builder，不字符串拼接 URL
- [ ] cert cache key 改为结构化 key
- [ ] 增加 fake custom provider 绕过测试

### actrix/AIS

- [ ] register handler 在生成 ActrId、ACL key 或持久化状态前调用 `ActrType::validate()`
- [ ] `handlers.rs` 的 `to_string_repr()` ACL key 只使用已校验的 ActrType
- [ ] 增加绕过 Hyper 直接提交非法 register request 的拒绝测试
- [ ] 用与 protocol/package 相同的合法/非法身份矩阵做一致性测试

### crates/actr-hyper storage

- [ ] 用 `storage_root` 替换 `storage_path_template`
- [ ] 为旧 `storage_path_template` 保留只报错的 Serde tombstone
- [ ] TOML 相对 `storage_root` 按配置目录解析，builder 相对值按 `config.data_dir` 解析
- [ ] 删除 `DEFAULT_STORAGE_TEMPLATE`
- [ ] 删除 `HyperConfig` 模板字段、Debug 输出和 `with_storage_template()`
- [ ] 删除 `NamespaceResolver`、变量 `HashMap`、环境变量展开和全部模板测试
- [ ] 更新示例配置和用户文档，确认生产代码无 `NamespaceResolver` 残留
- [ ] Hyper 初始化创建、检查并 canonicalize storage root
- [ ] canonical root 只保存于 `HyperInner`
- [ ] `SecureStorageRoot::path_for()` 唯一实现 `actors/v1/<hash>/store.sqlite3`
- [ ] `resolve_storage_path()` 与 bootstrap 共用相同推导函数
- [ ] 保持 ActorStore open 在 AIS register 之前
- [ ] 复用 `HyperError::InvalidManifest`

### platform/storage

- [ ] 在 `actr-platform-traits` 定义 `StorageNamespace`，补充所需 hex 依赖
- [ ] 把 `PlatformProvider::secret_store()` 改为接收 `&StorageNamespace`
- [ ] 更新所有 provider、mock、fixture 和下游实现；不提供裸字符串兼容适配器
- [ ] Hyper platform 分支传强类型 namespace
- [ ] NativePlatform 使用 digest 映射自己的 root，不把 opaque key 当路径
- [ ] 新增 `SecureStorageRoot` / `ActorStore::open_under`
- [ ] `prepare_database_path()` 逐级创建并检查祖先，禁止退回裸 `create_dir_all()`
- [ ] 删除或降级 public `ActorStore::open(path)` 为 crate-private `open_unchecked()`
- [ ] 迁移 platform-native、`lib_tests.rs`、`storage/db_tests.rs` 的 raw open 调用
- [ ] 检查 root、祖先 symlink 和 Windows reparse point
- [ ] SQLite 使用 `SQLITE_OPEN_NOFOLLOW`
- [ ] 验证 WAL/SHM 位于同一私有 namespace 目录

### 验证与发布

- [ ] 跑 `cargo fmt`
- [ ] 跑 `cargo clippy --fix --lib --bins --all-targets --allow-dirty`
- [ ] 跑 `cargo test -p actr-pack`
- [ ] 跑 `cargo test -p actr-hyper`
- [ ] 跑 platform-native 相关测试
- [ ] 跑 Unix symlink 测试
- [ ] 在 Windows CI 跑 drive/UNC/reparse-point 测试
- [ ] 用 `rg` 确认没有生产 `NamespaceResolver`、旧模板 API 或 public raw open 残留
- [ ] 发布前按**字节**扫描在用 `.actr` 的三段身份（`Name` 的 32 上限按 UTF-8 字节计，CJK 约 10 字即超限），列出不满足新规则的包并重新发布
- [ ] 同时扫描配置文件与 CLI 中的 ActrType 字符串：`from_string_repr()` 现在会走 `validate_version()`，写法不规范的 version（含空格、非 ASCII 等）将从「能解析」变为「报错」
- [ ] 盘点下游自定义 provider、`ActorStore::open(path)` 和模板配置调用方
- [ ] 更新配置示例和 release notes，突出身份规则、provider trait 和 ActorStore API 变更
- [ ] 在 Issue #350 同步修订 AIS 拒绝验收项
- [ ] 发布前确认部署方接受旧 ActorStore 和 provider-backed namespace 数据重置

---

## 九、剩余风险与非目标

1. **同 UID 本地并发攻击**：若攻击者能并发替换 storage root 内目录，portable
   canonicalize + open 仍可能有 TOCTOU；需要 capability filesystem 或 SQLite VFS。
2. **可信 dynclib**：dynclib 本身拥有宿主代码执行能力，路径修复不会把它变成沙箱。
3. **AIS 注册幂等性**：当前注册成功后 `store.batch` 失败仍可能形成部分成功；这是既有
   分布式一致性问题，应单独设计。
4. **旧路径恢复**：本文明确不提供 fallback；任何自动读取旧路径都会重新引入迁移和
   路径安全问题。
5. **Hash 碰撞**：使用完整 SHA-256，风险按密码学碰撞模型处理；不得截断到短 ID。

---

## 十、参考

- [Actrium/actr#350](https://github.com/Actrium/actr/issues/350)
- `crates/actr-protocol/src/actr_ext.rs`
- `crates/actr-protocol/src/name.rs`
- `crates/actr-pack/src/manifest.rs`
- `crates/actr-pack/src/verify.rs`
- `crates/actr-hyper/src/verify/trust.rs`
- `crates/actr-hyper/src/verify/cert_cache.rs`
- `crates/actr-hyper/src/config.rs`
- `crates/actr-hyper/src/lib.rs`
- `crates/actr-hyper/src/storage/db.rs`
- `crates/actr-platform-traits/src/platform.rs`
- `crates/actr-platform-native/src/platform.rs`
- `crates/loran-ais/src/handlers.rs`
- CWE-22: Improper Limitation of a Pathname to a Restricted Directory
- CWE-73: External Control of File Name or Path
