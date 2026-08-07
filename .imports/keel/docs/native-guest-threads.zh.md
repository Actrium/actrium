# keel native guest 多线程计算 —— approach B（独立实例 worker 池）

> 状态：**通用 host 侧计算池已落地**。任意 native guest 可按固定 ABI 导出纯同步计算函数，
> host 用互相独立的 worker/wasm 实例并行执行并以 transferable buffer 搬运输入输出。旧的 blur
> 切片池仍作为瓦片/halo/缝合的算法示例。两条路径都**不改 seal、不用共享内存，也不增加
> guest 的 host import**。应用平台如何把它声明成 `host:compute` 能力不属于 keel 的 contract。

## 0. 定位与边界

大图像素计算（几千像素边长的卷积/模糊/重采样）的性能天花板有两块：缺 SIMD（已解，
~2× on V8）和缺多线程。本文谈多线程，走 **approach B**：

- **是什么**：一个 **worker 池**，每个 worker 承载一个**完全独立、非共享**的 wasm guest
  实例（各自的线性内存、各自的 talc 分配器），通过**可转移的 `ArrayBuffer` 瓦片**
  （transferable，零拷贝移交）把大图切块分发下去、算完移交回来。
- **明确不碰**：SharedArrayBuffer、`WebAssembly.Memory({shared:true})`、atomics/
  bulk-memory、COOP/COEP 响应头、import-memory 的 seal/ABI 改动——**统统不需要**。

## 1. 为什么是 B 不是 A

| | approach B（本文） | approach A（真 wasm 线程） |
|---|---|---|
| 内存 | 每 worker 独立线性内存 | 一块 `shared:true` 共享内存 |
| 需要 | transferable ArrayBuffer（浏览器基线能力） | atomics+bulk-memory + COOP/COEP 头 + import-memory |
| seal | **不变**（各 guest 仍只 import 那 4 个函数） | **破**（共享内存 = `env.memory` import，见第 10.2 节封口规则） |
| 平台成本 | 低（复用现有 worker 基建） | 高（改 seal/ABI + 部署侧加跨源隔离头） |

结论：**先做 B**。A 的唯一优势是省掉瓦片的移交开销；只有当 profiling 证明**移交
开销成为瓶颈**时才值得付 A 的代价。在那之前 A 不做。

## 2. 架构

```
主线程 (host)
  │  应用切图：源数据 → N 个工作块（卷积类算法自行附带 halo）
  │  pool.run({ exportName, input, outputCapacity, args })
  │  postMessage(input.buffer, [input.buffer])
  ▼
worker[0..N)   每个 = 同一 guest .wasm 的独立实例 + 私有线性内存
  │  guest.keel_compute_<name>(src_ptr, src_len, dst_ptr, dst_cap, ...args)
  ▼  postMessage(result.buffer, [result.buffer])
主线程
     应用接收结果并缝合/聚合
```

计算 worker 不启动 `keel_init`，也不装配 `NativeGuestHost` 的能力派发；它只实例化模块、
查找命名受限的纯计算导出并在 host 管理的 scratch 区同步调用。计算实例因此不能借这条路径
调用 `host:*`，也不会与主 guest 共享状态。

## 3. 关键设计点

### 3.1 瓦片切分 + halo（模糊的唯一算法坑）

盒式模糊每个输出像素依赖周围 ±r 的邻域。若直接把图切成不重叠的瓦片各自模糊，**缝边
会错**（瓦片边缘取不到邻块像素）。做法：每个瓦片向四周**多带 r 像素的 halo**（重叠
带），worker 对整块（含 halo）模糊，但**只写回内部非 halo 区域**。halo 宽度 = 半径 r。
正确性据此可与单线程逐字节对齐。

### 3.2 池大小 / cores 暴露

池上限取 `min(navigator.hardwareConcurrency, 8..16)`。但 **sealed guest 没有
`navigator`**，两条路：

- **host 管池**（approach B 天然如此）：worker 编排在 host 侧，guest 只管算一块，压根
  不需要知道核数。**首选**。
- 或**加法式在 `host:canvas.getInfo` 上补一个 `cores` 字段**——`getInfo` 已是能力探针
  （`{ binaryDraw, wireVersion }`，老 host 不实现即回落），加 `cores` 与既有模式一致。

### 3.3 fail-closed 上限（bomb-wall）

仿 present 的**逐 canvas 字节预算**（超预算即 fail-closed 那套）：给瓦片**大小/数量**
加上限，越界直接拒绝（fail-closed），防止「一图切成百万瓦片」打爆 worker 池/内存。
上限常量与既有 present byte-budget 并列管理。

### 3.4 正确性门禁

- **多 worker === 单线程**：同一源图，N-worker 瓦片路径的输出与单实例整图路径**逐字节
  相等**（halo/缝合无缝的证明）。
- 每瓦片内 **SIMD === scalar** 仍成立（已由 `blur-demo-guest.test.ts` 保证，逐瓦片复用）。

## 4. 复用现有基建

web host 已有 off-main-thread 的完整 guest 承载层，但纯计算池刻意不用它：完整承载层会启动
guest 并开放能力派发，而计算池只允许一组命名受限、同步、无副作用的导出。两者复用的是
浏览器 Worker/transferable 基建，不复用生命周期或 capability dispatch。

## 5. web-first；native 宿主的差异

approach B 的「Web Worker 池」是 **web 专属**。native 宿主（Apple/RN 等）没有 Web
Worker，等价物是「OS 线程各承载一个独立 VM 实例 + 线程间传 buffer」，属**宿主特定、更
大**的工作，本预研不覆盖。故 B 先在 web host 落地。

## 6. 边界 / 未决

- keel 提供的是 host 侧 `NativeGuestComputePool`，不是 guest 可直接调用的 `host:compute`
  contract。应用平台若要开放后者，必须自行声明 capability、限制可调用导出并接入授权/撤销。
- 未决：瓦片调度粒度（固定瓦片 vs 按核数自适应）、halo 冗余计算与移交开销的平衡点
  由调用方按算法选择；通用池不假装理解图像边界。
- native 宿主的 OS 线程实现、approach A（共享内存真 wasm 线程）仍未做。

## 7. 预期收益（来自 #57 的 spike 实测）

worker 池近线性：8 worker ~9×、16 worker ~12×（16 核，超线程税使 16× 打折到 12×）。
与「算法（滑窗）+ SIMD」叠加：**1.2s 的 naive 模糊 → ~40ms（约 30×，16 worker）**，逼近
实时。这也是「先 B」的依据——不动 seal、不改部署，就拿到主要收益。

## 8. 已落地

### 8.1 通用 native compute pool

- **`native-compute-pool.ts`** —— `NativeGuestComputePool`，也可从
  `keel/host/native-compute` 导入。默认池大小 `min(hardwareConcurrency, 8)`、硬上限 16；每个
  输入/输出默认上限 64 MiB、数字参数最多 8 个、等待加运行任务最多 64 个、单任务 wall
  timeout 默认 5 秒。超时会终止并替换该 worker。
- **`native-compute-worker.ts`** —— 每个 worker 实例化一份私有 wasm，不运行 `keel_init`，
  不提供 host capability dispatch。导出 ABI 固定为
  `keel_compute_<name>(src_ptr, src_len, dst_ptr, dst_cap, ...i32_args) -> out_len`。导出必须同步、
  不得分配/持有 host scratch 指针，也不得依赖主 guest 的可变状态。需要自行打包 worker 的
  平台可从 `keel/host/native-compute-worker` 导入这个独立入口，并把生成的 URL 传给池。
- 输入先复制再 transferable 移交，因此不会 detach 调用方的 `Uint8Array`；输出只复制导出
  返回的实际长度，并移交独立 `ArrayBuffer`。共享内存、atomics、COOP/COEP 均不需要。
- 池只负责单个工作块的排队、并行、上限和中止。切片、halo、缝合及逐块重试由调用方负责。

`__tests__/native-compute-pool.test.ts` 用实际 worker 同时跑三份
`keel_compute_box_blur`，并与主实例 SIMD 结果逐字节比较；还覆盖输入上限、非法/缺失导出、
队列上限和 wall timeout。

### 8.2 blur 专用参考实现

host 侧 worker 池已实现，全在 `src/host/native-guest/`，复用 `blur-demo-guest.wasm`，**未改
seal / guest / 任何 wire·contract**：

- **`blur-pool.ts`** —— 主线程 `BlurPool` 类：把整图切成**全宽行条**、每条带上下各 `radius`
  行 halo（clamp 到真图边界），把每条的像素拷进一块独立 `ArrayBuffer` **transferable
  移交**给一个 worker，收回后只缝合内部行。fail-closed 维度/半径守卫（`maxPixels`/
  `maxRadius`，派发前检查，仿 host:asset「先卡维度再动 raster」）。池大小是构造参数
  （下游读 `navigator.hardwareConcurrency`），guest 不感知核数。
- **`blur-pool-worker.ts`** —— worker 入口：一个独立 `NativeGuestHost` + blur guest（各自私有
  线性内存，无共享内存/atomics），`writeBytes → box_blur_simd → readBytes` 后把结果
  transferable 移交回来;scratch 缓冲按最大瓦片惰性增长(blur_alloc 泄漏,故复用不重分配)。
- **`blur-pool-protocol.ts`** —— 三进三出的极简消息协议(init/blur/destroy ↔ ready/result/error)。

**逐字节门禁**：`__tests__/blur-worker-pool.test.ts` —— N-worker 拼接 === 单实例整图，逐
字节相等,覆盖不均切分、height<workers、radius>条高、细条、radius=0,外加两条 fail-closed
上限用例。随 `bun run preflight` 跑。

**提速 bench**：`__benchmarks__/blur-worker-pool-bench.ts`（`bun <file>` 显式跑，非门禁）。
1024×1024 r=4、bun/JSC、16 核实测：1w=1.00×、2w=1.44×、4w=2.27×、8w=2.76×。次线性来自
每次 blur 的主线程切条/拷贝/缝合串行段 + JSC worker 开销；V8 上更好（引擎 SIMD ~2× vs JSC
~1.1×）。

**仍未做（follow-up）**：更细瓦片 + 拷贝/计算重叠（削减主线程串行段、提升并行度）;
把源像素路径接到真实 `host:asset`/`present`（当前 demo 由调用方传入整块 `Uint8Array`）;
contract 级的瓦片 bomb-wall（本切片是 host 侧运行时守卫，语义上更靠下游 present 强制那侧,
若要上升为 wire cap 需 canvas-wire.json + build.rs codegen + 一致性/golden，属更大改动);
approach A（真 wasm 线程 + 共享内存），仅当瓦片移交开销成瓶颈时才做。
