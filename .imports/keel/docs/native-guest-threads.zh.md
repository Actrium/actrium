# rill native guest 多线程计算 —— approach B 设计草稿（预研，未实现）

> 状态：**设计草稿 / 预研**。本文只画方案与边界，**不含实现**，也未落任何 host API
> 或 seal 改动。对应 issue #57 验收的「线程」那半；SIMD 那半已单独落地（见
> `docs/native-guest.zh.md` 第 10 节 + `crates/blur-demo-guest`）。

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
  │  切图：源 framebuffer → N 个瓦片(带 halo)
  │  postMessage(tile.buffer, [tile.buffer])   // transferable，零拷贝
  ▼
worker[0..N)   每个 = 一个独立 WasmGuestHost + 一份 blur guest .wasm
  │  guest.box_blur_simd(tilePtr, w, h, r)      // 各自线性内存内跑，互不可见
  ▼  postMessage(result.buffer, [result.buffer])
主线程
     缝合：各瓦片内部区域写回目标 framebuffer → host:canvas.present
```

现状对照：`WasmGuestHost`（`src/host/wasm-guest/wasm-guest-host.ts`）今天只实例化
**一个** guest。approach B = 在 worker 里跑 N 个这样的实例 + 一层「瓦片工作 API」。

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

web host 已有 off-main-thread 的 engine-in-worker 与派发/协议层：
`src/host/web/worker/{worker-engine,worker-host,worker-dispatch,protocol,turn-gate}.ts`。
approach B 的 worker 承载与消息协议应**扩展这套**，而非另起炉灶——池化与瓦片消息是
在其上加的一层「compute 工作」协议。

## 5. web-first；native 宿主的差异

approach B 的「Web Worker 池」是 **web 专属**。native 宿主（Apple/RN 等）没有 Web
Worker，等价物是「OS 线程各承载一个独立 VM 实例 + 线程间传 buffer」，属**宿主特定、更
大**的工作，本预研不覆盖。故 B 先在 web host 落地。

## 6. 不做 / 未决

- 本预研**不落实现**、不改 seal、不加 host API。
- 未决：瓦片工作 API 归属——挂在 `host:canvas` 下，还是新开一个 `host:compute` 模块？
  （倾向后者：计算与画布展示是两件事。）
- 未决：瓦片调度粒度（固定瓦片 vs 按核数自适应）、halo 冗余计算与移交开销的平衡点
  （决定何时才值得转 approach A）。

## 7. 预期收益（来自 #57 的 spike 实测）

worker 池近线性：8 worker ~9×、16 worker ~12×（16 核，超线程税使 16× 打折到 12×）。
与「算法（滑窗）+ SIMD」叠加：**1.2s 的 naive 模糊 → ~40ms（约 30×，16 worker）**，逼近
实时。这也是「先 B」的依据——不动 seal、不改部署，就拿到主要收益。
