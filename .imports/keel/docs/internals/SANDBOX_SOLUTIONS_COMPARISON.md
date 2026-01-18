# Rill Sandbox Solutions Comparison (Final)

## 5 Core Providers

Rill provides **5 verified sandbox solutions**, all supporting **Scheme B** (high-performance direct object passing):

| Provider | Platform | Isolation | Transfer Capability | Size | Performance | Recommendation |
|----------|----------|-----------|---------------------|------|-------------|----------------|
| **JSC Native** | iOS/macOS | Full | Full | 0 KB | Excellent | ⭐⭐⭐⭐⭐ |
| **Hermes Native** ✨ | RN (Hermes) | Full | Full | 0 KB | Excellent | ⭐⭐⭐⭐⭐ |
| **QuickJS Native** | RN All Platforms | Full | Full | ~200 KB | Very Good | ⭐⭐⭐⭐⭐ |
| **QuickJS Native WASM** | Web | Strong | Full | ~300 KB | Very Good | ⭐⭐⭐⭐⭐ |
| **Node VM** | Node/Bun | Strong | Full | 0 KB | Excellent | ⭐⭐⭐⭐⭐ |

**Core features**:
- ✅ All solutions support passing functions, circular references, complex objects
- ✅ Zero JSON serialization overhead
- ✅ True isolation (process/memory/WASM boundary)
- ✅ No degradation, no fallback, no intermediate states

---

## 1️⃣ JSC Native

**Characteristics**:
- Platform: iOS, macOS, tvOS, visionOS
- Technology: System JSC + JSI
- Size: 0 KB (system built-in)

**Advantages**:
- ✅ Zero size
- ✅ Best performance
- ✅ Complete JSI (can pass any object)

**Usage**:
```typescript
import { JSCProvider } from 'rill/sandbox/native';

const engine = new Engine({
  provider: new JSCProvider({ timeout: 5000 })
});
```

---

## 2️⃣ Hermes Native ✨

**Characteristics**:
- Platform: React Native with Hermes runtime
- Technology: Hermes + JSI (isolated Hermes runtime)
- Size: 0 KB (uses app's Hermes engine)

**Advantages**:
- ✅ Zero additional size (reuses app's Hermes)
- ✅ Excellent performance
- ✅ Complete JSI (can pass any object)
- ✅ **Bytecode precompilation support** (`evalBytecode`)

**Unique Feature - Bytecode Precompilation**:

Hermes supports AOT (Ahead-of-Time) compilation. Use `hermesc` to precompile JS to bytecode:

```bash
# Compile JS to Hermes bytecode
hermesc -emit-binary -O -out guest.hbc guest.js
```

```typescript
// Load and execute precompiled bytecode
const bytecode = await fetch('guest.hbc').then(r => r.arrayBuffer());

if (context.evalBytecode) {
  context.evalBytecode(bytecode);  // Skips parsing/compilation
}
```

| Method | Parse | Compile | Execute |
|--------|-------|---------|---------|
| `eval(source)` | ✓ | ✓ | ✓ |
| `evalBytecode(hbc)` | - | - | ✓ |

**Usage**:
```typescript
import { HermesProvider } from 'rill/sandbox/native';

const engine = new Engine({
  provider: new HermesProvider({ timeout: 5000 })
});
```

**Note**: Requires `RILL_SANDBOX_ENGINE=hermes` during native build.

---

## 3️⃣ QuickJS Native

**Characteristics**:
- Platform: iOS, Android, macOS, Windows
- Technology: QuickJS + JSI
- Size: ~200 KB

**Advantages**:
- ✅ True cross-platform
- ✅ Complete JSI
- ✅ Small size

**Usage**:
```typescript
import { QuickJSProvider } from 'rill/sandbox/native';

const engine = new Engine({
  provider: new QuickJSProvider({ timeout: 5000 })
});
```

---

## 4️⃣ QuickJS Native WASM

**Characteristics**:
- Platform: Web (browser)
- Technology: Compile `rill/native/quickjs` to WASM
- Size: ~300 KB (gzipped)

**Key Advantages**:
- ✅ **Code reuse**: Web and RN use the same C++ code
- ✅ **True JSI**: Not a wrapper
- ✅ **Complete transfer capability**: Functions, circular references, complex objects
- ✅ **Unified architecture**: Reduced maintenance cost

**Usage**:
```typescript
import { QuickJSNativeWASMProvider } from 'rill/sandbox/web';

const engine = new Engine({
  provider: new QuickJSNativeWASMProvider({
    wasmPath: '/assets/quickjs_sandbox.wasm'
  })
});
```

**Build**:
```bash
# One-time setup
cd rill/native/quickjs
./build-wasm.sh release

# Output:
# → rill/src/sandbox/wasm/quickjs_sandbox.{js,wasm}
```

---

## 5️⃣ Node VM

**Characteristics**:
- Platform: Node.js, Bun
- Technology: Node.js `vm` module
- Size: 0 KB

**Advantages**:
- ✅ Zero size
- ✅ Best performance (shared memory)
- ✅ Complete transfer capability
- ✅ Hard interrupt timeout

**Usage**:
```typescript
import { VMProvider } from 'rill/sandbox';

const engine = new Engine({
  provider: new VMProvider({ timeout: 5000 })
});
```

---

## 🎯 Recommended Solutions (By Scenario)

### React Native Applications

```typescript
// iOS/macOS
import { JSCProvider } from 'rill/sandbox/native';
const provider = new JSCProvider();

// Android/Cross-platform
import { QuickJSProvider } from 'rill/sandbox/native';
const provider = new QuickJSProvider();
```

### Web Mini-Program Platforms

```typescript
import { QuickJSNativeWASMProvider } from 'rill/sandbox/web';

const provider = new QuickJSNativeWASMProvider({
  wasmPath: '/assets/quickjs_sandbox.wasm'
});
```

**Advantages**:
- ✅ Uses same C++ code as RN
- ✅ Can pass functions, event objects
- ✅ Strong isolation (WASM)

### Node.js Build Tools

```typescript
import { VMProvider } from 'rill/sandbox';

const provider = new VMProvider({ timeout: 5000 });
```

---

## 📈 Performance Comparison

### Initialization Time

| Provider | First Load | Subsequent Loads |
|----------|-----------|------------------|
| JSC Native | < 1ms | < 1ms |
| QuickJS Native | < 5ms | < 1ms |
| QuickJS WASM | ~80ms | ~10ms (cached) |
| Node VM | < 1ms | < 1ms |

### Function Call Overhead (Per Call)

| Provider | Host → Guest | Guest → Host |
|----------|-------------|--------------|
| JSC Native | < 0.01ms | < 0.01ms |
| Hermes Native | < 0.01ms | < 0.01ms |
| QuickJS Native | < 0.02ms | < 0.02ms |
| QuickJS WASM | ~0.05ms | ~0.05ms |
| Node VM | < 0.01ms | < 0.01ms |

### Memory Usage

| Provider | Base Usage | Per Instance |
|----------|-----------|--------------|
| JSC Native | 0 MB | ~2 MB |
| Hermes Native | 0 MB | ~2 MB |
| QuickJS Native | ~5 MB | ~3 MB |
| QuickJS WASM | ~5 MB | ~3 MB |
| Node VM | 0 MB | ~2 MB |

---

## 🔧 DefaultProvider Strategy

```typescript
import { DefaultProvider, SandboxType } from 'rill/sandbox';

// Automatically select best Provider
const provider = DefaultProvider.create({
  timeout: 5000,
  wasmPath: '/assets/quickjs_sandbox.wasm' // Web only
});

// Or force a specific sandbox type
const hermesProvider = DefaultProvider.create({
  sandbox: SandboxType.Hermes
});

/**
 * Auto-selection logic (React Native):
 * 1. HermesProvider (when RILL_SANDBOX_ENGINE=hermes)
 * 2. JSCProvider (Apple platforms)
 * 3. QuickJSProvider (all platforms)
 *
 * Other platforms:
 * - Node/Bun → VMProvider
 * - Web → QuickJSNativeWASMProvider
 */
```

---

## Summary

### ✅ Design Principles

1. **Simple**: Only 5 solutions, no degradation, no fallback
2. **Secure**: All strongly isolated, either available or not, no intermediate states
3. **High Performance**: All support Scheme B (direct object passing)
4. **Unified**: Web and RN use same C++ code (QuickJS WASM)

### 🎯 Core Advantages

- ✅ Solved the fundamental "button click not working" issue (circular references)
- ✅ Can pass functions, event objects, complex types
- ✅ Zero JSON serialization overhead
- ✅ Simple architecture, low maintenance cost

### 📍 Solutions No Longer Provided

The following solutions don't meet design principles (cannot pass functions and circular references) and are no longer recommended:

- ❌ **Proxy Sandbox**: Weak isolation, escape risk
- ❌ **quickjs-emscripten**: Third-party wrapper, high maintenance cost

**Reason**: Rill only provides **secure and high-performance** solutions, no compromises.
