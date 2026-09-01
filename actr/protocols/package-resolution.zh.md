# Actr Package Resolution

状态：架构草案；HTTP wire format 尚未定义。

本文仅提供简体中文版。

本文定义 `actr:` URI 的 Target、authority 的 Package service、不可变 Package 发布引用和
`.actr` artifact 之间的职责边界。它先固定长期语义，具体 HTTP method、request/response
schema、认证质询和错误编码由后续版本补全。

## 1. 核心结论

非空 `actr:` URI Target 标识一个由 authority 发布的**入口 Actor Package**，供 Host 安装
并尝试启动。它不标识已经运行的 Actor instance、signaling endpoint 或当前会话。

标准解析链为：

```text
actr:// authority + opaque Target
              │
              ▼
/.well-known/actr
              │  选择 rel=packages
              ▼
Package service
              │  解析 Target
              ▼
不可变 Package 发布引用
              │  取得 artifact
              ▼
根 .actr
              │  验证并检查 Host/Profile
              ▼
安装并启动入口 Actor
```

Target **MUST NOT** 先通过运行时 `rel=discovery` 解析。`discovery` 用于 Actor 启动后按依赖、
contract 和 capability 发现 Actor instance；它不是浏览器地址栏的 Package resolver。

## 2. Target

Target 是 `actr://<authority>/` 后经过 URI 规范化的不透明 path，例如：

```text
actr://example.com/echo
actr://example.com/apps/echo
```

通用客户端不得从 segment 位置猜测 manufacturer、Actor name、version、realm 或仓库目录。
这些名称如何映射到发布物，由控制该 authority 的 Package service 决定。

Target 可以作为长期入口保持稳定；例如 `echo` 可以随发布策略指向当前稳定版本。但是一次
解析的结果必须落到不可变的 Package 发布引用，不能在下载或验证过程中悄悄漂移。

以下内容不是 URI Target：

- Actor instance identity 或 serial number；
- signaling、STUN、TURN 或 WebRTC 地址；
- 当前 session；
- manifest 中依赖解析得到的运行中 Actor；
- Package repository 内部文件路径。

authority 可以选择不为某个服务器 Actor 发布公开 Target。该 Actor 的 `.actr` 仍可作为部署
发布物或其他 Package 的锁定依赖存在；其运行实例由 runtime discovery 发现。

## 3. Package service discovery

客户端先取得：

```text
https://<authority>/.well-known/actr
```

再从 `services` 数组选择 `rel` 为 `packages` 的 endpoint。descriptor 中没有一个名为
`packages` 的顶层字段；`packages` 是 service relation。

示意：

```json
{
  "version": 1,
  "authority": "actr://example.com",
  "services": [
    {
      "rel": "packages",
      "href": "https://packages.example.com/actr",
      "protocol": "actr-packages/1"
    }
  ]
}
```

`href` 标识 Package service endpoint，不是 Target 的 URL prefix。客户端 **MUST NOT** 仅把
`echo` 拼成 `https://packages.example.com/actr/echo`。它必须按照 `protocol` 指定的 Package
Resolution 请求格式提交完整 Target。

## 4. Package 发布引用

Package service 成功解析 Target 后，应返回足以唯一选择并验证 artifact 的不可变引用。其
逻辑信息至少包括：

- Actor type；
- 精确版本；
- Package publisher；
- Package digest 及算法；
- artifact locator；
- artifact media type 或 format。

以下 JSON 仅展示逻辑形状，不是已经确定的 wire schema：

```json
{
  "actor": "acme:echo",
  "version": "1.2.0",
  "publisher": "acme",
  "digest": "sha256:0123456789abcdef...",
  "artifact": {
    "href": "https://cdn.example.com/actr/acme/echo/1.2.0/package.actr",
    "mediaType": "application/octet-stream"
  }
}
```

媒体类型尚未登记；在正式定义前，实现可以使用通用二进制媒体类型。Package digest 用于
选择、缓存和传输完整性，不能代替 `.actr` manifest signature 与 publisher trust 验证。

## 5. Host 行为

取得根 `.actr` 后，Host 必须按 Actor Package 规范验证容器、digest、manifest、签名、
Workload Profile、Host API 和 capability policy。

Target 可被 authority 正常解析，不代表每个 Host 都能运行其 Package。例如：

- Actrium 通常打开带 `keel.guest/1` Workload 和 `ui.render` capability 的入口 Package；
- 基于 Wasmtime 的 Host 可以运行 `actr.component/1`；
- Actrium 收到只支持 `actr.component/1` 的服务器 Package 时，应报告 unsupported-profile，
  而不是把它当作可浏览页面；
- 格式和签名有效但 capability policy 不允许时，Host 应明确拒绝启动。

authority 面向 Actrium 发布可导航入口时，应该让 Target 解析到 Actrium 能呈现的根 Package，
而不是暴露一个只能远程连接的 Server Actor instance。

## 6. 与运行时 Actor discovery 的边界

应用启动后的 Actor 连接不再经过浏览器导航 Target。例如 Echo：

```text
用户打开 actr://example.com/echo
  → Package service 解析 echo
  → 取得并启动 echo-ui.actr
  → Echo UI 根据 manifest dependency 与 contract 请求 echo service
  → runtime discovery 找到可用的 Echo Server Actor instance
  → signaling 建立连接
```

用户不需要打开 `actr://example.com/echo/server`。服务器实例的 identity、位置、授权和会话
状态不会编码进公开的应用 URI。

## 7. Loran 的职责

Loran 作为完整的默认 authority 实现，可以同时提供：

- `/.well-known/actr` authority descriptor；
- Package Target resolver；
- Package metadata、索引和 release 管理；
- `.actr` artifact 下载，或把 artifact 委派给对象存储/CDN；
- 运行时 Actor discovery、signaling 和 ICE configuration 等独立服务。

这些能力可以部署在同一进程，但协议角色必须保持分离。Package Target resolution 不得因为
Loran 的合并实现而被等同于运行时 Actor instance discovery。

## 8. 后续工作

- 定义 `actr-packages/1` 的 HTTP method、Target 编码和响应 schema；
- 定义 Package 发布引用的正式字段与 JSON Schema；
- 定义版本选择、channel、缓存和 redirect 规则；
- 定义私有 Target 的认证与授权质询；
- 定义 not-found、unsupported-profile、digest-mismatch 等错误映射；
- 决定是否申请专用 `.actr` media type；
- 增加 Target 解析、artifact 替换攻击和缓存一致性测试向量。

## 9. 相关文档

- [Actr URI Scheme](uri.zh.md)
- [Actr Authority Discovery](authority-discovery.zh.md)
- [Actr Actor Package 规范](actor-package.zh.md)
