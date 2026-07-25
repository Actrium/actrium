# Guest 两条线：JS guest 与 native guest

Rill 用**两条互不相同的执行线**来跑 guest 应用。它们的 SDK 名字只差一个字符
（`rill/guest` 与 `rill-guest`），含义却相反；而旧的「rill vs wasm」简称又有误导。
本页把规范术语定下来，并把命名隐患记录清楚，让文档、issue、代码口径一致。

## 真正的区分轴：解释执行的 JS vs 编译成原生

区分这两条线的**不是**「是不是 wasm」——在 Web 上*两条线都*会编译成 wasm（QuickJS 以
`quickjs-sandbox.wasm` 形态承载 `sandbox: 'wasm-quickjs'`，而 native guest 是
Rust→wasm）。真正把它们分开的轴是**应用怎么写、怎么执行**：是解释执行的 JavaScript，
还是编译成原生代码。

| | **JS guest** | **native guest** |
|---|---|---|
| 应用用什么写 | JavaScript / React | Rust / C / Zig → wasm |
| 谁来执行 | JS 引擎（解释执行） | `WasmGuestHost`（直跑 wasm） |
| 怎么选择 | `EngineOptions.sandbox` —— `'node-vm' \| 'jsc' \| 'hermes' \| 'quickjs' \| 'wasm-quickjs' \| 'tenant-manager'`（`src/host/engine/types.ts`） | 走 `WasmGuestHost` 路径（`src/host/wasm-guest/`） |
| SDK | **`rill/guest`** —— JS 包（`package.json` 导出 `"./guest" → ./src/sdk`） | **`rill-guest`** —— Rust crate（`crates/rill-guest`） |
| 隔离 | JS 引擎上下文（QuickJS/JSC/Hermes/Node VM）；Web 上是 WASM 线性内存边界 | WASM 线性内存边界 |

`rill-guest` crate 自己的描述就是*「用于编写 rill **原生（非-JS）** WASM guest 的 Rust
SDK」*——代码本身清楚自己是什么，只是名字没说出来。

## 规范术语 —— 到处都用这两个

- **JS guest** —— 解释执行的 JavaScript 线：应用用 JS/React 写，由任意 JS 引擎（含
  `wasm-quickjs`）执行，通过 **`rill/guest`** SDK。
- **native guest** —— 编译成原生的线：应用用 Rust（或 C/Zig）写，编译成 wasm 后由
  `WasmGuestHost` 直接执行，通过 **`rill-guest`** crate。文档里也叫它*原生（直跑-wasm）*
  线。

在正文里需要明确区分时，就说**「JS-guest 线」**和**「native-guest（直跑-wasm）线」**。

## 弃用的简称 —— 不要再用

- ❌ **「rill 线」对「wasm 线」**。「wasm」不是区分轴（Web 上两条线都是 wasm），而
  「rill」指的是整个项目、不是某一条线。
- ❌ **`runtime: "rill"` / `runtime: "wasm"`**。这**不是真实存在的代码字段**，只是概念
  上的说法。真正的选择器是 `EngineOptions.sandbox`（JS 线）与 `WasmGuestHost` 路径
  （native 线）。

## 已知命名隐患

`rill/guest`（JS SDK）与 `rill-guest`（Rust/native crate）只差一个字符（`/` 对 `-`），
却指相反的两条线——读写都容易出错。

**已决策（2026-07-25）**：crate 改名为 **`rill-native-guest`**，宿主类改名为
**`NativeGuestHost`**（连同整个 `NativeGuest*` 家族与 `src/host/native-guest/` 目录），
并保留 `@deprecated` 向后兼容别名，避免静默破坏下游。执行**推迟**到几个正在进行、改动
同批文件的功能 PR 落地之后，届时一次性干净改名；具体在命名 issue 里跟踪。在它落地之前，
本术语表就是这些术语的权威口径。

另见：[Sandbox 引擎对比](./sandbox-comparison.zh.md)（JS-guest 的引擎后端）与
`docs/native-guest*.zh.md` 下的 native-guest 设计文档。
