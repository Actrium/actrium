# 从 `rill` 迁移到 `keel`

项目已从 **rill** 更名为 **keel**。这次改名是纯机械的 —— 行为没有变化，没有新增
或删除任何能力 —— 但它在每一处对外命名上都是**破坏性**变更：npm 包名、guest ABI
符号、三个二进制线协议魔数、原生模块与 Android 包名，以及全部 `RILL_*` 环境变量。

没有任何兼容层。host 与 guest 必须处在改名的同一侧。

## 1. 包与入口

| 原来 | 现在 |
|---|---|
| `rill` | `keel` |
| `rill/host` | `keel/host` |
| `rill/guest` | `keel/guest` |
| `rill/host/preset` | `keel/host/preset` |
| `rill/host/web` | `keel/host/web` |
| `rill/wire` | `keel/wire` |
| `rill/contract` | `keel/contract` |
| `rill/devtools` | `keel/devtools` |
| `rill/cli` | `keel/cli` |
| `rill` 命令行 | `keel` |

Rust guest SDK crate 同步改名：`rill-native-guest` → `keel-native-guest`
（`crates/keel-native-guest`），入口宏为 `keel_native_guest::guest_main!`。
C guest 头文件为 `sdk/c/keel_guest.h`。

## 2. TypeScript API

所有带产品名的导出标识符都跟着改：

| 原来 | 现在 |
|---|---|
| `defineRillContract` | `defineKeelContract` |
| `RillContractShape` | `KeelContractShape` |
| `RillCapabilitiesManifest` | `KeelCapabilitiesManifest` |
| `RillErrorBoundary` / `RillErrorBoundaryProps` | `KeelErrorBoundary` / `KeelErrorBoundaryProps` |
| `RillKeyEvent` | `KeelKeyEvent` |
| `RillPlatformInfo` | `KeelPlatformInfo` |

生成的能力清单文件现在叫 `keel-capabilities.json`。

## 3. Guest ABI（对所有已编译 guest 是破坏性的）

改名前编译的原生 guest **无法实例化** —— 它的 import / export 名字对不上了，
必须基于 `keel-native-guest` 重新编译。

host → guest 的 import（模块 `env`）：

| 原来 | 现在 |
|---|---|
| `rill_host_call` | `keel_host_call` |
| `rill_send_batch` | `keel_send_batch` |
| `rill_log` | `keel_log` |

guest → host 的 export：

| 原来 | 现在 |
|---|---|
| `rill_init` | `keel_init` |
| `rill_alloc` | `keel_alloc` |
| `rill_resolve` | `keel_resolve` |
| `rill_on_event` | `keel_on_event` |
| `rill_abi_version` | `keel_abi_version` |
| `rill_wire_caps` | `keel_wire_caps` |

`KEEL_ABI_VERSION` **仍然是 1**。改名没有动线上布局、调用约定或 fail-closed 语义，
只动了符号拼写，因此 ABI 版本号不递增。

## 4. JS 沙箱全局量

沙箱里每一个 `__rill*` 全局量现在都是 `__keel*` —— `__keel_sendBatch`、
`__keel_emitEvent`、`__keel_onHostEvent`、`__keel_getConfig`、
`__keel_handleMessage`、`__keel_importHostModule`、`__keel_stageBinary`、
`__keel_takeBinary` 等等。基于旧全局量打好的 guest bundle 必须重新构建。

## 5. 二进制线协议魔数

三个姊妹线协议的 4 字节魔数全部重写。除了这 4 个首字节之外，帧结构、字段布局、
上限与 fail-closed 原因词表**逐字节不变**。

| 契约 | 原来 | 现在 |
|---|---|---|
| op-batch（`contracts/op-batch-wire.json`） | `RILL` `0x4c4c4952` | `KEEL` `0x4c45454b` |
| canvas（`contracts/canvas-wire.json`） | `RCNV` `0x564e4352` | `KCNV` `0x564e434b` |
| store/net bytes（`contracts/store-net-bytes.json`） | `RBS1` `0x31534252` | `KBS1` `0x3153424b` |

分流纪律原样成立：三者仍共享首字节（现在是 `0x4B` `'K'`，原来是 `0x52` `'R'`），
并且仍在第 2 字节相异 —— `'E'` `0x45`、`'C'` `0x43`、`'B'` `0x42` —— 所以只要 host
比较**完整的 `u32`**，每一帧就仍然被路由到唯一的解码器；喂错解码器的缓冲区依旧在
读取任何字段之前以 `bad-magic` 失败。不带字节流的调用仍然是纯 JSON，永远不会进入
信封。

所有 golden 向量都由 Rust oracle 重新生成，因此 `op-batch-wire.golden.json`、
`op-batch-wire.matrix.json`、`canvas-wire.golden.json` 与
`store-net-bytes.golden.json` 现在锁的是新魔数。硬编码旧值的解码器会立刻拒绝新
golden —— 这正是预期的 fail-closed 行为。

## 6. 原生集成

| 原来 | 现在 |
|---|---|
| `RillSandboxNative.podspec` / pod `RillSandboxNative` | `KeelSandboxNative.podspec` / pod `KeelSandboxNative` |
| `RillSandboxNativeTurboModule` | `KeelSandboxNativeTurboModule` |
| `RillQuickJSNativeTurboModule` | `KeelQuickJSNativeTurboModule` |
| `RillTenantManager` | `KeelTenantManager` |
| Android 包 `com.rill.sandbox` | `com.keel.sandbox` |
| Android 包 `com.rill.demo`（示例 App） | `com.keel.demo` |
| JNI 入口 `Java_com_rill_sandbox_*` | `Java_com_keel_sandbox_*` |
| JNI 动态库 `rillsandbox` | `keelsandbox` |
| postinstall `scripts/rill_postinstall.js` / `rill_post_install.rb` | `scripts/keel_postinstall.js` / `keel_post_install.rb` |

autolinking 会从 `react-native.config.js` 读到新名字
（`com.keel.sandbox.KeelSandboxNativePackage`），所以依赖 autolinking 的 App 只需
改依赖名。显式写死 pod 名或包名的 App 两处都要改。

## 7. 环境变量

每个 `RILL_*` 变量现在都是 `KEEL_*`，含义完全不变。接入方构建里最可能出现的几个：

- `RILL_SANDBOX_ENGINE` → `KEEL_SANDBOX_ENGINE`（`jsc` / `hermes` / `quickjs`），
  在 Podfile 里设置以选择原生沙箱引擎。
- `RILL_WIP_CDP_DEVTOOLS`、`RILL_WIP_NATIVE_SECURITY`、
  `RILL_WIP_BINARY_PROTOCOL`、`RILL_WIP_JSC_INSPECTOR` → `KEEL_WIP_*`。
- `RILL_QJS_*`、`RILL_WASM_*`、`RILL_IOS_E2E_*`、`RILL_ANDROID_E2E_*` → `KEEL_*`。

## 8. 没有变的东西

- ABI 版本（`1`）、协议版本（`1`）、所有上限、所有 opcode、所有 value tag、
  所有 `reason` 词元。
- 能力契约的形状、dispatch 表面、fail-closed 姿态。
- Apache-2.0 许可证、仓库历史。

只有仓库地址跟着名字一起搬了：`https://github.com/Actrium/keel`。
