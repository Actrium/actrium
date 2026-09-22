# Actr Package Resolution

语言：[English](package-resolution.md) | **简体中文**

状态：`actr-packages/1` 初始草案。

本文定义 `actr:` URI 中的不透明 Target 如何解析为不可变的入口 `.actr` Package，并定义
Actr Authority Discovery 所发布 Package service 的 HTTP wire format。

## 1. 角色与边界

非空 Target 标识由 authority 选择的**入口 Actor Package**。它不标识正在运行的 Actor
instance、signaling endpoint 或 session。

```text
actr://authority/opaque-target
        → GET /.well-known/actr
        → 选择 rel=packages、protocol=actr-packages/1
        → 解析完整 Target
        → 下载不可变的 .actr artifact
        → 验证并在兼容 Host 中启动
```

运行时 Actor discovery 发生在入口 Package 启动之后。即使同一套 Loran 部署同时实现二者，
Package service 和 runtime discovery service 也是两个独立协议角色。

## 2. 发现 Package service

authority descriptor 使用 service link 发布 Package service：

```json
{
  "rel": "packages",
  "href": "https://packages.example.com/actr/packages",
  "protocol": "actr-packages/1"
}
```

`href` 是 service endpoint，不是 Target 的 URL prefix。客户端必须使用下节定义的请求，不能
把 Target path segment 直接追加到 `href`。

## 3. Target 解析请求

`actr-packages/1` service 必须支持：

```http
GET /actr/packages/resolve?target=apps%2Fecho HTTP/1.1
Host: packages.example.com
Accept: application/json
```

`target` 必须且只能出现一次。它的值是去掉前导 `/` 的完整规范化 Target，并按 HTTP query
value 编码。服务端将其视为不透明值，不得假设其中 segment 表示 publisher、version、
realm、文件路径或 Actor identity。

解析操作安全且幂等，服务端应提供明确的 HTTP 缓存指令。公开 Target 可以匿名解析；私有
Target 可以使用常规 HTTP 认证与授权，但秘密信息不能进入 `actr:` URI。

## 4. 成功响应

成功响应使用 `200 OK` 和 `application/json`，body 遵循
[`package-resolution.schema.json`](../../contracts/actr/package-resolution.schema.json)：

```json
{
  "version": 1,
  "target": "apps/echo",
  "package": {
    "href": "https://packages.example.com/actr/packages/artifact?target=apps%2Fecho&sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "media_type": "application/zip",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "size": 18432,
    "profile": "keel.guest/1"
  }
}
```

- `version` 是响应格式版本，本草案固定为 `1`；
- `target` 必须与请求中解码后的完整 Target 完全一致；
- `href` 是本次解析选中不可变 artifact 的绝对地址；
- `media_type` 为 `application/zip`，因为 `actr-package/2` 是 ZIP container；
- `sha256` 覆盖 artifact 下载响应的全部字节；
- `size` 是预期 artifact 字节数；
- `profile` 是可选提示，可用于提前检查 Actr Host 兼容性，但签名后的 Package manifest 才是
  权威信息。

客户端必须忽略未知字段，并拒绝不支持的版本、Target 不匹配、畸形 URL、无效 digest 或
无效 size。

## 5. Artifact 下载

客户端对 `package.href` 发起 `GET`。成功响应使用 `200 OK` 和 `application/zip`。

客户端必须限制下载大小，验证 `size` 和 `sha256`，然后才能把字节交给 Actor Package
verifier。传输完整性不能代替 manifest signature、publisher trust、Workload Profile、
Actr Host API 和 capability policy 验证。

Artifact URL 应采用内容寻址，因此可以使用长期 immutable cache。Loran 把选中的 digest
写入下载 URL；如果当前配置的文件已不再匹配该 digest，下载请求会被拒绝。

## 6. 错误

- `400 Bad Request`：Target 缺失、重复或格式错误；
- `401 Unauthorized` / `403 Forbidden`：需要认证或授权被拒绝；
- `404 Not Found`：authority 未发布该 Target，或不可变 artifact 引用不存在；
- `406 Not Acceptable`：没有客户端支持的响应表示；
- `503 Service Unavailable`：已配置的 artifact storage 暂不可用。

错误 body 不是成功的 Package Resolution document。

## 7. Actr Host 行为

Target 成功解析不代表当前 Actr Host 一定兼容。例如 Actrium 可以接受 `keel.guest/1`，而
Wasmtime Host 接受 `actr.component/1`。不兼容的 Actr Host 应报告 unsupported profile，不能把
服务器 workload 当作可浏览页面。

Echo 的预期顺序是：

```text
打开 actr://example.com/echo
  → 解析并启动 echo-ui.actr
  → 读取它声明的 Echo dependency 与 contract
  → runtime discovery 选择运行中的 Echo Server Actor
  → signaling 建立 Actor 连接
```

用户不需要访问 `actr://example.com/echo/server`。

## 8. Loran 组合方式

Loran 的 `loran-packages` 组件负责 Target resolution 和 artifact download。即使本机 Loran
不提供 `/.well-known/actr`，该组件也能独立运行。默认 Loran daemon 可以同时挂载二者并通过
`rel=packages` 发布，也可以把 Package service 指向另一 Loran 节点、对象存储 gateway 或
CDN-backed implementation。

Publisher Registry 继续负责 publisher identity、key、signing authority 和 publication audit。即使二者可
部署到同一进程，Publisher Registry 也不会因此成为 artifact store。

## 9. 安全考虑

- 生产解析与下载必须使用 HTTPS；开发实现只能对明确的 loopback authority 允许 HTTP；
- resolver 不得把不可信 Target 直接变换为本地文件路径；Loran 只解析管理员配置的精确
  Target mapping；
- 客户端必须在跟随 artifact URL 前应用 redirect、response size、timeout 和 network address
  policy；
- 私有 Target 的 cache key 必须包含授权状态；
- `actr:` URI 和公开解析响应都不携带 bearer secret。

## 10. 相关规范

- [Actr URI Scheme](uri.zh.md)
- [Actr Authority Discovery](authority-discovery.zh.md)
- [Actr Actor Package 规范](actor-package.zh.md)
