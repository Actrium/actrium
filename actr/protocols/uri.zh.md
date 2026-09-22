# Actr URI Scheme

语言：[English](uri.md) | **简体中文**

状态：初始草案；尚未向 IANA 注册。

本文定义 `actr:` URI scheme 的语法、规范化方式和解析模型，计划作为 IANA
`actr` URI scheme 登记项所引用的规范。

## 1. 目的与范围

`actr:` URI 是对 Actr authority，或该 authority 所发布应用入口的稳定逻辑引用。它同时是：

- 可由应用保存、比较和交换的标识符；
- bootstrap 引用，Actr Host 可由此解析入口 Actor Package、验证并启动应用。

`actr:` URI 不是直接的 signaling、WebSocket、WebRTC、STUN 或 TURN 传输地址。
authority 通过 [Actr Authority Discovery](authority-discovery.zh.md) 委派这些网络地址。

本文不将 Actr realm、publisher、Actor type、version 或 serial number 固定在 target
reference 的特定 path 位置。target reference 保持不透明，并按
[Actr Package Resolution](package-resolution.zh.md) 解析成一个入口 Actor Package 的发布
引用。

`actr:` URI 不用于寻址已经运行的 Actor instance。Actor 启动后所需的实例发现、路由、
signaling 和会话建立属于 Actr runtime 协议。

本文中的大写关键词 **MUST**、**MUST NOT**、**SHOULD**、**SHOULD NOT** 和 **MAY**
按 BCP 14 解释。

## 2. 语法

`actr` scheme 采用 RFC 3986 的通用分层 URI 语法。初始语法为：

```abnf
actr-uri        = "actr://" actr-authority [ "/" target-reference ]
actr-authority  = host [ ":" port ]
target-reference = target-segment *( "/" target-segment )
target-segment  = 1*( unreserved / pct-encoded / sub-delims / ":" / "@" )
```

`host`、`port`、`unreserved`、`pct-encoded` 和 `sub-delims` 引自 RFC 3986。
URI scheme 名称不区分大小写，但 Actr URI 的规范形式使用小写 `actr`。

本草案有意排除以下通用 URI 能力：

- authority 中的 `userinfo`；
- query；
- fragment；
- 空 target segment；
- dot segment `.` 和 `..`。

## 3. URI 组成部分

### 3.1 Scheme

scheme 名称为 `actr`，表示本文定义的 Actr 逻辑寻址与 bootstrap 模型，其自身并不选择
任何底层传输协议。

### 3.2 Authority

URI authority 是 DNS 名称或 IP literal，可带可选端口：

```text
actr://example.com
actr://example.com:8443
actr://[2001:db8::1]:8443
```

authority 是 Actr 目标稳定的管理及信任边界，也是 authority discovery 使用的 HTTPS
origin。未写端口时，authority discovery 使用 HTTPS 默认端口；写明端口时，其 HTTPS
origin 使用该端口。

Realm 不是 URI authority 的一部分。不含端口的 DNS authority 同时可以作为 Realm 名中的
域名：域名提供全局唯一、归属可验证的名称，其下每个 Realm 是一个应用实例，由 Realm label 标识
（第 7.1 节）。因此一个域名可以命名多个 Realm，而自身无需运行任何 Actr 基础设施。

通用 URI 语法中的 `userinfo` 在这里被禁止，因此 Actr URI 不会使用
`actr://user@example.com/...` 这样的形式。

### 3.3 Target reference

authority 后的可选 path 是 target reference：

```text
actr://example.com/echo
actr://example.com/actors/echo
```

这些例子只用于展示 path 形状；本草案不赋予其中 segment 名称任何含义。

空 target reference 标识 authority 本身。非空 target reference 由该 authority 公布的
Package service 解释，并解析成一个入口 Actor Package 的发布引用。通用 Actr URI parser
**MUST** 保留规范化后的 segment 序列，且 **MUST NOT** 根据固定 path 位置推断 realm、
publisher、version、Actor identity 或仓库文件路径。

target reference **MUST NOT** 解析成已经运行的 Actor instance、signaling endpoint 或当前
会话。authority 也 **MUST NOT** 要求客户端把 target reference 直接拼接到 Package service
URL；具体请求格式由 Package Resolution 协议定义。

target segment 区分大小写。允许的 ASCII 集合之外的字符及任意 octet，必须先编码为
UTF-8，再按 RFC 3986 进行 percent-encoding。

### 3.4 Query 与 fragment

本草案没有定义 query 和 fragment，合规 Actr URI 中 **MUST NOT** 出现这两部分。协议扩展
不得使用它们携带 credential、access token、private key 或其他秘密。

## 4. 解析模型

客户端收到 `actr:` URI 后，按以下逻辑步骤处理：

1. 解析并规范化 URI。
2. 提取 `host[:port]` authority。
3. 按 Actr Authority Discovery 的定义获取
   `https://<authority>/.well-known/actr`。
4. 验证返回的 descriptor 与原 authority 绑定。
5. URI 含 target reference 时，选择 descriptor 中 `rel=packages` 的 Package service。
6. 按 Package Resolution 协议把完整 target reference 提交给该服务，并取得不可变的
   Package 发布引用；客户端不得把 target 当作 URL path 直接拼接。
7. 从 Package service 指示的地址取得根 `.actr`，验证 Package identity、version、digest、
   发布者签名、Workload Profile 和 Actr Host 要求。
8. Actr Host 支持且本地策略允许时，安装并启动该入口 Actor；否则返回明确的 unsupported-profile、
   capability-denied 或其他安装错误。
9. Actor 启动后，再按 manifest、contract 和 runtime 协议发现并连接其依赖的 Actor instance。

因此，即使 authority 将 signaling 或其他基础设施迁移到不同网络端点，`actr:` URI 仍可
保持稳定。

## 5. 规范化与比较

序列化或比较 Actr URI 前，按以下规则规范化：

1. scheme 序列化为小写 `actr`。
2. 按 RFC 3986 规范化 host；DNS 名称使用小写，国际化 DNS 名称序列化为 IDNA ASCII
   形式。
3. 移除显式端口 `443`，因为它是 discovery HTTPS 的默认端口。
4. percent-encoded triplet 中的十六进制数字使用大写。
5. 将表示 unreserved 字符的 percent-encoded octet 解码。
6. 拒绝空 target segment 以及 dot segment `.`、`..`，而不是对其做路径归并。
7. URI 仅标识 authority、没有 target reference 时，省略尾部 `/`。

IPv6 literal **SHOULD** 使用 RFC 5952 的规范文本形式。

两个 Actr URI 的规范化序列化结果完全相同时，二者等价。target reference 仍区分大小写；
实现比较时 **MUST NOT** 将其转为小写。

## 6. 示例

合规示例：

| URI | 本层含义 |
| --- | --- |
| `actr://example.com` | Actr authority `example.com`。 |
| `actr://example.com/echo` | 解析到该 authority 发布的 Echo 入口 Actor Package。 |
| `actr://example.com/actors/echo` | 语义仍由 authority 决定的不透明、多 segment Package target。 |
| `actr://example.com:8443/echo` | authority discovery 使用 HTTPS 8443 端口。 |
| `actr://[2001:db8::1]:8443/echo` | 以 IPv6 literal 表示的 authority。 |

不合规示例：

| URI | 原因 |
| --- | --- |
| `actr://user@example.com/echo` | 禁止 user information。 |
| `actr://example.com/echo?token=secret` | query 未定义，也不得携带 credential。 |
| `actr://example.com/echo#method` | fragment 未定义。 |
| `actr://example.com/a//b` | 存在空 target segment。 |
| `actr://example.com/a/../b` | 存在 dot segment。 |
| `actr://101:acme+echo@1.0.0` | 旧语法，不是符合 RFC 3986 的 Actr authority。 |

## 7. 与 Actor Package 和 Actor instance 的关系

Actr runtime 以第 7.2 节的 ActorId 标识 Actor。ActorId 不是 Actr URI，也从不放入 URI
authority。

非空 target reference 只解析成一个入口 Actor Package 的发布引用。该引用至少需要最终绑定：

- Actor type；
- 精确版本；
- Package publisher；
- 不可变 Package digest；
- `.actr` artifact 的取得位置。

target reference 本身可以保持长期稳定，而其解析结果可以按 authority 的发布策略升级到
新的版本。一次具体解析得到的 Package 发布引用必须是不可变的，以便客户端验证、缓存和
重现安装结果。

Actr Host 使用 Package、部署配置和新分配的身份创建 Actor instance。该实例需要联系其他 Actor
时，使用 manifest 中的依赖、contract 和运行时发现协议；不会把依赖实例当成新的浏览器
导航 Target。例如，用户打开 `actr://example.com/echo` 后，Actrium 可以取得并启动带
`keel.guest/1` Workload 的 Echo UI；Echo UI 随后发现 Echo Server Actor，用户不需要也不应
在地址栏打开 `actr://example.com/echo/server`。

### 7.1 Realm 名

Realm 是一个应用实例的信任边界，其名字是某个 Realm 域名下的 Realm label：

```abnf
realm-name      = label "@" domain
domain          = dns-name / installation-id ".actr.alt"
label           = let-dig [ *61( let-dig / "-" ) let-dig ]
let-dig         = %x61-7A / DIGIT              ; 小写 a-z 与 0-9
installation-id = 26( %x61-7A / %x32-37 )      ; 小写 base32，a-z 与 2-7
```

- `dns-name` 按第 5 节规范化：小写 IDNA A-label，不含端口、userinfo 或 path。它与不含
  端口的 Actr authority `actr://<dns-name>` 同名，其 Realm 签名 key 由
  [Actr Authority Discovery](authority-discovery.zh.md) 公布。带显式端口或以 IP literal
  为 host 的 authority 不能作为 Realm 名中的域名。
- `<installation-id>.actr.alt` 是某个 Actr Host 安装的伪域名，位于 RFC 9476 `.alt` 后缀
  之下，永不经 DNS 解析。`installation-id` 是安装时生成的 128 位随机数，按小写 base32
  编码。这种本机 Realm 名只在该安装上可用（[Actr Realm Profile](realm-profile.zh.md)
  第 2.2 节）。
- `label` 使用 DNS label 规则：1 到 63 个字符，由小写 ASCII 字母、数字和 `-` 组成，不以
  `-` 开头或结尾。它在所属域名内唯一，创建后不可修改，Realm 退役后永不复用。Actr Host 默认
  生成随机 Realm label（第 9 节）；可读 label 需显式选择。

示例：

```text
k7m2q9xh@acme.com                                ; 默认随机 label
notes@acme.com                                   ; 显式选择的可读 label
h3x8p2wq@<installation-id>.actr.alt              ; 本机 Realm
```

Realm 名恰好含一个 `@`：左边是 label，右边是域名。Realm 名在规范化后按字符串精确比较。
解析 Realm 名是纯语法操作，没有网络副作用；把 DNS 域名下的 Realm 名解析为经过验证的
Realm Profile 由 Actr Realm Profile 定义。

Realm 名形似电子邮件地址，但不是邮件地址。Actr Host **MUST NOT** 把它渲染为 `mailto:` 链接或
当作邮件地址处理，应显示为实例地址。它从不出现在 `actr://` 的 authority 中（那里禁止
userinfo，见第 3 节）。Realm 名或 ActorId 放进 URI 时是数据：承载方 **MUST** 把 `@`
percent-encode 为 `%40`（如 `?service=2af01.k7m2q9xh%40acme.com`），解析方使用前先解码。

### 7.2 ActorId

持久 Actor 身份（ActorId）的文本形式为：

```abnf
actor-id        = serial "." realm-name
serial          = 1*( DIGIT / %x61-66 )        ; 小写十六进制
```

例如 `2af01.k7m2q9xh@acme.com`。`serial` 是 Actor serial：由该 Realm 的 Issuer 签发的小写十六进制序列号。
唯一的 `@` 左边的局部部分像 DNS 名一样从具体到一般：先是 Actor，再是其 Realm label。
解析时先在 `@` 处拆分，再在局部部分的第一个 `.` 处拆分；label 不含 `.`，因此拆分无歧义。
去掉 `<serial>.` 即得 Realm 名。
ActorId 只回答「哪个 Realm 里的哪个 Actor」，不包含程序身份（`publisher/program@version`），
程序身份由 Actor 注册与 Actor credential 携带。Realm Provider 内部的数值 realm 标识
**MUST NOT** 出现在 ActorId 中。ActorId 不是 Actr URI。

## 8. 旧 URI 迁移

现有 Actr prototype 曾接受以下历史格式：

```text
actr://<realm>:<publisher>+<name>@<version>
```

例如：

```text
actr://101:acme+echo-service@1.0.0
```

这个格式把应用字段放到了 RFC 3986 预期的网络 authority 位置。由于 `@` 用于分隔 user
information 和 host，通用 URI parser 会把 `101:acme+echo-service` 解释为 user
information，把 `1.0.0` 解释为 host。因此它不符合本规范。

实现 **MAY** 仅在显式的 legacy migration 模式中识别该历史格式，但 **MUST NOT**
悄悄把它当作合规的新 Actr URI。迁移工具必须通过配置的旧 authority 数据解析旧 realm
和 Actor type，再输出新的 authority-based URI。

## 9. 安全与隐私考虑

- Actr URI 很可能出现在日志、浏览器历史、崩溃报告、遥测、剪贴板以及应用间消息中，
  因此不得包含 credential 或秘密授权材料。
- 客户端必须先验证 HTTPS 和 authority 绑定，再信任发现到的服务端点。
- parser 必须拒绝无效 percent-encoding、控制字符、畸形 authority、被禁止的组成部分、
  空 path segment 和 dot segment。
- 客户端必须对发现到的端点执行 scheme 和网络策略，以防止 downgrade 和 SSRF 攻击。
- 显示软件应在视觉上突出 authority，并考虑国际化域名的同形异义风险。
- 授权发生在 Package 解析、取得或安装时；仅持有 Actr URI 并不授予访问权。
- Realm 名与 ActorId 在分享时会暴露 Realm 域名与 Realm label。Actr Host 创建 Realm 时
  **SHOULD** 默认生成随机 Realm label，把人类可读的名称放在 Realm Profile 的 `display_name` 中。

## 10. IANA 考虑

计划申请的 provisional 登记为：

```text
Scheme name: actr
Status: Provisional
Applications/protocols that use this scheme name: Actr
Contact: Actr Protocol Maintainers
Change controller: Actrium Project
References: this document
```

`actr` scheme 通过 Actr Authority Discovery 定义的 HTTPS Well-Known URI 支持 authority
discovery。

## 11. 参考资料

- [RFC 3986：Uniform Resource Identifier: Generic Syntax](https://www.rfc-editor.org/rfc/rfc3986.html)
- [RFC 5952：A Recommendation for IPv6 Address Text Representation](https://www.rfc-editor.org/rfc/rfc5952.html)
- [RFC 7595：Guidelines and Registration Procedures for URI Schemes](https://www.rfc-editor.org/rfc/rfc7595.html)
- [RFC 8615：Well-Known Uniform Resource Identifiers](https://www.rfc-editor.org/rfc/rfc8615.html)
- [RFC 9476：The .alt Special-Use Top-Level Domain](https://www.rfc-editor.org/rfc/rfc9476.html)

## 12. 待完成工作

本初始草案有意保留以下工作：

- 完成 Package Resolution 的 HTTP 请求、响应和错误格式；
- 规定私有 Package target 的授权质询；
- 冻结 ActorId 的二进制编码；
- 增加 parser、规范化、等价性及 legacy migration 测试向量；
- 确立 IANA 登记使用的长期公开规范地址和项目联系人。
