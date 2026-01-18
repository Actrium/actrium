# Rill 沙箱方案对比（最终版）

## 📊 5 种核心 Provider

Rill 只提供 **5 种经过验证的沙箱方案**，全部支持**方案 B**（高性能直接对象传递）：

| Provider | 平台 | 隔离性 | 传递能力 | 体积 | 性能 | 推荐度 |
|----------|------|--------|---------|------|------|--------|
| **JSC Native** | iOS/macOS | Full | Full | 0 KB | Excellent | ⭐⭐⭐⭐⭐ |
| **Hermes Native** ✨ | RN (Hermes) | Full | Full | 0 KB | Excellent | ⭐⭐⭐⭐⭐ |
| **QuickJS Native** | RN 全平台 | Full | Full | ~200 KB | Very Good | ⭐⭐⭐⭐⭐ |
| **QuickJS Native WASM** | Web | Strong | Full | ~300 KB | Very Good | ⭐⭐⭐⭐⭐ |
| **Node VM** | Node/Bun | Strong | Full | 0 KB | Excellent | ⭐⭐⭐⭐⭐ |

**核心特性**：
- ✅ 所有方案都支持传递函数、循环引用、复杂对象
- ✅ 零 JSON 序列化开销
- ✅ 真正的隔离（进程/内存/WASM 边界）
- ✅ 无降级、无 fallback、无中间状态

---

## 1️⃣ JSC Native

**特点**：
- 平台：iOS、macOS、tvOS、visionOS
- 技术：系统 JSC + JSI
- 体积：0 KB（系统内置）

**优势**：
- ✅ 零体积
- ✅ 性能最佳
- ✅ 完整的 JSI（可传递任意对象）

**使用**：
```typescript
import { JSCProvider } from 'rill/sandbox/native';

const engine = new Engine({
  provider: new JSCProvider({ timeout: 5000 })
});
```

---

## 2️⃣ Hermes Native ✨

**特点**：
- 平台：React Native with Hermes runtime
- 技术：Hermes + JSI（隔离的 Hermes 运行时）
- 体积：0 KB（复用应用的 Hermes 引擎）

**优势**：
- ✅ 零额外体积（复用应用的 Hermes）
- ✅ 性能优秀
- ✅ 完整的 JSI（可传递任意对象）
- ✅ **字节码预编译支持**（`evalBytecode`）

**独有特性 - 字节码预编译**：

Hermes 支持 AOT（Ahead-of-Time）编译。使用 `hermesc` 将 JS 预编译为字节码：

```bash
# 将 JS 编译为 Hermes 字节码
hermesc -emit-binary -O -out guest.hbc guest.js
```

```typescript
// 加载并执行预编译字节码
const bytecode = await fetch('guest.hbc').then(r => r.arrayBuffer());

if (context.evalBytecode) {
  context.evalBytecode(bytecode);  // 跳过解析/编译
}
```

| 方法 | 解析 | 编译 | 执行 |
|------|------|------|------|
| `eval(source)` | ✓ | ✓ | ✓ |
| `evalBytecode(hbc)` | - | - | ✓ |

**使用**：
```typescript
import { HermesProvider } from 'rill/sandbox/native';

const engine = new Engine({
  provider: new HermesProvider({ timeout: 5000 })
});
```

**注意**：需要在原生构建时设置 `RILL_SANDBOX_ENGINE=hermes`。

---

## 3️⃣ QuickJS Native

**特点**：
- 平台：iOS、Android、macOS、Windows
- 技术：QuickJS + JSI
- 体积：~200 KB

**优势**：
- ✅ 真正的跨平台
- ✅ 完整的 JSI
- ✅ 体积小

**使用**：
```typescript
import { QuickJSProvider } from 'rill/sandbox/native';

const engine = new Engine({
  provider: new QuickJSProvider({ timeout: 5000 })
});
```

---

## 4️⃣ QuickJS Native WASM

**特点**：
- 平台：Web（浏览器）
- 技术：将 `rill/native/quickjs` 编译为 WASM
- 体积：~300 KB (gzipped)

**关键优势**：
- ✅ **代码复用**：Web 和 RN 用同一套 C++ 代码
- ✅ **真正的 JSI**：不是 wrapper
- ✅ **完整的传递能力**：函数、循环引用、复杂对象
- ✅ **统一架构**：减少维护成本

**使用**：
```typescript
import { QuickJSNativeWASMProvider } from 'rill/sandbox/web';

const engine = new Engine({
  provider: new QuickJSNativeWASMProvider({
    wasmPath: '/assets/quickjs_sandbox.wasm'
  })
});
```

**构建**：
```bash
# 一次性设置
cd rill/native/quickjs
./build-wasm.sh release

# 输出：
# → rill/src/sandbox/wasm/quickjs_sandbox.{js,wasm}
```

---

## 5️⃣ Node VM

**特点**：
- 平台：Node.js、Bun
- 技术：Node.js `vm` 模块
- 体积：0 KB

**优势**：
- ✅ 零体积
- ✅ 性能最佳（共享内存）
- ✅ 完整的传递能力
- ✅ 硬中断 timeout

**使用**：
```typescript
import { VMProvider } from 'rill/sandbox';

const engine = new Engine({
  provider: new VMProvider({ timeout: 5000 })
});
```

---

## 🎯 推荐方案（按场景）

### React Native 应用

```typescript
// iOS/macOS
import { JSCProvider } from 'rill/sandbox/native';
const provider = new JSCProvider();

// Android/跨平台
import { QuickJSProvider } from 'rill/sandbox/native';
const provider = new QuickJSProvider();
```

### Web 端小程序平台

```typescript
import { QuickJSNativeWASMProvider } from 'rill/sandbox/web';

const provider = new QuickJSNativeWASMProvider({
  wasmPath: '/assets/quickjs_sandbox.wasm'
});
```

**优势**：
- ✅ 与 RN 用同一套 C++ 代码
- ✅ 可传递函数、事件对象
- ✅ 强隔离（WASM）

### Node.js 构建工具

```typescript
import { VMProvider } from 'rill/sandbox';

const provider = new VMProvider({ timeout: 5000 });
```

---

## 📈 性能对比

### 初始化时间

| Provider | 首次加载 | 后续加载 |
|----------|---------|---------|
| JSC Native | < 1ms | < 1ms |
| Hermes Native | < 1ms | < 1ms |
| QuickJS Native | < 5ms | < 1ms |
| QuickJS WASM | ~80ms | ~10ms (缓存) |
| Node VM | < 1ms | < 1ms |

### 函数调用开销（每次）

| Provider | Host → Guest | Guest → Host |
|----------|-------------|-------------|
| JSC Native | < 0.01ms | < 0.01ms |
| Hermes Native | < 0.01ms | < 0.01ms |
| QuickJS Native | < 0.02ms | < 0.02ms |
| QuickJS WASM | ~0.05ms | ~0.05ms |
| Node VM | < 0.01ms | < 0.01ms |

### 内存占用

| Provider | 基础占用 | 每个实例 |
|----------|---------|---------|
| JSC Native | 0 MB | ~2 MB |
| Hermes Native | 0 MB | ~2 MB |
| QuickJS Native | ~5 MB | ~3 MB |
| QuickJS WASM | ~5 MB | ~3 MB |
| Node VM | 0 MB | ~2 MB |

---

## 🔧 DefaultProvider 策略

```typescript
import { DefaultProvider, SandboxType } from 'rill/sandbox';

// 自动选择最佳 Provider
const provider = DefaultProvider.create({
  timeout: 5000,
  wasmPath: '/assets/quickjs_sandbox.wasm' // Web 专用
});

// 或强制指定沙箱类型
const hermesProvider = DefaultProvider.create({
  sandbox: SandboxType.Hermes
});

/**
 * 自动选择逻辑（React Native）：
 * 1. HermesProvider（当 RILL_SANDBOX_ENGINE=hermes）
 * 2. JSCProvider（Apple 平台）
 * 3. QuickJSProvider（全平台）
 *
 * 其他平台：
 * - Node/Bun → VMProvider
 * - Web → QuickJSNativeWASMProvider
 */
```

---

## 总结

### ✅ 设计原则

1. **简单**：只有 5 种方案，无降级、无 fallback
2. **安全**：全部强隔离，可用或不可用，无中间状态
3. **高性能**：全部支持方案 B（直接传递对象）
4. **统一**：Web 和 RN 用同一套 C++ 代码（QuickJS WASM）

### 🎯 核心优势

- ✅ 解决了"按钮点击无效"的根本问题（循环引用）
- ✅ 可以传递函数、事件对象、复杂类型
- ✅ 零 JSON 序列化开销
- ✅ 架构简单，维护成本低

### 📍 不再提供的方案

以下方案不符合设计原则（无法传递函数和循环引用），不再作为推荐方案：

- ❌ **Proxy Sandbox**：弱隔离，有逃逸风险
- ❌ **quickjs-emscripten**：第三方 wrapper，维护成本高

**理由**：Rill 只提供**安全且高性能**的方案，不做妥协。
