# 开发 macOS Actrium Front

[English](keel-askit-guest-guide.md) · [Front API 参考](front-api-reference.zh.md)

Actrium Front 是用 JS 或 TS 写的 React 界面。Guest 在 Keel 沙箱里运行，Actrium 用 macOS Host 已注册的组件渲染界面。Guest 通过具名的 `host:*` 导入调用 Host 能力。在 Realm 中运行的 Front 还可以调用绑定的 ACTR Service 处理业务。本文适用于当前 Actrium v1 JS Front，产物为签名的 `keel.guest/1` Workload。

## 1. 编写 Guest

从 Actrium 仓库根目录开始。先安装 Bun 和仓库的 JS 依赖，确保 PATH 中的 `actr` CLI 支持 `pkg keygen`、`pkg pack` 和 `pkg verify`。在 macOS 上运行时，使用兼容的 Actrium 构建版本。

```sh
actrium_repo="$PWD"
front_dir="$(mktemp -d "${TMPDIR:-/tmp}/actrium-front.XXXXXX")"
cd "$front_dir"
mkdir -p workload dist
```

把下面的代码保存为 `guest.tsx`。Keel v1 用默认导出作为根组件，UI 组件从 `keel/guest` 导入。这个例子可以增加计数，也可以请 Host 把当前计数复制到剪贴板。

```tsx
import { useState } from 'react';
import { Button, Text, View } from 'keel/guest';
import { writeText } from 'host:clipboard';

export default function Counter() {
  const [count, setCount] = useState(0);
  const [message, setMessage] = useState('');

  async function copyCount() {
    try {
      await writeText({ text: String(count) });
      setMessage('Copied');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <View style={{ padding: 16, gap: 8 }}>
      <Text>Count: {count}</Text>
      <Button title="Increment" onPress={() => setCount((value) => value + 1)} />
      <Button title="Copy count" onPress={copyCount} />
      <Text>{message}</Text>
    </View>
  );
}
```

如果用编辑器或 `tsc` 做类型检查，把以下声明保存为 `host-modules.d.ts` 并纳入 TypeScript 项目，同时让 TypeScript 能解析本地的 `keel` 和 React 包。Keel 编译器会按 Host 契约链接 `host:*` 导入。

```ts
declare module 'host:clipboard' {
  export function writeText(input: { text: string }): Promise<void>;
}
```

## 2. 声明并构建 Workload

把下面的内容保存为 `manifest.toml`。这个包申请 UI 渲染和剪贴板写入权限。写入剪贴板时，Actrium 仍会向用户请求授权。打包器会填入文件大小、SHA-256 摘要和签名密钥 ID。

```toml
format = "actr-package/2"

[actor]
publisher = "dev.example"
name = "counter-front"
version = "0.1.0"

[workload]
profile = "keel.guest/1"
entry = "workload/guest.js"

[workload.keel]
runtime = "js"
guest-api = "keel.guest/1"
capabilities = "workload/capabilities.json"

[host]
profile = "actr.host/1"
api = "^1.0.0"
required-capabilities = ["ui.render", "host:clipboard.write"]

[signature]
algorithm = "ed25519"
key-id = "placeholder"

[[files]]
path = "workload/guest.js"
role = "workload"
media-type = "text/javascript"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"

[[files]]
path = "workload/capabilities.json"
role = "profile"
media-type = "application/json"
size = 0
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
```

在示例目录执行 v1 构建。编译器会同时输出 Guest bundle 和能力清单，两者应来自同一次构建。能力清单记录 Guest 用到的导入和组件，`manifest.toml` 声明这个包申请的权限。

```sh
bun "$actrium_repo/packages/keel/src/cli/bin.ts" build guest.tsx \
  --contract "$actrium_repo/packages/actrium/src/runtime-host/contract.ts" \
  --v1-capability-manifest workload/capabilities.json \
  --outfile workload/guest.js
```

## 3. 签名并验证 .actr 包

在示例目录生成一次开发密钥，重打包时复用。单独导出的公钥文件用于验签。

```sh
actr pkg keygen --no-register --publisher dev.example \
  --output dev-key.json --public-key-output dev-public-key.json
```

打包示例并检查签名和内容。归档内包含清单声明的 payload。

```sh
actr pkg pack --manifest-path manifest.toml --payload-root . \
  --key dev-key.json --output dist/counter-front.actr
actr pkg verify --package dist/counter-front.actr --pubkey dev-public-key.json
```

## 4. 在 macOS 打开并重建

1. 将 `dev-public-key.json` 里的**公钥**加入 Actrium 的本地信任库 `~/Library/Application Support/Actrium/trusted-keys.json`。文件内容是 JSON 数组，元素形如 `{"public_key":"<Base64 公钥>"}`。文件已存在时追加一项。CLI 验签和 Actrium 的本地信任库各自使用自己的公钥来源。
2. 用 `printf '%s\n' "$PWD/dist/counter-front.actr"` 查看绝对路径，并粘贴到 Actrium 地址栏打开包。
3. 点 **Increment**，确认数字变化。点 **Copy count**，在 Actrium 提示时允许剪贴板写入，然后在文本框里粘贴检查。用户拒绝时，Guest 会在界面上显示错误。
4. 修改 `guest.tsx` 的按钮文字，用相同的密钥和输出路径重新构建、打包、验签，然后在 Tab 按 **⌘R**。如果 Tab 指向已安装副本，就把重建后的归档绝对路径粘贴到地址栏重新打开。

有关资源和展示元数据，见 [.actr 包清单说明](manifest-specification.zh.md)。

## 5. 调用 Realm Service

如果 Front 需要 Service，在 `[dependencies.service]` 中声明对应的 Service 包及 Proto 契约，申请 `service.call`，再导入具体的 RPC 路由。下面的片段使用 Echo Chat 的编码和解码函数。

```ts
import { call as echo } from 'host:service/echo.v1.EchoService.Echo';
import { decodeEchoReply, encodeEchoRequest } from './echo-wire';

async function requestEcho(message: string) {
  const reply = await echo({
    payloadBase64: encodeEchoRequest(message),
    timeoutMs: 8_000,
  });
  return decodeEchoReply(reply.payloadBase64);
}
```

在选中对应 Service 的 Realm 中打开 Front。剪贴板示例可以直接打开本地归档，调用 Service 则需要绑定 Realm 的 Tab。完整源码、打包命令和 Service 启动方式见 [Echo Chat](https://github.com/Actrium/actr-hub/tree/main/echo)。要在 Realm 中加载本地构建，见 [本地开发 Front](https://github.com/Actrium/actr-hub/blob/main/docs/develop-front-locally.md)。

## 排查问题

| 现象 | 检查位置 |
| --- | --- |
| `component_not_allowed` | 确认导入的组件在 Actrium 的[组件清单](front-api-reference.zh.md#组件)里。 |
| `capability_mismatch` 或缺少能力清单 | 重新生成同一套 bundle 和能力清单。检查包的权限声明和 `[workload.keel]` 中的清单路径。 |
| 剪贴板调用被拒绝 | 确认包声明了 `host:clipboard.write`，并检查用户是否在 Actrium 中授权。 |
| 无法调用或订阅 Service | 检查固定路由或事件名、Proto 契约、Service 是否在运行，以及 Realm 中的 Front 和 Service 是否匹配。 |
| 归档验签或加载失败 | 用配对公钥运行 `actr pkg verify`，再检查 Actrium 是否信任该公钥，以及 Tab 是否指向重建后的归档。 |

完整的 Host 方法、UI 组件和 Guest 运行时 API 见 [Front API 参考](front-api-reference.zh.md)。
