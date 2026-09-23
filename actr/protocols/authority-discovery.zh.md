# Actr Authority Discovery

语言：[English](authority-discovery.md) | **简体中文**

状态：初始草案；尚未向 IANA 注册。

本文定义 Actr 客户端如何发现某个 Actr authority 的公开服务，计划作为 IANA `actr`
Well-Known URI 登记项所引用的规范。

## 1. 范围

每个域名在 `https://<domain>/.well-known/actr` 发布一份公开的 authority descriptor，说明
该域名提供哪些 Actr 服务，并可携带 Realm 签名 key、Publisher Profile 和 Realm 链接前缀。
它不定义运行时 Actor discovery、signaling、认证或 ICE configuration 的协议；`actr:` URI
由 [Actr URI 规范](uri.zh.md) 定义。

本文中的大写关键词 **MUST**、**MUST NOT**、**SHOULD**、**SHOULD NOT** 和 **MAY**
按 BCP 14 解释。

## 2. Discovery URI

对于域名 `example.com`，discovery resource 为：

```text
https://example.com/.well-known/actr
```

它使用 HTTPS 默认端口，只适用于该域名本身，不会自动适用于父域或子域。

版本 1 不定义 `/.well-known/actr` 下的额外 path component，任何规范都不得为这类子路径
赋予含义。特定于某个 Realm 或某个 publisher 的信息不通过子路径发现：descriptor 可以携带
第 4.4、4.5 节定义的签名成员，并由 `realms` relation（第 4.3 节）定位
[Actr Realm Profile](realm-profile.zh.md) 定义的 Realm Profile。因此一个域名可以
命名多个 Realm。需要独立控制 discovery document 的部署应使用不同的 Actr
authority，通常是不同的 hostname。

## 3. 请求

客户端 **MUST** 通过 HTTPS 获取该 resource，并按正常 HTTPS 规则验证服务器证书。

服务器 **MUST** 支持：

```http
GET /.well-known/actr HTTP/1.1
Host: example.com
Accept: application/json
```

该请求用于公开 bootstrap discovery。客户端 **MUST NOT** 发送 `Authorization` header、
cookie 或其他应用 credential。服务器 **SHOULD** 支持 `HEAD`，返回与 `GET` 相同的
response header，但不返回 response body。本文不定义其他 method 的语义。

## 4. 成功响应

成功响应 **MUST** 使用 `200 OK` 状态和 `application/json` media type。response body
**MUST** 是符合
[`authority-descriptor.schema.json`](../../contracts/actr/authority-descriptor.schema.json)
的 UTF-8 JSON object。

示例：

```json
{
  "version": 1,
  "authority": "actr://example.com",
  "services": [
    {
      "rel": "signaling",
      "href": "wss://signal.example.com/actr",
      "protocol": "actr-signaling/1"
    },
    {
      "rel": "identity",
      "href": "https://identity.example.com/actr"
    },
    {
      "rel": "discovery",
      "href": "https://directory.example.com/actr"
    },
    {
      "rel": "ice-config",
      "href": "https://rtc.example.com/actr/ice",
      "protocol": "actr-ice-config/1"
    },
    {
      "rel": "contracts",
      "href": "https://contracts.example.com/actr"
    }
  ],
  "links": ["/i/"]
}
```

JSON member name **MUST** 唯一。若 document 出现重复 member name、缺少 required member
或使用不支持的 `version`，客户端 **MUST** 拒绝。客户端 **MUST** 忽略未知 member，使
兼容性扩展不需要新的 discovery URI。

### 4.1 `version`

`version` 标识 authority descriptor format。本文定义整数版本 `1`。descriptor format 的
演进应通过该 member 表达，而不是使用 `actr-v2` 之类的新 Well-Known URI suffix。

### 4.2 `authority`

`authority` 是本文档所描述域名的规范 `actr:` URI，即 `actr://<domain>`，不含 path、
query 或 fragment。

应用 Actr URI 规范的 normalization rule 后，客户端 **MUST** 验证该值与构造 discovery
request 时使用的 authority 一致。redirect 不改变此绑定关系。

### 4.3 `services`

`services` 是非空的 service link 数组。authority 公布多个 endpoint 时，可以有多个条目
使用同一 relation。

每个 service link 包含：

- `rel`：服务角色；
- `href`：服务 endpoint 的 absolute URI；
- `protocol`：可选，表示 endpoint 支持的 Actr 协议名称和 major version。

版本 1 定义以下 core relation：

| Relation | 用途 |
| --- | --- |
| `signaling` | 建立 Actor 连通性并交换 session negotiation 消息。 |
| `identity` | 解析 identity、key 和 authentication metadata。该 authority 作为 Realm Provider 时，其 `href` 也是 Realm Provider 账号登录所用的 OAuth 2.0 授权服务器标识（issuer identifier，RFC 8414）（[Actr Realm Profile](realm-profile.zh.md) 第 7.3 节）。 |
| `discovery` | Actor 启动后，按依赖、contract 和 capability 发现可用的 Actor instance。 |
| `ice-config` | 按 [Actr ICE Configuration](ice-configuration.zh.md) 获取 ICE server 配置，并在授权后取得短期 TURN credential。 |
| `contracts` | 获取公开的 Actor contract 和 interface description。 |
| `realms` | 从存放位置获取以该域名命名的各 Realm 的签名 Realm Profile，见 [Actr Realm Profile](realm-profile.zh.md)。protocol 值为 `actr-realm-profile/1`。 |
| `realm-management` | 接受该 authority 作为 Realm Provider 所服务 Realm 的 Realm 管理请求（`POST`），见 [Actr Realm Profile](realm-profile.zh.md) 第 8 节。protocol 值为 `actr-realm-management/1`。 |

extension relation 值 **MUST** 是由扩展作者控制的 absolute URI，以避免与短 core
relation name 冲突。

descriptor **MUST NOT** 包含 access token、password、长期 TURN credential、private key
或用户专属数据。特别地，`ice-config` 只标识一个 endpoint；它不会在这份公开文档中嵌入
私有 TURN credential。

descriptor 的 `signaling`、`ice-config` link 服务于 authority 自身，与以该域名命名的
Realm 无关。客户端 **MUST** 只通过 Realm 已接受 Realm Profile 指定的 Realm
Provider 访问该 Realm；Realm Profile 取不到、无效或已退役时，**MUST NOT** 使用包括这些
link 在内的任何其他 Realm Provider。

### 4.4 `realm_signing_keys`

`realm_signing_keys` 是可选 member。存在时，该域名即 Realm 名 `<realm_id>@<domain>` 中的
域名。

该 member 公布 Realm 签名 key，即以该域名命名的所有 Realm 的长期信任根：

```json
"realm_signing_keys": {
  "sequence": 3,
  "keys": [
    { "id": "rs-2026-1", "algorithm": "Ed25519", "public_key": "<base64>", "status": "active" },
    { "id": "rs-2025-1", "algorithm": "Ed25519", "public_key": "<base64>", "status": "retired" }
  ],
  "rotations": [
    { "from": "rs-2025-1", "to": "rs-2026-1", "sequence": 2, "signature": "<base64>" }
  ],
  "signature": { "key_id": "rs-2026-1", "algorithm": "Ed25519", "value": "<base64>" }
}
```

- `sequence` 是非负整数，该 member 每次变更都递增。
- `keys` 列出 Realm 签名 key。每个 key 有唯一 `id`、`algorithm`（版本 1 只定义 `Ed25519`）、
  base64 `public_key` 与 `status`。`active` key 用于签名；`retired` key 不再签名，但它
  此前所做的签名（如已有 Realm Profile 上的）仍然有效。由于签名不带可信时间，`retired`
  不提供安全保证；吊销 key 必须将其移除。从 `keys` 中移除某个 key 即吊销
  它：它签过的任何内容都不再被接受，因此域名所有者应移除可能泄露的 key，并重签受影响
  的 Profile。至少一个 key **MUST** 为 `active`。
- `rotations` 是 Realm 签名 key 轮换链。每一项由 `from` 指向的 key 对
  `{"from": ..., "to": ..., "public_key": ..., "sequence": ...}` 的 JCS 形式签名，
  其中 `public_key` 取自 `keys` 中 `to` 对应的 key，`sequence` 是引入它的那次 member
  `sequence`；`signature` 为该 detached 签名。
- `signature` 是某个 `active` key 对去掉 `signature` 后的该 member 按 RFC 8785 JCS
  规范化结果所做的 detached Ed25519 签名，覆盖 `sequence`。

首次接受某域名的 Realm 签名 key 依赖 HTTPS（Web PKI）或企业策略预置的 key pin。此后，
已固定该域名的客户端或 Realm Provider 把曾接受过的每个 key 都视为已固定，并且：

- **MUST** 拒绝 `signature` 验证失败的 member；
- **MUST** 忽略 `from` 不在当前 `keys` 中的 Realm 签名 key 轮换项；这类轮换项不使 member 无效，只有尚未
  接受的 key 才需要可验证的链；
- **MUST** 只通过从已固定 key 出发的有效轮换链接受尚未接受的 key，且每个新轮换项的
  `sequence` 高于已接受的 member `sequence`；
- **MUST** 拒绝由此前已接受的 member 中已标为 `retired` 或已移除的 key 签名的 member 或新轮换项；
- **MUST** 拒绝经已固定链签名、但 `sequence` 低于已接受值的 member；
- **MUST** 把签名 key 无法链接到已固定 key 的 member 视为身份变化：不做 `sequence`
  比较，要求用户或运营者重新确认信任，确认后以新 member 的 `sequence` 为基线。

该 member **MUST NOT** 列出以该域名命名的 Realm，避免从 descriptor 枚举 Realm 名。

### 4.5 `publisher`

`publisher` 是可选 member，内嵌该 authority 的签名 Publisher Profile。存在时，该
域名即 Publisher ID。它取代独立的 publisher bootstrap 文件；
规范 **MUST NOT** 为此另行定义 `/.well-known/actr/publisher.json` 或
`/.well-known/actrium-publisher.json` 等 Well-Known 位置。

Publisher Profile 使用 snake_case member 名与 `"schema": "actr.publisher/v1"`，其完整
格式、key 角色与轮换规则留给 Actor Package 规范定义（第 10 节）。在 descriptor 层面：

- `domain` **MUST** 等于 `authority` 的 host；
- Publisher Profile 由 `active` 的 Publisher Profile 签名 key 签名，只凭自身签名验证，与 HTTPS 通道无关；
- HTTPS（或企业 pin）只提供域名与 publisher key 之间的首次绑定；
- 接受首个 Publisher Profile 后，sequence 与轮换校验按 Actor Package 规范执行，不受 descriptor
  的 HTTP 缓存语义影响。

登记 publisher 的 Realm Provider 使用 `register_publisher` 请求：采用
[Actr Realm Profile](realm-profile.zh.md) 第 8 节的 Actr 管理信封，由 `active` 的
Publisher Profile 签名 key 签名，而不是在公开 descriptor 中放置 challenge。

### 4.6 `links`

`links` 是可选 member：路径前缀的数组，每项以 `/` 开头并以 `/` 结尾，至少含一段（不能只是 `/`），
例如 `["/i/"]`。它声明本域名下哪些 https 路径是 Realm 链接：`https://<domain><前缀><realm_id>…`
对应 `actr://<domain>/<realm_id>…`（[Actr URI 规范](uri.zh.md) 第 3 节）。

- 前缀由域名自行选择，本规范不定义默认值。
- Actr Host **MUST** 只按这里的声明转换 https 地址，**MUST NOT** 凭路径外观推断。
- 多个前缀同时匹配时取最长者。

### 4.7 签名部分与未签名部分

descriptor 整体仍是由 HTTPS 保护、未签名的公开文档。`realm_signing_keys` 与 `publisher`
只凭各自的签名验证；其中一个的有效签名不为任何其他 member 提供认证。第 6 节的文档大小
上限适用于包含签名 member 在内的整个 descriptor。

## 5. 错误、redirect、缓存与 CORS

不支持 Actr discovery 的 origin **SHOULD** 返回 `404 Not Found`。其他失败使用相应的
正常 HTTP status code。错误 response body 不是 authority descriptor。

服务器 **SHOULD** 直接提供 discovery resource。客户端 **MAY** 跟随最多五次 redirect，
但每个 redirect target **MUST** 使用 HTTPS。客户端 **MUST NOT** 将 redirect target 当成
新的 Actr authority，且 **MUST** 继续用原始 Actr authority 验证返回的 `authority`。

服务器 **SHOULD** 发送明确的 `Cache-Control` freshness information。客户端和中间节点
**MUST** 应用正常 HTTP caching semantics。服务器未给出 freshness lifetime 时，本草案
不定义隐含的有效期。

因为 descriptor 公开且不含 credential，支持 browser-based Actr client 的部署
**SHOULD** 发送：

```http
Access-Control-Allow-Origin: *
```

本规范不定义 credentialed cross-origin discovery。

## 6. 安全与隐私考虑

- 必须使用 HTTPS。客户端 **MUST NOT** 将 discovery 或被 redirect 的请求降级为明文
  HTTP。
- 控制 discovery document 就代表控制整个 Actr authority，必须相应限制其写权限。
- 客户端使用任何已公布服务前，必须验证 authority 绑定。
- 客户端必须验证每个 service URI，并仅允许该 relation 所支持的 scheme。特权进程使用
  service URI 时，实现还应防范 SSRF。
- descriptor 是公开、可预测且可缓存的，不得包含 secret、user identifier 或
  authorization state。
- 实现应限制 response size 和 JSON parsing 资源；建议默认文档上限为 64 KiB。

## 7. 版本与兼容性

兼容修订可以增加 optional member、新的 URI-valued extension relation 或澄清，而无需更改
`version`。`realms` relation 以及 `realm_signing_keys`、`publisher`、`links` member 都属于这类新增：
未实现它们的客户端直接忽略。若修订改变现有 member 含义，或使原本有效的 descriptor 失效，则需要新的
descriptor version。

不同 descriptor version 继续使用同一个 Well-Known URI：`/.well-known/actr`。

## 8. IANA 考虑

计划申请的 provisional 登记为：

```text
URI suffix: actr
Change controller: Actrium Project
Specification document: this document
Related information: none
```

该 Well-Known URI 允许的 URI scheme 是 `https`。

本文使用已登记的 `application/json` media type，不申请新的 media type。JSON Schema
属于 Actr 规范的一部分，不需要单独向 IANA 注册。

## 9. 参考资料

- [RFC 3986：Uniform Resource Identifier: Generic Syntax](https://www.rfc-editor.org/rfc/rfc3986.html)
- [RFC 8259：The JavaScript Object Notation Data Interchange Format](https://www.rfc-editor.org/rfc/rfc8259.html)
- [RFC 8615：Well-Known Uniform Resource Identifiers](https://www.rfc-editor.org/rfc/rfc8615.html)
- [RFC 9110：HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110.html)
- [RFC 9111：HTTP Caching](https://www.rfc-editor.org/rfc/rfc9111.html)

## 10. 待完成工作

本初始草案有意保留以下工作：

- 稳定 Actr URI 规范及其 authority 映射；
- 定义各 service protocol 及其允许使用的 URI scheme；
- 决定是否需要 service ordering、priority 或 failover metadata；
- 在 Actor Package 规范中定义 `actr.publisher/v1` Publisher Profile 格式；
- 为 `realm_signing_keys` 的轮换链 payload 定义机器可读 schema，并增加签名 test vector；
- 定义受限的 Realm 创建委派 key（版本 1 不提供）；
- 增加 interoperability fixture 和 negative test case；
- 确立 IANA 登记使用的长期公开规范地址和项目联系人。
