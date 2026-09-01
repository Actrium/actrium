# Guest 两条线：JS guest 与 native guest

Keel 用**两条互不相同的执行线**来跑 guest 应用。过去用「keel vs wasm」这个简称来区分
它们，而那个简称是有误导的——见下面的区分轴。本页把规范术语定下来，让文档、issue、
代码口径一致。

## 真正的区分轴：解释执行的 JS vs 编译成原生

区分这两条线的**不是**「是不是 wasm」——在 Web 上*两条线都*会编译成 wasm（QuickJS 以
`quickjs-sandbox.wasm` 形态承载 `sandbox: 'wasm-quickjs'`，而 native guest 是
Rust→wasm）。真正把它们分开的轴是**应用怎么写、怎么执行**：是解释执行的 JavaScript，
还是编译成原生代码。

| | **JS guest** | **native guest** |
|---|---|---|
| 应用用什么写 | JavaScript / React | Rust / C / Zig → wasm |
| 谁来执行 | JS 引擎（解释执行） | `NativeGuestHost`（直跑 wasm） |
| 怎么选择 | `EngineOptions.sandbox` —— `'node-vm' \| 'jsc' \| 'hermes' \| 'quickjs' \| 'wasm-quickjs' \| 'tenant-manager'`（`src/host/engine/types.ts`） | 走 `NativeGuestHost` 路径（`src/host/native-guest/`） |
| SDK | **`keel/guest`** —— JS 包（`package.json` 导出 `"./guest" → ./src/sdk`） | **`keel-native-guest`** —— Rust crate（`crates/keel-native-guest`） |
| 隔离 | JS 引擎上下文（QuickJS/JSC/Hermes/Node VM）；Web 上是 WASM 线性内存边界 | WASM 线性内存边界 |

`keel-native-guest` crate 自己的描述就是*「用于编写 keel **原生（非-JS）** WASM guest 的 Rust
SDK」*——代码本身清楚自己是什么，只是名字没说出来。

## 规范术语 —— 到处都用这两个

- **JS guest** —— 解释执行的 JavaScript 线：应用用 JS/React 写，由任意 JS 引擎（含
  `wasm-quickjs`）执行，通过 **`keel/guest`** SDK。
- **native guest** —— 编译成原生的线：应用用 Rust（或 C/Zig）写，编译成 wasm 后由
  `NativeGuestHost` 直接执行，通过 **`keel-native-guest`** crate。文档里也叫它*原生（直跑-wasm）*
  线。

在正文里需要明确区分时，就说**「JS-guest 线」**和**「native-guest（直跑-wasm）线」**。

## 弃用的简称 —— 不要再用

- ❌ **「keel 线」对「wasm 线」**。「wasm」不是区分轴（Web 上两条线都是 wasm），而
  「keel」指的是整个项目、不是某一条线。
- ❌ **`runtime: "keel"` / `runtime: "wasm"`**。这**不是真实存在的代码字段**，只是概念
  上的说法。真正的选择器是 `EngineOptions.sandbox`（JS 线）与 `NativeGuestHost` 路径
  （native 线）。

## 那个命名隐患，以及它是怎么解决的

Rust crate 过去叫 `keel-guest`，与 JS SDK 的 `keel/guest` 只差一个字符（`-` 对 `/`），
指的却是相反的那条线——读写都容易出错。2026-07-25 决策，现已落地：

| 原来 | 现在 |
|---|---|
| `keel-guest` crate（`examples/keel/guests/rust/keel-guest`） | **`keel-native-guest`**（`crates/keel-native-guest`） |
| `keel_guest::keel_guest_main!` | `keel_native_guest::guest_main!` |
| `WasmGuestHost` / `WasmGuestView` / `WasmGuestEngine`（及各自的 `*Options`） | **`NativeGuestHost`** / `NativeGuestView` / `NativeGuestEngine` |
| `createWasmGuestEngine` | `createNativeGuestEngine` |
| `src/host/wasm-guest/` | `src/host/native-guest/` |

那 5 个 TypeScript 名字是从 `keel/host` 导出的，所以旧拼法作为 `@deprecated` 别名继续
导出，不静默破坏下游。Rust 侧则是干净断裂：该 crate `publish = false`，全部调用方都在
本仓库内，没有需要照顾的下游。

有两处刻意没改。公开的 JS 导出仍是 **`keel/guest`**——它是主线、是已发布的入口，改成
`keel/js-guest` 只换来对称性，代价是把大家真正在用的 API 搅动一遍。C guest SDK 仍是
`sdk/c/keel_guest.h`：改一个语言中立的头文件及其 `keel_guest_*` 符号是另一次破坏性变更，
命名 issue 并没有要求。

另见：[Sandbox 引擎对比](./sandbox-comparison.zh.md)（JS-guest 的引擎后端）与
`docs/native-guest*.zh.md` 下的 native-guest 设计文档。
