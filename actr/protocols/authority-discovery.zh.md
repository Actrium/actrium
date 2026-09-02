# Actr Authority Discovery

语言：[English](authority-discovery.md) | **简体中文**

状态：初始草案；尚未向 IANA 注册。

本文件记录当前中文草案；英文版尚待同步。

本文定义 Actr 客户端如何发现某个 Actr authority 的公开服务，计划作为 IANA `actr`
Well-Known URI 登记项所引用的规范。

## 1. 范围

`actr:` URI 标识某个 authority 下的 Actr 目标。authority 是组织目标的稳定名称，不一定是
signaling、identity、discovery 或 ICE 服务的实际网络位置。

Authority discovery 将该稳定 authority 映射到服务端点。它不定义 `actr:` URI 的 target
语法、Package resolution、运行时 Actor discovery、signaling 协议、认证协议或 ICE
configuration 响应；这些属于不同的 Actr 协议范围。URI 语法由
[Actr URI 规范](uri.zh.md) 定义，Target 到 Package 的解析边界由
[Actr Package Resolution](package-resolution.zh.md) 定义。

本文中的大写关键词 **MUST**、**MUST NOT**、**SHOULD**、**SHOULD NOT** 和 **MAY**
按 BCP 14 解释。

## 2. Discovery URI

对于 authority 为 `example.com` 的 `actr:` URI，discovery resource 为：

```text
https://example.com/.well-known/actr
```

Actr authority 明确包含端口时，HTTPS origin 使用同一端口；未包含端口时，使用 HTTPS
默认端口。

discovery document 仅适用于取得它的确切 HTTPS origin，不会自动适用于父域、子域或其他
端口。

版本 1 不定义 `/.well-known/actr` 下的额外 path component，也不定义 per-realm discovery
差异。需要独立控制 discovery document 的部署应使用不同的 Actr authority，通常是不同的
hostname。

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
    },
    {
      "rel": "packages",
      "href": "https://packages.example.com/actr/packages",
      "protocol": "actr-packages/1"
    }
  ]
}
```

JSON member name **MUST** 唯一。若 document 出现重复 member name、缺少 required member
或使用不支持的 `version`，客户端 **MUST** 拒绝。客户端 **MUST** 忽略未知 member，使
兼容性扩展不需要新的 discovery URI。

### 4.1 `version`

`version` 标识 authority descriptor format。本文定义整数版本 `1`。descriptor format 的
演进应通过该 member 表达，而不是使用 `actr-v2` 之类的新 Well-Known URI suffix。

### 4.2 `authority`

`authority` 是本文档所描述 authority 的规范 Actr authority URI。它包含 `actr` scheme 和
authority，但不包含 target path、query、fragment 或 user information。

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
| `identity` | 解析 identity、key 和 authentication metadata。 |
| `discovery` | Actor 启动后，按依赖、contract 和 capability 发现可用的 Actor instance。 |
| `ice-config` | 按 [Actr ICE Configuration](ice-configuration.zh.md) 获取 ICE server 配置，并在授权后取得短期 TURN credential。 |
| `contracts` | 获取公开的 Actor contract 和 interface description。 |
| `packages` | 将公开 URI Target 解析为入口 Actor Package 发布，并取得 `.actr` artifact。 |

extension relation 值 **MUST** 是由扩展作者控制的 absolute URI，以避免与短 core
relation name 冲突。

descriptor **MUST NOT** 包含 access token、password、长期 TURN credential、private key
或用户专属数据。特别地，`ice-config` 只标识一个 endpoint；它不会在这份公开文档中嵌入
私有 TURN credential。

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
`version`。若修订改变现有 member 含义，或使原本有效的 descriptor 失效，则需要新的
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
- 增加 interoperability fixture 和 negative test case；
- 确立 IANA 登记使用的长期公开规范地址和项目联系人。
