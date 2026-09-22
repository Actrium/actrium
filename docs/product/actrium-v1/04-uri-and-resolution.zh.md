# 04. `actr://` URI 与解析规范

## 1. 设计目标

`actr://` 必须同时做到：

- 人能看出 Publisher、Program 与 Version；
- 机器能精确区分 Create 和 Attach；
- DNS 命名空间下 Realm 的实例 URI 可以成为地址栏、历史、收藏和跨设备分享链接；本机 Realm 的 URI 明确为安装内地址；
- URI 不承载秘密；
- 已指定 ActorId 时不引入服务发现或 LB；
- 规范化后同一含义只有一种稳定表示。

## 2. 基础语法

```text
actr://<publisher>/<program>@<version>[/<guest-route>][?<query>][#<fragment>]
```

示例：

```text
actr://notes.example/team-notes@1.4.2
actr://notes.example/team-notes@1.4.2/today
```

简化 ABNF：

```abnf
actr-uri       = "actr://" publisher "/" program "@" version
                 [ "/" guest-route ] [ "?" query ] [ "#" fragment ]
publisher      = reg-name
program        = ALPHA *( ALPHA / DIGIT / "-" / "_" / "." )
version        = semver
guest-route    = *pchar *( "/" *pchar )
```

正式注册 scheme 时应遵循 URI scheme 注册规范，并以 RFC 3986 的 authority、path、query、fragment 解析规则为基线。

## 3. Publisher

### 3.1 身份规则

正式 `actr://` URI 中的 Publisher：

- 必须是 DNS 域名；
- 转为 IDNA A-label 后小写规范化；
- 不允许 userinfo；
- 不允许 IP literal；
- 不包含端口；
- 不自动添加或删除 `www`；
- 最大长度遵循 DNS 名称限制；
- Unicode 显示必须提供同形字风险提示。

```text
Example.COM       → example.com
bücher.example    → xn--bcher-kva.example
example.com       ≠ www.example.com
```

### 3.2 为什么 Publisher 不含端口

端口不会破坏 TLS 对主机名的证书验证，但会改变产品身份语义：

```text
example.com:8443 ≠ example.com:9443
```

把端口纳入 Publisher 会造成：

- 同一组织被拆成大量发布者身份；
- 市场去重、权限记忆和域名验证更复杂；
- 用户难以识别；
- 链接更容易被代理和防火墙阻断；
- loopback、私网和 SSRF 风险混入正式命名空间。

因此：

- Publisher ID 只使用域名；
- Publisher 与 Realm 命名空间的域名 bootstrap 固定走 HTTPS 443；DNS Realm 命名空间即 Actr authority，其 bootstrap 就是 `https://<domain>/.well-known/actr` 这份 authority descriptor；
- Realm Profile 从 authority descriptor 中 core relation `realms`（已在 Actr Authority Discovery §4.3 定义）声明的 HTTPS 位置获取，可以使用显式端口；
- authority descriptor `services` 中的默认 provider 只作为创建新 Realm 时的建议；已有 Realm 一律以其 Profile 的委派为准；
- Profile 验证后委派的 package resolver、CDN、issuer、WSS、STUN/TURN endpoint 可以使用显式端口；
- 非标准端口是传输配置，不是 Publisher 名称的一部分。

## 4. Program 与 Version

### 4.1 Program

- 在单个 Publisher 下唯一；
- 大小写敏感性在发布时禁止歧义，规范形式使用小写；
- 长度建议不超过 64 字节；
- 禁止路径分隔符、百分号和控制字符。

### 4.2 Version

- 正式 URI 必须使用精确 SemVer；
- 不允许 `latest`、`stable`、范围表达式或缺省版本；
- prerelease 允许，但必须完整写出；
- build metadata 是否进入身份必须在 ActrType 规范中统一；v1 建议禁止，以免等价判断分叉。

合法：

```text
actr://example.com/editor@1.2.3
actr://example.com/editor@2.0.0-beta.4
```

非法：

```text
actr://example.com/editor
actr://example.com/editor@latest
actr://example.com/editor@^1.2
```

Program Catalog 可以提供「使用推荐版本」按钮，但必须先解析为精确版本，再写入地址栏。

## 5. Create URI

没有 `service` 查询参数的 URI 表示 Create Intent：

```text
actr://notes.example/team-notes@1.4.2
```

语义：

- 下载/读取精确 Program Release；
- 选择 Realm 命名空间，默认本机 `local` 命名空间；
- 在该命名空间下新建一个 Realm（`local_name` 由 Actro 生成、用户可改），不会放进已有 Realm；
- 在新 Realm 内创建一个新的 Service；
- 选择或创建 Workspace；
- 成功后将地址原位替换为 Attach URI。

Create URI 可用 `namespace` 参数指定 DNS 命名空间：

```text
actr://notes.example/team-notes@1.4.2?namespace=team.example
```

- 值为规范化 DNS 域名，规则与 Publisher 相同：IDNA A-label 小写、无端口、userinfo 或 IP literal；
- 省略时使用本机 `local:<installation-id>` 命名空间；URI 中不能写 `local` 命名空间；
- 该命名空间必须已受信或在确认页完成首次信任；使用 Provider 自有命名空间时，确认页必须如实显示信任根是该 Provider；
- `local_name` 不进入 URI，在确认页决定。

Create URI 是一个操作意图，不是某个已存在资源的永久地址。收藏此 URI 等价于「新建模板」。

## 6. Attach URI

带 `service` 的 URI 表示 Attach Intent：

```text
actr://notes.example/team-notes@1.4.2/today
  ?service=2af01%40notes~dns%3Ateam.example%2Fnotes.example%3Ateam-notes%3A1.4.2
```

排成单行：

```text
actr://notes.example/team-notes@1.4.2/today?service=2af01%40notes~dns%3Ateam.example%2Fnotes.example%3Ateam-notes%3A1.4.2
```

### 6.1 `service`

- 值为 Actr 规范 `ActorId` 字符串的 UTF-8 percent-encoding；
- 必须完整包含 Realm 引用（命名空间 + `local_name`）、serial 与 ActrType；不含数值 `realm_id`；
- 解析后 ActrType 必须与 URI 的 publisher/program/version 完全相同；
- 不允许同一个 query 出现多个 `service`；
- 不允许空值；
- 不允许使用 Actor 类型、别名、市场 ID 或本地数据库 ID 替代。

### 6.2 `realm`

v1 正式 ActorId 本身必须携带 Realm 引用，因此 Attach URI 不需要第二个定位参数。Actro 从 `service` 解出 Realm：

```text
2af01@notes~dns:team.example/notes.example:team-notes:1.4.2
```

`realm` 只用于一种情况：导入当前 Actr 的 legacy ActorId（只含数值 `realm_id`）时，显式补足完整 Realm 引用，例如 `?realm=notes~dns:team.example`（percent-encode 后写入）。Create URI 不再使用 `realm`，改用 `namespace`；Create URI 出现 `realm` 时拒绝。

Legacy Attach 解析成功后必须升级为含 Realm 引用的 v1 ActorId，并从规范化 URI 移除 `realm`。不能让 `realm` 与 `service` 长期构成两个互相可能矛盾的定位源。

本机 Realm 的 ActorId 使用 `local:<installation-id>` 命名空间，例如 `2af01@h3x8p2wq~local:<installation-id>/...`。它能在本机精确重连，但不能被其他设备解析；Actro 必须禁用「复制 portable link」，而不是伪造 DNS 命名空间。

### 6.3 Attach 的硬规则

Attach 失败时：

- 不得按类型寻找替代；
- 不得创建本地实例；
- 不得自动降级版本；
- 不得换 Realm；
- 不得忽略 ActorType 或 contract mismatch。

## 7. Guest route、query 与 fragment

### 7.1 Path

Program/version 后的 path 是 Guest 前台路由：

```text
/today/note/123
```

它不参与 Service 选择，也不能改变 Workspace。

### 7.2 Reserved query

v1 保留：

- `service`
- `namespace`（仅 Create）
- `realm`（仅 legacy Attach 导入）

其他 query 字段在去除保留字段后作为只读 launch parameters 提供给 Guest。Program 不得把业务字段命名为保留字段。

### 7.3 Fragment

Fragment 只用于 Guest 页面内状态，不参与：

- Program 解析；
- Service 定位；
- Realm 选择；
- 权限；
- 包版本；
- Actor 身份验证。

Actro 不允许在 fragment 中传 bearer invite。邀请和授权通过独立的 Realm/Service 流程完成。

## 8. 规范化

Actro 保存历史、收藏、复制链接前必须：

1. scheme 小写；
2. Publisher 与 Realm 命名空间域名（`namespace` 参数、ActorId 中的 `dns:` 部分）转为小写 IDNA A-label；
3. 移除默认空 path；
4. Program 使用发布记录中的规范大小写；
5. Version 使用标准 SemVer 字符串；
6. ActorId 先 parse，再用 Actr 标准 formatter 输出并 percent-encode；
7. query 先放 `service`；只有 Create URI 允许 `namespace`，只有 legacy Attach URI 允许 `realm`（导入后移除），其余字段按 key/value 稳定排序；
8. 不改变 Guest path 中有意义的大小写；
9. 拒绝重复保留参数；
10. 拒绝解码后的控制字符、NUL 和非法 UTF-8。

## 9. 解析算法

```text
parse(uri):
  1. 验证 scheme
  2. 验证 authority 只有 publisher host
  3. 规范化 publisher
  4. 解析 program@exact-version
  5. 解析 guest route
  6. 提取并去重 reserved query
  7. 如果 service 缺失：
       拒绝 realm；规范化可选 namespace（缺省为本机命名空间）
       → CreateIntent（新 Realm 在确认后创建）
  8. 如果 service 存在：
       拒绝 namespace
       parse ActorId，取出 Realm 引用 (namespace, local_name)
       assert ActorId.type == coordinate
       DNS 命名空间：
         获取 https://<domain>/.well-known/actr（authority descriptor）
         用已固定的命名空间 key 验证 realm_namespace 成员的整体签名
           与轮换链，拒绝 sequence 回退；已接受 Profile 的签名 key
           若已从 keys 移除，丢弃该 Profile 及其 sequence 基线，改用最后一份
           由仍列出的 key 签名的已接受 Profile；没有则按从未接受重新获取
         经 realms relation 获取 Realm Profile，用命名空间 key 验签
         assert profile.namespace/local_name == ActorId 的 Realm 引用
         assert profile.sequence 未回退（否则 URI_REALM_PROFILE_INVALID）
         assert provider.id 与已接受 Profile 一致（否则 REALM_PROVIDER_MISMATCH）
         assert profile.status != retired
         任一步失败 → fail closed：拒绝 Attach，
           MUST NOT 回退到 authority descriptor services 中的默认 provider
       本机命名空间（无 Profile）：
         assert installation-id == this installation 且本地 record 存在
       → AttachIntent
  9. fragment 作为 Guest 状态保存
```

第 1–7 步与第 8 步的语法部分只产生意图对象，不下载包、不创建 Service；第 8 步的 authority descriptor 与 Profile 获取属于解析 Realm 的网络步骤，同样必须在用户确认和策略检查之后执行，结果可缓存。

`realm_namespace` 成员与 `realms` relation 已在 Actr Authority Discovery §4.3–4.4 定义。协议对象字段一律 snake_case（如 `realm_namespace`、`local_name`）。

Realm Profile 取不到、验不过或已退役时，Actro 必须 fail closed：不连接该 Realm 的任何 Actor，也不得改用 authority descriptor `services` 中的默认 provider 与该 Realm 通信；默认 provider 只用于创建新 Realm 时的建议。

## 10. 创建后的 URI 改写

创建事务成功返回 ActorId 后：

```text
replaceState(
  actr://publisher/program@version/path
  ?service=<new-actor-id>
)
```

必须使用 replace，而不是 push：

- Back 不应回到会再次创建的确认页；
- Refresh 必须连接新 Service；
- History 只记录最终实例地址。

如果 Service 已创建但 URI 改写失败，Actro 必须从 `creation_intent_id` 恢复结果，不能再创建一次。

## 11. 开发 URI

开发来源使用独立 scheme：

```text
actr-dev://localhost:7443/team-notes@0.0.0-dev
```

规则：

- 仅在显式开发模式注册；
- authority 是 HTTPS Origin，可以含端口；
- 允许 localhost、loopback 和显式信任的私有 CA；
- 不写入 verified Program Catalog；
- 安全 UI 始终显示「开发来源」；
- 导出或分享前给出风险提示；
- `actr-dev://` 不得被正式 Marketplace 接受。

正式 `actr://` 不支持 HTTP fallback。

## 12. 外部唤起

来自网页、二维码、邮件或其他 App 的 URI：

- 必须先展示解析后的 Publisher、Program、Version 和 Create/Attach 意图；
- Create Intent 必须确认；
- Attach 可以按用户设置直接打开，但首次信任 Realm 命名空间或 Publisher 仍需确认；
- 同一来源短时间创建多个实例时触发限流；
- 外部 URI 不得携带权限预授权；
- 未知 query 只在 Guest 启动后可见，不影响安装或权限页面。

## 13. URI 错误码

| 代码 | 含义 |
|---|---|
| `URI_SCHEME_INVALID` | 非受支持 scheme |
| `URI_PUBLISHER_INVALID` | Publisher 不是合法正式域名 |
| `URI_PORT_NOT_ALLOWED` | 正式 Publisher 带端口 |
| `URI_PROGRAM_INVALID` | Program 语法非法 |
| `URI_VERSION_REQUIRED` | 缺少精确版本 |
| `URI_VERSION_INVALID` | 版本不是允许的 SemVer |
| `URI_RESERVED_DUPLICATE` | 重复保留参数 |
| `URI_ACTOR_ID_INVALID` | Service ActorId 无法解析 |
| `URI_ACTOR_TYPE_MISMATCH` | ActorId type 与 Program coordinate 不符 |
| `URI_NAMESPACE_INVALID` | `namespace` 不是合法规范化域名（含端口、IP literal 等） |
| `URI_NAMESPACE_NOT_ALLOWED` | Attach URI 出现 `namespace`，或 Create URI 出现 `realm` |
| `URI_REALM_REQUIRED` | Legacy ActorId 缺 Realm 引用，无法导入 |
| `URI_REALM_MISMATCH` | Legacy `realm`、Realm Profile 与 ActorId 的 Realm 引用不符 |
| `URI_REALM_NAMESPACE_INVALID` | authority descriptor 缺少 `realm_namespace`，或其签名、轮换链无效，或 `sequence` 回退 |
| `URI_REALM_PROFILE_UNAVAILABLE` | 无法经 `realms` relation 取得 Realm Profile；fail closed，不回退默认 provider |
| `URI_REALM_PROFILE_INVALID` | Realm Profile 签名无效（含签名 key 已被移除）、`sequence` 回滚，或与 ActorId 的 Realm 引用不符；fail closed |
| `REALM_PROVIDER_MISMATCH` | 新 Profile 的 `provider.id` 与已接受 Profile 不符（Realm 与 Provider 永久绑定，见 12 错误码）；fail closed |
| `URI_REALM_RETIRED` | ActorId 所属 Realm 的 Profile 已退役 |
| `URI_LOCAL_REALM_FOREIGN` | 本机 Realm ActorId 不属于本安装 |

错误码供日志、测试和开发者工具使用；用户文案必须说明可操作原因。
