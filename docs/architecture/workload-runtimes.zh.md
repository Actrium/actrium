# Hyper Profile Runtime 与执行引擎

状态：提议中的架构基线。

## 1. 目的

本文定义 Actor Package 的 Workload Profile、Hyper Profile Runtime、具体执行引擎和 Host
产品之间的层次与命名。目标是让以下概念保持独立：

- Package 声明自己遵循哪项稳定规范；
- Hyper 使用哪种 Runtime 装载该 Workload；
- Runtime 内部使用哪个执行引擎；
- 最终由哪个 Host 产品承载 Actor。

本文不定义各 Workload Profile 的完整 ABI。ABI、capability 和安全要求仍由对应 Profile
规范定义。

## 2. 术语与层次

### 2.1 Workload Profile

Workload Profile 是 Package 与 Host 之间带 major version 的规范契约。它定义：

- entry 的文件形态；
- Workload 与 Host 的 ABI；
- Actor message 和 lifecycle 的映射方式；
- Workload 可以调用的 Host capability；
- target、资源限制和安全要求。

Package 通过 `[workload].profile` 声明 Profile，例如：

```toml
[workload]
profile = "actr.component/1"
entry = "workload/service.wasm"
```

Profile 描述可移植契约，不指定某个具体执行引擎。

### 2.2 Profile Runtime

Profile Runtime 是 Hyper 中负责实现一个或多个 Workload Profile 的装载器。它至少负责：

1. 判断是否支持给定 Profile major version；
2. 验证 Profile-specific manifest 和 entry；
3. 实例化 Workload；
4. 映射 Actor RPC、lifecycle 和 Host capability；
5. 执行资源限制；
6. 在 Actor 停止时可靠地释放 Workload。

Profile Runtime 是 Hyper 的一级扩展点。

### 2.3 Execution Engine

Execution Engine 是 Profile Runtime 内部真正执行代码的机制，例如 Wasmtime、Hermes、JSC
或 QuickJS。Engine 通常是 Host 的实现选择，不属于 Actor Package 的可移植身份。

同一个 Profile 可以存在多个兼容 Runtime 实现。例如 `actr.component/1` 可以由 Wasmtime
实现，也可以由未来另一个完全兼容 WebAssembly Component Model 和 Actr ABI 的引擎实现。

### 2.4 Host

Host 是最终承载 Actor 的可信产品或进程。它组合：

- Hyper Core；
- 一个或多个 Profile Runtime；
- 平台存储、密钥和权限实现；
- 产品或部署策略。

Actrium 是交互型 Host；基于 Wasmtime 的 headless 进程也是一种 Host composition。Host 名称
不等同于 Profile 或 Engine 名称。

## 3. 总体结构

```text
Host
└── Hyper Core
    └── Profile Runtime                         绑定 Workload Profile
        ├── Actr Component Runtime              actr.component/1
        │   └── Wasmtime Engine
        ├── Actr Native Runtime                 actr.native/1
        │   └── Dynamic Library Engine
        ├── Actr Linked Runtime                 actr.linked/1
        │   └── In-Process Engine
        └── Keel Guest Runtime                  keel.guest/1
            └── Keel Sandbox Engine
                ├── Hermes
                ├── JSC
                ├── QuickJS
                └── WASM
```

Keel Guest Runtime 与其他 Profile Runtime 在 Hyper 层平级。Keel 内部的 Sandbox Engine
才是它的下一级实现。

## 4. 为什么使用 `component`，而不是 `wasm`

`wasm` 只说明使用 WebAssembly 技术，不能唯一确定可执行契约。它可能表示：

- WebAssembly Core Module；
- WebAssembly Component Model；
- 浏览器中的 `wasm32-unknown-unknown` 模块；
- 某个 WASI 版本的 command 或 reactor；
- 带私有 imports/exports 的自定义 ABI。

`actr.component/1` 则明确表示 WebAssembly **Component Model** Workload，并由下级规范绑定
具体 WIT world、异步调用模型、Host imports、lifecycle 和资源规则。因此：

- Profile 使用 `component`，准确描述稳定契约；
- 实现名使用 `wasmtime`，准确描述具体 Engine；
- `.wasm` 仍然可以是 entry 文件扩展名，但扩展名不定义 Profile。

推荐称呼是 **Actr Component Runtime** 和 **Wasmtime Component Runtime**，而不是含义过宽
的 “Actr Wasm Runtime”。

## 5. 规范名称与实现名称

| Workload Profile | 规范类型名 | 首个具体实现名 | 建议包名 |
| --- | --- | --- | --- |
| `actr.component/1` | Actr Component Runtime | Wasmtime Component Runtime | `actr-hyper-component-wasmtime` |
| `actr.native/1` | Actr Native Runtime | Dylib Native Runtime | `actr-hyper-native-dylib` |
| `actr.linked/1` | Actr Linked Runtime | In-Process Linked Runtime | `actr-hyper-linked-inproc` |
| `keel.guest/1` | Keel Guest Runtime | Keel Guest Runtime for Hyper | `actr-hyper-keel` |

命名顺序遵循：

```text
<platform>-<subsystem>-<profile-kind>-<implementation>
```

其中最后一段只有在存在独立实现选择时才需要。Keel 已经是具有独立身份的 Runtime，因此
无需再造一个同义实现名。

## 6. Profile 与实现的机器绑定

包名只服务于人类识别，不能成为合规判断依据。每个 Runtime 必须通过注册描述或代码常量
明确声明支持的 Profile。下面是概念接口示例，不预先固定最终 Rust API：

```rust
pub trait ProfileRuntime {
    const PROFILES: &'static [&'static str];

    // verify、instantiate、dispatch、lifecycle、shutdown 等接口省略。
}

pub struct WasmtimeComponentRuntime;

impl ProfileRuntime for WasmtimeComponentRuntime {
    const PROFILES: &'static [&'static str] = &["actr.component/1"];
}
```

Keel adapter 的等价声明为：

```ts
class KeelGuestRuntime implements ProfileRuntime {
  readonly profiles = ["keel.guest/1"] as const;
}
```

Host 必须根据此声明和 Profile compatibility rules 选择 Runtime。未知 Profile major version
必须被拒绝，不能因为 entry 文件扩展名看起来可执行就猜测 Runtime。

## 7. 各 Runtime 的职责

### 7.1 Actr Component Runtime

`actr.component/1` 面向 WebAssembly Component Model。Wasmtime 实现负责：

- 编译及实例化 Component；
- 提供规定的 WIT Host imports；
- 映射 Actor RPC 和 lifecycle；
- 应用内存、并发、时间及其他资源限制；
- 关闭 Store 并清理 Actor execution。

Package 只依赖 `actr.component/1`，不应仅因当前实现使用 Wasmtime 就声明 Wasmtime 私有依赖。

### 7.2 Actr Native Runtime

`actr.native/1` 面向平台原生动态库。首个实现通过 `.so`、`.dylib` 或 `.dll` 装载稳定的
Actr native ABI。它具有比 sandboxed Component 更高的本地权限，Host 应采用更严格的来源、
签名和安装策略。

`dylib` 是实现机制，不是 Profile 名称；Profile 保持 `native`，以便未来支持其他合规的
原生装载形式。

### 7.3 Actr Linked Runtime

`actr.linked/1` 面向构建时链接进 Host 的 Workload。首个实现是 In-Process Linked Runtime，
不需要从 Package 动态装入 executable entry，但仍必须遵循 Actor lifecycle、identity、RPC
和 capability 边界。

若该实现始终是 Hyper Core 的内置能力，可以不单独发布 crate；其正式实现名和 Profile
绑定仍然保留。

### 7.4 Keel Guest Runtime

`keel.guest/1` 面向由 Keel 装载的交互型 Guest Workload。`actr-hyper-keel` 只表示 Hyper 与
Keel 的适配层，不代表整个 Keel 项目。它负责：

1. 验证并装载 Keel Guest entry；
2. 将 Actor message 和 lifecycle 映射到 Keel Guest；
3. 将 Guest 的 Actor 调用映射到受控 Hyper capability；
4. 提供并约束 `ui.render` 等交互能力；
5. 在 Actor 终止时销毁对应 Keel execution。

Keel 自身还包含 Host UI、Guest SDK、renderer、contracts、devtools 和 bundler。Hermes、JSC、
QuickJS、Keel WASM 等属于 Keel Sandbox Engine，是 Keel Guest Runtime 的内部下一级。

## 8. Package 声明

### 8.1 Component Workload

```toml
[workload]
profile = "actr.component/1"
entry = "workload/echo.wasm"

[host]
profile = "actr.host/1"
api = "^1.0.0"
required-capabilities = ["actor.call", "actor.tell"]
```

Host 可以选择 Wasmtime Component Runtime。Package 不需要知道这一选择。

### 8.2 Keel Guest Workload

```toml
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
required-capabilities = ["actor.call", "actor.tell", "ui.render"]
```

`runtime = "js"` 描述 Keel Guest 的语言/runtime family，不强制指定 Hermes、JSC 或 QuickJS。
具体 Sandbox Engine 由 Host 根据平台、策略和可用能力选择。

## 9. 建议代码与包结构

```text
actr/
└── core/
    └── hyper/                         后端无关的 Hyper Core

crates/
├── actr-hyper/                        Hyper Core crate
├── actr-hyper-component-wasmtime/     actr.component/1 实现
├── actr-hyper-native-dylib/           actr.native/1 实现
└── actr-hyper-linked-inproc/          actr.linked/1 实现；可按需内置

packages/
├── actr-hyper-keel/                   keel.guest/1 的 Hyper adapter
├── keel-host/
├── keel-guest/
├── keel-runtime/
└── keel-backend-*/                    Hermes、JSC、QuickJS、WASM 等
```

物理目录可以在迁移期间保留现状；Cargo/npm package name 和公开类型名应先遵循上述层次，避免
再次把 Profile、Runtime、Engine 和 Host 混在一个名称中。

## 10. 兼容性与选择规则

1. Package 声明 Workload Profile，不声明普通实现偏好。
2. Host 在验证 Package、签名、Profile 和 capability 后选择 Profile Runtime。
3. Profile Runtime 可以选择或配置其内部 Engine。
4. Engine 选择不得降低 Profile 规定的安全、ABI 和资源保证。
5. 同一 Host 可以同时安装多个 Profile Runtime。
6. Runtime 可以支持多个明确声明的 Profile，但不得对未知 major version 自动降级。
7. Actrium 产品状态、Tab、window、icon 等不得进入通用 Keel Guest ABI；这些内容属于
   Actrium extension。

## 11. Echo 示例中的组合

Headless Echo Service：

```text
Headless Host
├── Hyper Core
├── Wasmtime Component Runtime
└── echo-service.actr                 actr.component/1
```

Actrium Echo Client：

```text
Actrium Host
├── Hyper Core
├── Keel Guest Runtime
│   └── 平台选择的 Keel Sandbox Engine
└── echo-client.actr                  keel.guest/1
```

两端都是普通 Actor。Loran 提供 authority discovery、信令和 ICE 基础设施，不参与选择
Workload Runtime。

## 12. 与 Actor Package 规范的关系

Actor Package 格式及 `[workload]`、`[host]` 字段由
[`actor-package.zh.md`](../../actr/protocols/actor-package.zh.md) 定义。本文只固定其运行时层次、
正式类型名和首批实现命名；各 Profile 的完整 ABI 仍需分别形成下级规范。
