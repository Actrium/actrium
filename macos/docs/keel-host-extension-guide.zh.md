# 扩展 macOS Actrium Keel Host

[English](keel-host-extension-guide.md) · [Front 开发指南](keel-askit-guest-guide.zh.md) · [Front API 参考](front-api-reference.zh.md)

本文面向为 macOS v1 JS Front 开放接口的维护者。编译器把 Guest 的导入记录到 `workload/capabilities.json`。Host 加载前会从 bundle 的链接记录独立重建这份清单，只绑定通过验证的接口。可调用的能力还需要包声明和运行时授权策略。[Bundle 校验与 Host 绑定](../../packages/actrium/src/runtime-host/v1-guest.ts)

| Guest 的需求 | 扩展类型 | Guest 的使用方式 |
| --- | --- | --- |
| 调用 Host 执行一次操作并获取结果 | [Host RPC](#host-rpc) | 从 `host:*` 具名导入，返回 Promise |
| 接收 Host 持续推送的更新 | [Host 订阅](#host-订阅) | 从 `host:*` 具名导入，返回取消订阅函数 |
| 调用已绑定 ACTR Service 的业务方法或接收业务事件 | [Service 路由与传输](#service-路由与传输) | 从 `host:service/<name>` 具名导入 `call` 或 `on` |
| 在 React 树里渲染一个元素 | [UI 组件](#ui-组件) | 从 `keel/guest` 具名导入组件 |
| 使用沙箱内的工具函数、运行时模块或全局对象 | [Guest SDK 与运行时](#guest-sdk-与运行时) | 使用 SDK 或模块导入，或使用由沙箱显式提供的全局 API |

## Host RPC

以现有剪贴板写入为例。Guest 导入的模块是 `host:clipboard`，方法是 `writeText`，包清单声明的能力 ID 是 `host:clipboard.write`。

1. **定义契约。** 在 `contract.hostModules` 对应的模块下添加 `rpc<Input, Output>()`，校验 Guest 输入及需要校验的返回值。剪贴板写入接收 `{ text: string }`，调用成功后直接完成。[Host 契约](../../packages/actrium/src/runtime-host/contract.ts)
2. **映射能力 ID 并设置策略。** 在 `FRONT_CAPABILITY_SPECS` 添加或更新能力 ID 与 Keel 模块名、方法名的映射。在 `FRONT_CAPABILITY_DEFINITIONS` 配置开放对象、授权要求，以及适用的系统权限或原生代理依赖。给已有模块增加方法时，检查模块简写是否会自动涵盖新增方法。新 Guest 包使用方法级能力 ID。[能力 ID 与简写](../../packages/actrium/src/runtime-host/capabilities.ts) · [授权策略](../../packages/actrium/src/runtime-host/capability-model.ts)
3. **实现方法并检查授权。** 在 `createHostModules` 中实现方法，先完成授权检查再执行操作。剪贴板实现先等待 `authorizeCapability('host:clipboard.write')`，再调用 macOS 剪贴板适配器。由 Host 接入平台依赖。对于在 renderer 进程之外执行的能力，可参考文件选择器，在原生操作实际执行的位置再次核对当前 Tab、授权和参数。[Host 实现](../../packages/actrium/src/runtime-host/host-modules.ts) · [macOS 接线](../../packages/actrium/src/runtime-host/StandaloneGuestApp.tsx) · [原生文件选择器](../Actrium/MultiProcess/RendererFileImport.swift)
4. **将方法加入开放列表。** 把实际实现的导出加入 `HOST_SUPPORTED_METHODS`，并将能力定义的 `implemented` 设为 `true`。可用模块列表由方法列表派生，有效授权集合则使用能力定义。两处要同步。[开放方法列表](../../packages/actrium/src/runtime-host/host-modules.ts) · [有效能力定义](../../packages/actrium/src/runtime-host/capability-model.ts)
5. **用 Guest 验证。** 给导入添加类型声明，在测试 Front 包清单的 `[host]` 中声明准确的能力 ID，并用 Actrium 契约构建。检查生成的能力清单，再从 Guest 调用。测试包未声明能力、输入无效和用户拒绝的情况。请求被拒绝时，原生操作应保持未执行。[Guest 构建与打包](keel-askit-guest-guide.zh.md) · [Host 方法测试](../../packages/actrium/src/runtime-host/host-modules.test.ts)

从 Guest 的事件处理函数调用下面的辅助函数。测试包声明对应能力后，它会走公开剪贴板 API 的完整 Host 路径。

```ts
import { writeText } from 'host:clipboard';

async function copyLabel(text: string) {
  await writeText({ text });
}
```

## Host 订阅

模块导出、能力 ID、授权策略和开放列表沿用 Host RPC 的步骤。契约使用 `subscription<Event>()`，并用 `parseEvent` 校验事件数据。编译器会把该导出写入 `events`。订阅应立即返回一个取消订阅函数，还要在 Engine 销毁或 Tab 关闭时清理。受保护的事件源应先授权再连接。若授权可撤销，撤销后应停止投递并清理订阅。

`host:speech.onInterimResult` 的事件来自同一个 Guest 已授权的 `recognize` 调用。`service.on` 展示了如何取消待完成的授权并清理长期连接。测试两个 Guest 之间的隔离、取消订阅、关闭 Tab，以及适用时的权限撤销。[订阅契约](../../packages/actrium/src/runtime-host/contract.ts) · [订阅实现](../../packages/actrium/src/runtime-host/host-modules.ts) · [订阅测试](../../packages/actrium/src/runtime-host/host-modules.test.ts)

## Service 路由与传输

新增业务 RPC 或事件时，沿用现有的 `service.call` 或 `service.on`。更新 Service 及 Proto 或事件契约，在 Front 导入准确的业务名称，再验证绑定的 Service 是否允许访问。Host 从导入中确定固定路由，并按导入的事件名过滤订阅。这类新增路由沿用现有 Host 能力 ID。[Front 调用 Service 示例](keel-askit-guest-guide.zh.md#5-调用-realm-service) · [固定路由绑定](../../packages/actrium/src/runtime-host/v1-guest.ts)

当 Host 新增传输操作时，需要扩展 Host API。像 `uploadAttachment` 这样直接由 `host:service` 开放的操作，除了上面的 Host API 步骤，还要同步更新链接器的 `V1_SERVICE_TRANSPORTS` 和 Host 清单校验的 `SERVICE_TRANSPORT_CAPABILITIES`。检查包是否必须用精确能力 ID 声明，再验证当前 Tab 的可用性，以及要求精确声明时未声明的包会被拒绝。[v1 链接规则](../../packages/keel/src/v1.ts) · [Service 清单校验](../../packages/actrium/src/runtime-host/capability-verify.ts) · [传输链接测试](../../packages/actrium/src/runtime-host/v1-guest.test.ts)

## UI 组件

一个组件要经过 SDK 导出、v1 组件白名单和 macOS 组件注册表才能开放。各处使用一致的名称。`Image` → `ImageAsset` 这类已有协议别名，由链接器与注册代码显式映射。

1. **定义 Guest props 和回调。** 在 `sdk.ts` 用 `declareHostComponent<Props>('Name')` 声明组件，再从 `sdk/index.ts` 导出组件和公开类型。只向 Guest 提供所需的 props 和事件数据。[Guest SDK 实现](../../packages/keel/src/sdk/sdk.ts) · [SDK 导出](../../packages/keel/src/sdk/index.ts)
2. **加入组件白名单。** 将清单里的组件名加入 `V1_COMPONENTS`。Actrium 的 `COMPONENT_WHITELIST` 复用同一份列表。v1 构建会根据具名导入把组件写入清单。[组件白名单](../../packages/keel/src/v1.ts) · [Host 策略](../../packages/actrium/src/runtime-host/capability-model.ts)
3. **注册实际渲染实现。** 将渲染器接到 `StandaloneGuestApp` 的注册表。v1 Guest 只会收到通过清单验证的注册项。如果组件依赖可选原生模块或构建开关，在加载引用它的 Guest 前检查是否可用。`Supramark` 是一个依赖构建配置的例子。[macOS 注册表和可用性检查](../../packages/actrium/src/runtime-host/StandaloneGuestApp.tsx)
4. **验证 props 和事件。** 用 v1 Guest 验证渲染和回调。在组件包装层或原生适配器测试无效 props，并确认未链接的组件不可用。`Image` 展示了如何限制可传入的 props 并在 Host 检查资源。[图片 props 过滤](../../packages/actrium/src/runtime-host/image-asset.ts) · [图片测试](../../packages/actrium/src/runtime-host/image-asset.test.ts) · [Engine 注册范围测试](../../packages/actrium/src/runtime-host/v1-guest.test.ts)

### 由原生 UI 支持的组件

`Select` 和 `Menu` 属于 UI 组件。renderer 在组件被点击时请求 macOS 菜单适配器。原生处理入口检查 Tab、刷新标识、可见区域和清单中的组件名，再把用户选中的值返回给组件回调。此类组件还要验证菜单取消、原生桥接不可用时的拒绝，以及 Tab 切换或组件卸载后的旧回包。[Guest 菜单组件](../../packages/actrium/src/runtime-host/guest-menu.tsx) · [macOS 菜单桥接](../ActriumViewService/GuestMenuModule.swift) · [原生菜单处理](../Actrium/MultiProcess/RendererGuestMenu.swift)

在启用了原生菜单桥接的 macOS 构建中，可以用下面的 Guest 验证 `Select`。

```tsx
import { useState } from 'react';
import { Select } from 'keel/guest';

export default function App() {
  const [value, setValue] = useState('small');
  return (
    <Select
      options={[
        { value: 'small', label: 'Small' },
        { value: 'large', label: 'Large' },
      ]}
      value={value}
      onValueChange={setValue}
    />
  );
}
```

## Guest SDK 与运行时

- **SDK 辅助函数。** 在 `keel/guest` 中实现并导出，再将具名导出加入 `V1_SDK_UTILITIES`。这些导出由沙箱里的 Guest bundle 提供，测试也应在沙箱里运行。UTF-8 和 Base64 工具可作为示例。[SDK 导出](../../packages/keel/src/sdk/index.ts) · [v1 SDK 白名单](../../packages/keel/src/v1.ts) · [Guest bundle](../../packages/keel/src/guest/bundle.ts) · [字节工具测试](../../packages/keel/src/sdk/sandbox-utils-bytes.test.ts)
- **运行时模块。** 如果要新增类似 React JSX runtime 的外部模块，同步更新 `V1_RUNTIME_IMPORTS`、编译器中从模块到全局变量的映射、沙箱提供实际值的入口和 Host 导入校验。验证该导入可以构建，并在受限 Engine 中解析。[v1 导入](../../packages/keel/src/v1.ts) · [编译器模块映射](../../packages/keel/src/cli/build.ts) · [Host 导入校验](../../packages/actrium/src/runtime-host/runtime-capability-verify.ts)
- **沙箱全局 API。** 在负责该 API 的 Engine 或 Guest 运行时初始化入口接入。在实际支持的沙箱引擎下验证行为，需要时覆盖生命周期。计时器和 console 可作为示例。访问系统资源或应用数据的操作通过 Host 能力提供。[Engine 全局 API 初始化](../../packages/keel/src/host/engine/engine.ts) · [Guest console 初始化](../../packages/keel/src/guest/runtime/globals-setup.ts)

## 验证扩展

参考现有的 [v1 编译器测试](../../packages/keel/src/cli/build.v1.test.ts)、[Host 清单测试](../../packages/actrium/src/runtime-host/runtime-capability-verify.test.ts)和 [Engine 端到端链接测试](../../packages/actrium/src/runtime-host/v1-guest.test.ts)。新增 macOS 原生操作或组件时，在 `macos/` 目录运行 `./scripts/bundle-actrium-runtime.sh --target system --dev` 重建系统 bundle，再用兼容的 Actrium 构建验证成功、拒绝和清理路径。

新接口可以开放后，同步更新中英文 [Front API 参考](front-api-reference.zh.md)，记录导入方式、契约或 props、使用条件和示例。
