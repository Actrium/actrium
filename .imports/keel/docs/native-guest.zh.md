# keel 原生（非-JS）WASM guest —— 地基

> 站在 keel 的角度，把「非-JS 绑定的原生 WASM guest」这条扩展的**地基**设计并夯实：让 app 直接编译成 `.wasm`（Rust / C / Zig…）作为 sealed guest，host:* 作 WASM import。**canvas 只是它日后的消费者之一**（重计算/原生库复用/polyglot 同样受益）——本文不被 canvas 这个具体需求束缚。
>
> 本文既是设计，也记录**已落地的 Phase A + B + C**：宿主侧 ABI（`NativeGuestHost`）+ 人体工学 Rust guest SDK（`store::put(k,v).await`、声明式 `render(view([text(…)]))`）+ 真 Rust `.wasm` guest 的 host:* 往返、封口负向、**渲染批次经真 receiver 物化出 UI**，全部 `bun test` 跑绿。

---

## 0. 定位

keel 现状 = **QuickJS-WASM 跑 JS guest**（沙箱是一个 WASM 模块，里面跑应用的 JS）。本扩展（记为 **②**，区别于 canvas 文档里的阶段号）= 让 **guest 本身就是原生编译的 `.wasm`**，不再套一层 QuickJS。

它把 keel 从「QuickJS 上的 sealed JS 框架」升级成「**语言中立的 sealed WASM 平台**」：多语言 guest、高性能层惠及**所有重计算**（加解密/编解码/ML/sqlite/ffmpeg 移植…不止图形）、复用现成原生库、把 host:* 边界形式化成**语言中立 ABI**、零拷贝数据路径、封口论证回归 WASM 本质。

「打好基础」= 先夯**装载模型 + 语言中立 host:\* ABI（含异步）**，再谈渲染 / canvas / 多语言 SDK。

## 1. 关键洞察：异步不用重造

回源发现：keel 现有的**隔离域（WASM）host 调用本来就是「同步 post + 回调 resolve」**——guest 同步 `__sendToHost` 拿即时返回，真正的异步结果由 host 之后**按 callback id 回调 guest**来 resolve（`__keel.callbacks` / `invokeCallback`，见 `src/host/sandbox/providers/quickjs-native-wasm-provider.ts`）。

原生 guest 的异步用**同一套模型**，只是把「JS 值 + `__sendToHost`」换成「**线性内存指针 + 导出函数**」。所以 ② **不是发明异步，是把现有 sync-call + callback-resolve 形式化成语言中立 ABI**——这是地基能稳、且与现有实现同构的原因。

## 2. 地基三件

**① 装载模型**（平行于现有 `loadBundle → evalCode(JS)`）：新增 `WebAssembly.instantiate(appWasm, importObject)`，注入 host:* + 运行时导入，调 guest 导出入口。它**不是** `JSEngineProvider`（那是「JS 引擎跑 JS」）——是「实例化 app 自己的 `.wasm`」这一新概念，由 `NativeGuestHost` 承载。

**② host:\* ABI（线性内存，语言中立）—— 地基核心**：字节按**指针+长度**在 guest 线性内存里传，host 持 guest 的 `Memory` **零拷贝读**。比 QuickJS 路线的序列化拷贝 + `number[]` 绕 TypedArray 疤**更简单更快**。完整 wire 见 §4。

**③ 封口（回归 WASM 本质）**：`importObject` 完全由 host 控制。guest 只拿到 host:* + `keel_alloc`/`keel_log`，**无 fetch / socket / RTC**（WASM 无 ambient 网络，host 不导入就没有）；**未声明的 host 模块 fail-closed**（resolve `ok=0`）。封口由 **WASM import 模型结构性保证**——比「QuickJS 恰好没有 fetch」更标准可审。发布判档（声明的能力 + egress → green/lime）不变。

## 3. 与现有 QuickJS 兼容——加法式，不动现有

**共享、一行不改**（兼容的根本）：
- **host:\* 派发层**（`createHostModuleDispatch`）+ 契约 + 壳层 broker + 信任/闸门——两类 guest 打进**同一批 host 实现**，`host:store` 写一遍、JS guest 和原生 guest 都用。`NativeGuestHost` 直接复用 `createHostModuleDispatch` 的产物。
- **渲染 receiver + 二进制 op 批次协议**——原生 guest（Phase C）发**同一套渲染批次**，receiver 照旧物化。
- **异步 callback-resolve 模型**——现有 QuickJS 用的就是它，原生 ABI 用同一套（只换载体），异步语义一致、不是兼容裂缝。

**新增/不同的只有 guest 侧那根线**：QuickJS 走「JS 值 `__sendToHost`/`__keel_fn_ret`」，原生走「线性内存 `ptr+len` 的 `keel_host_call`/`keel_resolve`/`keel_alloc`」。**这两根线是同一派发层上的两个适配器**——原生桥把线性内存字节解码成派发层期望的值形状 → 跑现有 `parseInput/impl/parseOutput` → 结果编码回 guest 内存。**派发层不变 → QuickJS 路径原封不动 → 零回归**。

一句话：**现有 QuickJS-WASM 是这个底层 host↔WASM 接口的一个「消费者」（它在上面跑 JS）；原生 guest 是另一个消费者。② 是把 QuickJS 早在用的那条桥抽成语言中立 ABI，不是替换它。**

**v1 兼容 / 终局最优**：首版走「原生桥把字节解码成派发层今天期望的值形状」（派发层偏 JS 值，还带 `number[]` 绕 TypedArray 的疤），以**复用现有派发、零回归上线**。**终局是最优性能**——让派发层原生走字节、**零拷贝到底**，并把 QuickJS 那条桥也重构到这条统一 ABI 上（收成一条边界）。

之所以敢「先兼容、后重构求最优」，是因为**这层是内部边界、不在顶部**：app 作者写高层 SDK（`store::get().await`），不直接碰 `keel_host_call`；QuickJS 那条桥更是纯内部实现。所以**内部调整安全**，v1 刻意**不过度冻结** ABI 的形状。

**诚实的临界点**：一旦有**预编译的原生 guest 被分发**（不再随 host 版本一起重编），ABI 就从「可自由 churn 的内部边界」变成「带版本的兼容面」——那时才需要 api 版本化处理（与 keel api-stability 的触发条件一致：等真实外部分发边界出现再定型，不预造）。在那之前，内部重构求最优是安全且预期内的。

## 4. ABI v0 规格（headless / 地基，已实现）

**host → guest imports**
| import | 签名 | 语义 |
|---|---|---|
| `env.keel_host_call` | `(mod_ptr,mod_len, method_ptr,method_len, in_ptr,in_len, cb_id)` | guest 发起一次 host:* 调用；**同步返回、不阻塞**；结果稍后经 `keel_resolve` 回来 |
| `env.keel_log` | `(ptr,len)` | guest 日志 |

**guest → host exports**
| export | 签名 | 语义 |
|---|---|---|
| `memory` | — | guest 线性内存，host 零拷贝读 |
| `keel_alloc` | `(size) -> ptr` | host 要往 guest 内存写结果时，让 guest 分配缓冲 |
| `keel_resolve` | `(cb_id, ok, res_ptr, res_len)` | host 异步完成后调它，把结果交回 guest（驱动 guest 侧 future/Promise） |
| `keel_init` | `()` | 入口 |

**wire**：请求/响应字节是 guest 线性内存里的 **UTF-8 JSON**，按 `(ptr,len)` 寻址。`ok=1` 正常、`ok=0` 失败（body 为 `{error}`）。

> JSON 是地基阶段的务实选择（与现有 JSON 默认编码一致、跨语言好实现）；高频/大载荷路径后续可换二进制编码，ABI 形状不变。

### 4.1 一次调用有两级状态，别只看第一级

上面那个 `ok` 是**传输级**的：它只回答"这个 host 模块有没有跑起来、有没有给出答复"。它**不回答**"模块答应不答应干这件事"。

模块自己的态度写在**响应体**里。凡是契约（`contracts/graphics-seams.json`）声明了 `"ok":boolean` 的方法 —— `canvas.present`、`canvas.draw`、`gpu.submit`、`gpu.configure`、`gpu.createResource`、`asset.blit` —— 一次被拒绝的调用长这样：传输级 `ok=1`（调用完全成功），响应体 `{"ok":false,"reason":"size-mismatch"}`（模块拒绝了）。两件事都是真的，而且不冲突。

拿 `present` 举例把过程走一遍：guest 要贴一帧 8600 设备像素宽的画面，宿主的上限是 8192，于是宿主原地丢掉这一帧，回一句 `{"ok":false,"reason":"invalid-input"}`。传输层一切正常，所以只看第一级的话，guest 收到的是"成功"。它会接着画下一帧、再下一帧，而窗口从此不再更新 —— 两边都不报错，什么日志也没有。这就是 issue #85 的现场。

所以 SDK 的规矩是：**凡是契约声明了体内 `ok` 的方法，包装函数把两级折成一个 `Result`，`Ok` 就代表这件事真的发生了。** 被拒绝时原样返回宿主的响应体，`reason` 还在里面，从 `Err` 里读。两级仍然分得清 —— 传输失败的 body 里根本没有 `ok` 字段可读。

判定是**失败关闭**的：没有明确 `"ok":true` 就算拒绝，包括契约声明了却漏掉这个字段的响应。实现在 `crate::module_result`，回归测试在 `crates/keel-native-guest/src/module_verdict_tests.rs`。

## 5. Phase A 已落地 + 后续

**Phase A · 装载 + ABI（本 PR，已跑绿）**
- `src/host/native-guest/native-guest-host.ts` —— `NativeGuestHost`：`instantiate` + `importObject` 注入 `keel_host_call`/`keel_log`，桥接 `keel_host_call → createHostModuleDispatch → keel_resolve`，`drain()` 等待在途调用。
- `src/host/native-guest/__tests__/fixtures/roundtrip.wat`（+ 编译产物 `.wasm`）—— 一个**手写最小原生 guest**，`keel_init` 发一次 `host:store.putText`，wire 级看清 ABI。
- `src/host/native-guest/__tests__/native-guest-host.test.ts` —— 两个真测试：**① 端到端往返**（guest 经线性内存调 host:* → 真派发 → `keel_resolve` 回传 → 断言结果 `{version:1}` + KV 实存了 guest 的写）；**② 封口 fail-closed**（未声明模块 → `ok=0` + `not registered`）。

**Phase B · Rust guest SDK（本 PR 已落地）**：`crates/keel-native-guest`（lib）—— 人体工学 SDK，开发者写 `store::put(k, v).await`，crate 处理 alloc / host_call / resolve / **future**（一个 no_std 单任务 executor：host 调用是个 future，首 poll 发 `keel_host_call` 挂起，host 之后 `keel_resolve` 唤醒再 poll 完成——即 guest 侧的 callback-resolve）；ABI 导出（`keel_alloc`/`keel_resolve`/`keel_init`）+ 全局 bump 分配器 + panic handler 由 `guest_main!` 宏在 guest cdylib 里生成。`crates/kv-guest` = 用它写的真 Rust guest，`crates/build.sh` 编成 `.wasm`（被 Phase A 的 `NativeGuestHost` 原样加载，测试驱动 `store::put("a","b").await` 端到端跑通）。

> **封口自证**：编出的 `kv-guest.wasm` 的 import 列表**只有 `env.keel_host_call` 一个**——guest 能触达的全部就在这张表里，无 fetch / socket / 任何网络原语。import 模型即沙箱，肉眼可查。

**Phase B 后续（未做）**：CLI 增加 `.wasm` guest 构建目标（现只出 JS bundle）、补 `host:net` 等更多能力的 typed 包装、host:store 活桥 e2e。

**Phase C · 渲染（本 PR 已落地首版）**：原生 guest 第一次**出 UI**。SDK 加了声明式构建 `ui::view([ui::text("…")])` + `render(root)`——把元素树走成一个**渲染批次**（`{version,batchId,operations:[CREATE/APPEND…]}`，文本节点为 CREATE type `__TEXT__` + `props.text`，与 JS reconciler 一致）经**新增的单向 ABI 通道 `keel_send_batch(ptr,len)`** 交给 host；`NativeGuestHost` 解码成 `OperationBatch` → 转交 `onRenderBatch`（**decoupled**：host 不硬编码 receiver）→ 喂给**真 `receiver.applyBatch`** 物化。`crates/ui-guest` 渲染 `View > [Text("hello from rust"), View > Text("nested")]`，测试用 `receiver.getComponentTree()` 断言物化出的真实节点树。**这是 JS guest 用的同一条渲染路径，只是批次在 Rust 里构建。**

**Phase C 后续（未做）**：批次 wire 从 JSON 换成现有**二进制 op 协议**（`BinaryProtocol`，与 JS guest 完全一致、高频优化）；输入 / 生命周期走 `keel_on_event(ptr,len)`（复用现有事件通道）；补 host:* 全集；扩 C/Zig SDK。

**消费者（之后）**：canvas 帧缓冲 / `host:gpu`、重计算 app……见 [keel-canvas.zh.md]（下游平台仓侧）。

## 6. 渲染 / 事件

渲染路径（Phase C 首版已通）：原生 guest **发渲染批次** → `keel_send_batch` → host 解码成 `OperationBatch` → 真 `receiver` 物化成 sealed 组件树，**与 JS guest 复用同一 receiver**。批次 wire 首版用 JSON（与 host:* wire 一致），后续换二进制 op 协议——ABI 通道形状不变。

**节点词汇边界（canvas-first，当前）**：原生 guest 能发的**全部**节点就三种——`ui::view`（容器）、`ui::text`（字符串）、`ui::canvas`（视口），而且 `View`/`Text` **不带 props / 样式 / id / 事件回调**。JS guest 的 21 名组件白名单（`src/sdk/sdk.ts`）从原生线**够不着**：原生 guest 挂不了任何可交互 RN 组件，连给 `View`/`Text` 定位、描边都做不到。真正的界面（工具栏、表单、下拉、文本框）要**自绘进一个 `Canvas`**——guest 自己做布局、命中测试、焦点，靠 canvas 的 `pointer` / `wheel` / `key` 事件流（经 `events::on` 投递）驱动。**这是当前能用的形态；把两条线收敛到一套组件模型（props + 事件回传通道）是已记录的 follow-up、不是永久设计**——见 issue #63 与 [native-guest-deferred.zh.md](native-guest-deferred.zh.md)。这条边界现在只有读 `lib.rs` 才发现，此处显式写明，避免作者对着它写完才知道。

**事件（host→guest，已落地）**：guest 导出 `keel_on_event(name_ptr,name_len, payload_ptr,payload_len)`；host 调 `NativeGuestHost.emitEvent(name, payload)`（编码 payload → `keel_alloc` 写入 → 调 `keel_on_event`，guest 无此导出则 no-op；**全程 try/catch，guest 的坏 alloc/trap 不使 host 抛错**）。

**事件 payload 的编码分叉（issue #82）**：payload 走**与 host 调用返回值同一套分叉**——不含字节时是 UTF-8 JSON（与改动前逐字节一致），含 `Uint8Array` 时编成 **KBS1 信封**（`contracts/store-net-bytes.json`），字节以原始段（segment）随行、不做 base64 / 数字数组膨胀。ABI 没变（`keel_on_event` 本来就是 `(ptr,len)` 字节区间），guest handler 拿到的也一直是原始 `&[u8]`：想要字节的 handler 自己 peek magic、`store_net_encode::decode_envelope` 零拷贝解框，不想要的照旧当 JSON 解。**这是原生 guest 唯一的 host→guest 推送通道**：宿主模块的 `subscription` 从原生线**结构性够不着**（订阅要递一个 handler 函数，而这条 ABI 只搬字节），所以外部对端主动送来的数据（转发的 socket、串口、子进程 stdout——guest 无从知道何时该拉）只能走事件。契约层的 `createCapabilitiesManifest` 会把这类导出列进 `jsOnlyCapabilities`，`subscription()` 也可用 `binary: { event: [...] }` 声明事件里的字节字段（派发处有 fail-closed 兜底：声明为字节的字段必须是真 `Uint8Array`，数字数组一律拒）。编码失败（段超 1 MiB 上限、循环引用）**不静默丢**：`console.warn` 报出事件名与原因（同 render 通道的告警上限，防洪）。SDK `events::on(name, |payload| …)` 注册同步 handler + `off(id)` 注销（`dispatch` 先快照 handler，允许 handler 内自删/新增而不 UAF）。这让原生 guest 从「只能画一帧」变**可收输入/生命周期 = 可交互**。**注**：handler 是同步的，暂不应在其中 `.await` host 调用——被 drop 的 future 不仅收不到结果，其结果槽还会在 guest 侧 `RESULTS` 累积（已加 64 上限兜底，避免无界泄漏）；异步 handler 是 follow-up。**canvas 的帧缓冲 present 是消费者层，地基不碰。**

## 7. 测试

`native-guest-host.test.ts`（26 绿）+ `engine.mixed-guests.test.ts`（1 绿）。因这是**信任边界**（host 解引用 guest 控制的指针），覆盖不止 happy-path，还含对抗、fuzz 与不变量：

- **端到端**：手写最小 guest host:store.putText 往返；真 Rust guest `store::put().await`；渲染批次经真 receiver 物化成节点树；**host→guest 事件**（`emitEvent` → guest handler 收到 payload、名字过滤、重复投递、无导出则 no-op）；**C guest**（C SDK 写的 guest 经同一 host **渲染**成功 → ABI 语言中立在 C 上得证。**诚实注**：C SDK 首版只做单向 render 路径，`host:*` 异步往返/事件的语言中立目前**只在 Rust 上验证**，C 侧 executor 是 follow-up）。
- **多 guest 共存**：同一进程挂一个 JS/node-vm guest（Engine）+ 一个原生 guest（NativeGuestHost），各渲染进各自 receiver、互不串（`engine.mixed-guests.test.ts`）。
- **对抗 / 边界**（host 必须 fail-closed、绝不崩）：畸形 JSON 输入 → `ok=0`；**越界 `ptr/len` → `ok=0`、不读 OOB**；畸形渲染批次 → 丢弃不崩；host handler 抛错 → `ok=0`；模块在但方法缺 → `ok=0`。
- **executor 压测**：真 Rust guest 连续两次 `await`（put→get，压 `cb_id` 匹配 + re-poll）；guest `Err` 分支（能力缺失 → `ok=0`）；JSON 转义（值含 `"`/`\`）双向往返。
- **封口不变量（自动化）**：对每个 guest wasm 断言 import ⊆ host 提供集；**+ 正向强制**——一个 import 了未声明 `env.evil` 的 guest 被 `load()`（`WebAssembly.instantiate` → `LinkError`）**拒绝**（这才是真测 seal 强制，非同义反复）。
- **事件对抗**：guest 的 `keel_on_event` **trap** → `emitEvent` 不抛（fail-closed）；handler 在 dispatch 中**自删**（`off(self)`）→ 不 UAF、恰好触发一次。
- **fuzz + 韧性**（seeded PRNG，可复现，无依赖）：`readBytes` 随机 `(ptr,len)` 400 轮 —— 越界必抛、界内必返 `len` 字节；`emitEvent` 随机 JSON payload（含 `"`/`\`/多字节 UTF-8/嵌套）150 轮 —— 双向逐字节往返、guest 不崩；**heap 耗尽 guest**（超额分配触发 alloc 失败）→ **可捕获错误、host 存活**（后续正常 guest 照跑）。

> **真实 bug 都是补对抗/fuzz 测试与独立 review 逼出来的：**
> 1. **重入栈溢出/UB**：错误路径曾同步 resolve，而 `keel_resolve` 重入 guest executor → 重 poll 正在 poll 的 future（不只是栈溢出，是对 guest `static mut TASK` 造 `&mut` 别名 = UB）。修法：`onHostCall` 先让出 microtask，resolve 永不同步重入。
> 2. **自旋挂死**：SDK panic handler 曾是 `loop {}`，guest panic/alloc 失败 → 死循环挂住主线程。修法：panic handler 改 `wasm32::unreachable()` **陷入**（trap），可捕获上抛。
>
> **独立 review 又补修（沙箱边界硬化）**：`emitEvent`/`resolve` 补 try/catch（guest 坏 alloc/trap 不使 host 抛错、不 reject `drain()`）；SDK `events::dispatch` 迭代前**快照 handler**（Box→Rc，消除 handler 内改 `HANDLERS` 的 UAF）；`RESULTS` 加上限（防 drop 的 future 结果槽无界泄漏）；封口测试补**正向拒绝**用例。

**Web Worker 隔离（已落地）**：Web host 可用 `createNativeGuestWorkerEngine` 把整个原生 guest（实例、线性内存、`keel_init`、event/resolve continuation）放进独立 Worker。主线程只保留真实的 `host:*` 实现与 Receiver；能力调用通过结构化消息往返。指针型能力不镜像整块 Wasm heap，而是通过有界的按需 memory RPC 只复制调用实际读取/写入的区间，所以 `canvas.present`、asset/file/media/audio/GPU 等路径需 `await readGuestMemory` / `await writeGuestMemory`。

Worker proxy 对每个 load/event turn 累计 guest 实际运行时间：`keel_init`、event、`keel_resolve` 等同步执行段计入 watchdog，等待文件选择器、网络 broker 等 host capability 的时间不计入。同步 `loop {}` 会由主线程 `terminate()` 硬终止；无穷 host_call→resolve 链也不再占住 UI 主线程，并受累计预算约束。真实 Worker 测试覆盖了渲染、指针读写、host 等待不计费和死循环终止。

**仍保留的直接模式**：`createNativeGuestEngine` 为非浏览器/测试/兼容场景继续提供同线程实现；选择它就仍须由集成方保证 guest 会返回。Worker 内的 `WebAssembly.Exports` 不能跨 structured clone，因此 worker engine 的 `exports` 只返回 `undefined`；诊断应改走命名事件或 host capability。打包器若不处理库内 `new URL(...)`，可通过 `createWorker` 注入内容哈希后的同源 Worker 入口。

**仍缺（follow-up）**：更广的 fuzz（随机 op 流/畸形 props）、runaway-guest 的 Worker 化中断、接平台后 `host:store` 活桥 e2e。

**defer 项设计留档**：二进制 wire / CLI `.wasm` 目标 / typed 包装模式 / QuickJS 统一 —— 见 [native-guest-deferred.zh.md](native-guest-deferred.zh.md)。

## 8. 源码索引

| 关注点 | 位置 |
|---|---|
| 宿主侧原生 guest ABI | `src/host/native-guest/native-guest-host.ts`（`NativeGuestHost`） |
| Rust guest SDK（future/executor/宏 + `store`/`ui`/`render`/`events`） | `crates/keel-native-guest/src/lib.rs` |
| 真 Rust guest：host:* 调用 / 连续 await / 渲染 / 收事件 | `crates/{kv,seq,ui,event,heap-churn}-guest/src/lib.rs`、`crates/build.sh` |
| C guest SDK + 例子（语言中立自证） | `sdk/c/keel_guest.h`、`sdk/c/example-guest.c`、`sdk/c/build.sh`（clang -c → wasm-ld 两步） |
| JS + 原生 guest 共存测试 | `src/host/__tests__/unit/engine.mixed-guests.test.ts` |
| 最小原生 guest（wire 级 ABI，手写） | `src/host/native-guest/__tests__/fixtures/roundtrip.wat` |
| 往返 + 封口负向 + Rust guest + 渲染物化测试 | `src/host/native-guest/__tests__/native-guest-host.test.ts` |
| 渲染批次物化（复用） | `src/host/receiver/receiver.ts`（`applyBatch`）、`src/host/registry.ts` |
| 复用的能力派发 | `src/contract/index.ts`（`createHostModuleDispatch`） |
| 异步模型来源（现有 QuickJS 桥） | `src/host/sandbox/providers/quickjs-native-wasm-provider.ts` |
| JS guest 装载（本路径平行物） | `src/host/engine/engine.ts`（`loadBundle`/`evalCode`） |

---

## 9. ABI 版本 / props 投递 / wire 泄漏修复（本轮硬化）

本节记录三处对既有语义的增量改动。三者都是**加法**：现网平台调用不受影响。

### 9.1 可选导出 `keel_abi_version() -> u32`

guest 现在**可以**（不强制）导出 `keel_abi_version`，向宿主声明自己所讲的 ABI 版本。Rust SDK 的 `guest_main!` 宏会自动生成该导出，值取自常量 `KEEL_ABI_VERSION`（当前 = 1）。

宿主端（`NativeGuestHost.load`）的处置规则：

- **缺席即宽容**：guest 没有这个导出 = 版本化之前的 guest（v0/v1 wire 完全一致），照常加载，`guestAbiVersion` 记为 `null`。
- **不识别即拒载（fail-closed）**：guest 声明了宿主不支持的版本号（`SUPPORTED_GUEST_ABI_VERSIONS` 之外），`load` 直接抛错，**不会**执行 `keel_init`。这是一处**有意的行为变更**——此前宿主完全无视版本导出。
- 版本闸门排在 `instantiate` 之后、`keel_init` 之前；探针函数自身若 trap，任其上抛（等同 instantiate 失败的 fail-closed 姿态）。

只在**破坏性**的 wire/导出变更时才 bump 这个号；加法式变更保持不变。

### 9.2 `loadBundle` 的 `initialProps` 经命名事件 `props` 投递

`NativeGuestView.loadBundle(source, initialProps, options)` 中，`initialProps` 现在会在 `load` 之后、`drain` 之前，通过命名事件通道投递给 guest（`events::on("props")`）——与 JS Engine 的 props 语义对齐。`source`/`options` 对原生 guest 无意义（wasm 字节在构造期已定），有意不使用。

投递排在 drain 之前，是为了让「从 props 渲染」的 guest 在其第一次稳定状态里就能看到初始 props。现网 `useEngineView` 对 wasm 分支传的是 `initialProps: undefined`，故生产环境零行为差异；此处记录是为了避免将来再发明第二套 props 通道。

### 9.3 wire arena 逐 turn 回收 + talc 全局分配器（泄漏修复）

原生 guest 有两处结构性堆泄漏，本轮各以对应手段钉死：

- **host 写入侧（大头）**：宿主每次 `keel_resolve` 结果、每个事件的 name+payload 都经 `keel_alloc` 从 bump 堆拿内存且**永不回收**——60fps 长时 guest 会结构性堆耗尽。修法：SDK 内新增一块 64 KiB 的 **wire arena**，`keel_alloc` 优先从其分配；在三个 host→guest 入口（`rt::init` / `rt::resolve` / `events::dispatch`）用 `begin_wire_turn` / `end_wire_turn` 括起来，**最外层 turn 关闭时整块回收**。安全不变量（承重）：宿主写入的 wire 缓冲，在「投递它的那次 host→guest 入口返回前」必被完全消费——`rt::resolve` 在入口 `to_vec` 拷走，`events::dispatch` 只在调用栈内借用 `&[u8]`，绝不跨 turn 存活。`TURN_DEPTH` 计数守护了嵌套入口（如 onLog/onRenderBatch 同步回调里再 emitEvent）的场景：内层 turn 结束**不**回收，只有最外层结束才回收。超出 arena 的超大请求退回全局堆（talc，见下），由其回收，不再泄漏。**若将来有人让 dispatch 把 payload 借用泄出调用栈，此不变量即破**——`lib.rs` 注释已就此告警。

- **guest 拷贝侧**：全局分配器已从旧的**固定 1 MiB bump 堆**换成 **talc**（`talc::wasm::WasmDynamicTalc`，`memory.grow` 驱动、可增长、带真自由链）。旧 bump 堆只能 LIFO 回滚（交错生命周期照旧泄漏、超 1 MiB 直接 trap）；talc 按任意顺序回收已释放块，堆随工作集增长后**趋于平台**。这替代了此前的 partial mitigation，是 R3 收口。

回归闸：① `event-guest` 的零分配 `tick` 处理器 + `tick_count()` 导出，TS 连发 8000×512B 事件（wire 总量 ~4.1 MiB ≫ 64 KiB wire arena），断言线性内存保持有界——只有 arena 逐 turn 回收才能通过。② `heap-churn-guest` 的 `heap_churn` 导出跑数百 MiB 的 alloc/free churn（FIFO 释放、非 LIFO），断言 `memory.buffer.byteLength` 暖机后**平台化**（起 1216 KiB → 平台 1856 KiB，此后 336 MiB churn 零增长），证明 talc 复用已释放帧。

---

**一句话**：② = 把 QuickJS 早就在用的那条底层 host↔WASM 桥，**抽成语言中立的线性内存 ABI**，供原生 `.wasm` 直接绑；因**派发/broker/渲染/信任全共享、只多一个 guest 侧适配器**，所以**加法式兼容、不动现有 QuickJS**。Phase A/B/C 已用真 Rust `.wasm` guest 把「装载 + host:* 往返 + 封口 + 渲染出 UI」端到端跑通。

## 10. wasm SIMD128 —— native guest 的向量化构建路径

像素级重计算（图像滤镜、合成）只在 **native guest 线**（Rust→wasm）才谈得上向量化——`runtime:"keel"` 的 JS/QuickJS 线是解释执行、无处上 SIMD（详见 issue #57 / #58）。本节给出这条线上 wasm SIMD128 的**构建路径**与一处基线示例。

### 10.1 构建路径：按函数 `#[target_feature]`，无需全局 flag

首选做法——SIMD 函数上标 `#[target_feature(enable = "simd128")]`，函数体内用 `core::arch::wasm32` 的 v128 内联函数（`v128_load32_zero` / `u16x8_extend_low_u8x16` / `i32x4_add` 等）：

- **无需 nightly、无需 `-Z build-std`**：固定工具链 `rustc 1.93.0`（远超 SIMD 稳定的 1.54）即可。
- **无需全局 `-C target-feature=+simd128`**：SIMD 按函数开，`crates/build.sh` 与 CI 的 flagless `cargo build --workspace` / `clippy` 都照常编到真 SIMD 路径。
- **现存 fixture 零漂移**：不动全局 codegen，`__tests__/fixtures/*.wasm` 逐字节可复现性不受影响（只新增该 guest 自己的 fixture）。

备选做法——若要让**整个 crate**都自动向量化，可给该 crate 的构建单独加全局 `-C target-feature=+simd128`（仿 `build.sh` 里 `canvas-binary-guest` 的独立 cargo 调用拆分）。代价：该 crate 全量 codegen 变化，需**同一改动内重建并重提交**其 fixture，且要覆盖 CI 无 RUSTFLAGS 的 build/clippy（`.cargo/config.toml` 只对无 env-RUSTFLAGS 的调用生效，`build.sh` 里的 env 会盖过它——两者不合并）。多数场景优先按函数开。

### 10.2 封口（seal）不变——SIMD 是 opcode，不是 import

host-import 白名单（`env.{keel_host_call, keel_send_batch, keel_log, keel_on_event}`，在 `native-guest-host.test.ts` 自动断言）对 SIMD **透明**：v128 是模块内的**指令**，不引入任何 import。实测：真发射了 v128 指令的 guest，import 集合与非 SIMD 基线逐字节一致。**唯一会破封口的是 atomics/threads**——共享内存会引入 `env.memory` import，与 SIMD 无关，切勿顺手一起开。

### 10.3 逐字节正确性：标量与 SIMD 除法必须同法

正确性验收要求 SIMD 输出与标量参照**逐字节一致**。盒式模糊的窗口和除以窗口计数时，**两条路径都用标量整数除法 `sum/count`**——避开「乘倒数」在取整上与整数除法差 1 的风险。SIMD 只把 4 个 RGBA 通道的窗口累加向量化（`i32x4`），最后逐 lane 取出、走同一套标量除法，故结果按位相同。

### 10.4 基线示例 + 门禁 + bench

- `crates/blur-demo-guest/`：一个盒式模糊 demo guest，导出 `box_blur_scalar` 与 `box_blur_simd`（+ `blur_alloc`）。
- `src/host/native-guest/__tests__/blur-demo-guest.test.ts`：**逐字节门禁**——多种尺寸/半径下 `simd === scalar`；随 `bun run preflight` 跑。
- `src/host/native-guest/__benchmarks__/blur-bench.ts`：门控 bench（`bun <file>` 显式跑，非门禁）。

**实测提速（同内核、引擎相关）**：V8（node/浏览器/web host）约 **2.07×**，JSC（bun 测试运行时）约 **1.11×**——差异纯来自引擎的 wasm-SIMD 代码生成质量。web host 跑 V8，故 ~2× 是相关数字；bun 下看到的 ~1.1× 低估了真实收益。正确性与引擎无关。

**scope**：具体滤镜（盒式模糊）属 app/demo 侧，框架只提供这条构建路径（+ 可选通用 helper），不做滤镜库。改任何 guest 后须 `crates/build.sh` 重建并提交其 fixture（CI 不重建）。
