# 04. `actr://` URI 与解析规范

## 1. 设计目标

`actr://` 必须同时做到：

- 人能看出 Publisher、Program 与 Version；
- 机器能精确区分 Create 和 Attach；
- DNS Realm 的 URI 可以成为地址栏、历史、收藏和跨设备分享链接；Local Realm URI 明确为安装内地址；
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
- Publisher/Realm 的域名 bootstrap 固定走 HTTPS 443；
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

Program Catalog 可以提供“使用推荐版本”按钮，但必须先解析为精确版本，再写入地址栏。

## 5. Create URI

没有 `service` 查询参数的 URI 表示 Create Intent：

```text
actr://notes.example/team-notes@1.4.2
```

语义：

- 下载/读取精确 Program Release；
- 创建一个新的本地 Service；
- 选择或创建 Workspace；
- 选择 Realm，默认本地 Realm；
- 成功后将地址原位替换为 Attach URI。

Create URI 是一个操作意图，不是某个已存在资源的永久地址。收藏此 URI 等价于“新建模板”。

## 6. Attach URI

带 `service` 的 URI 表示 Attach Intent：

```text
actr://notes.example/team-notes@1.4.2/today
  ?service=2af01%4042~dns%3Ateam.example%2Fnotes.example%3Ateam-notes%3A1.4.2
```

排成单行：

```text
actr://notes.example/team-notes@1.4.2/today?service=2af01%4042~dns%3Ateam.example%2Fnotes.example%3Ateam-notes%3A1.4.2
```

### 6.1 `service`

- 值为 Actr 规范 `ActorId` 字符串的 UTF-8 percent-encoding；
- 必须完整包含 Realm authority、Realm ID、serial 与 ActrType；
- 解析后 ActrType 必须与 URI 的 publisher/program/version 完全相同；
- 不允许同一个 query 出现多个 `service`；
- 不允许空值；
- 不允许使用 Actor 类型、别名、市场 ID 或本地数据库 ID 替代。

### 6.2 `realm`

v1 正式 ActorId 本身必须携带 Realm authority，因此 Attach URI 不需要第二个定位参数。Actro 从 `service` 解出 Realm：

```text
2af01@42~dns:team.example/notes.example:team-notes:1.4.2
```

`realm` 只用于两种情况：

- Create URI 显式选择联网 Realm，例如 `?realm=team.example`；省略时创建到本地 Realm；
- 导入当前 Actr 的 legacy ActorId 时补足缺失 authority。

Legacy Attach 解析成功后必须升级为含 authority 的 v1 ActorId，并从规范化 URI 移除 `realm`。不能让 `realm` 与 `service` 长期构成两个互相可能矛盾的定位源。

Local Realm ActorId 使用 `local:<installation-id>` authority。它能在本机精确重连，但不能被其他设备解析；Actro 必须禁用“复制 portable link”，而不是伪造 DNS Realm。

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
- `realm`

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
2. Publisher 和 Realm 转为小写 IDNA A-label；
3. 移除默认空 path；
4. Program 使用发布记录中的规范大小写；
5. Version 使用标准 SemVer 字符串；
6. ActorId 先 parse，再用 Actr 标准 formatter 输出并 percent-encode；
7. query 先放 `service`；Create/legacy URI 才允许 `realm`，其余字段按 key/value 稳定排序；
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
  7. 如果 service 缺失 → CreateIntent
  8. 如果 service 存在：
       parse ActorId
       assert ActorId.type == coordinate
       resolve ActorId.realm_authority
       assert ActorId.realm_id == profile.realm_id（DNS Realm）
       或 assert ActorId.local_installation == this installation（Local Realm）
       → AttachIntent
  9. fragment 作为 Guest 状态保存
```

解析只产生意图对象，不下载包、不创建 Service、不连接网络。副作用必须在用户确认和策略检查之后执行。

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
- 安全 UI 始终显示“开发来源”；
- 导出或分享前给出风险提示；
- `actr-dev://` 不得被正式 Marketplace 接受。

正式 `actr://` 不支持 HTTP fallback。

## 12. 外部唤起

来自网页、二维码、邮件或其他 App 的 URI：

- 必须先展示解析后的 Publisher、Program、Version 和 Create/Attach 意图；
- Create Intent 必须确认；
- Attach 可以按用户设置直接打开，但首次 Realm/Publisher 信任仍需确认；
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
| `URI_REALM_REQUIRED` | Legacy ActorId 缺 authority，无法导入 |
| `URI_REALM_MISMATCH` | Legacy `realm`、Realm Profile 与 ActorId 的 realm 不符 |
| `URI_LOCAL_REALM_FOREIGN` | Local Realm ActorId 不属于本安装 |

错误码供日志、测试和开发者工具使用；用户文案必须说明可操作原因。
