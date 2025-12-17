import { NoSandboxProvider } from './NoSandboxProvider';
import { RNQuickJSProvider, resolveRNQuickJS } from './RNQuickJSProvider';
import { VMProvider } from './VMProvider';
import { WorkerJSEngineProvider } from './WorkerJSEngineProvider';

function isReactNativeEnv(): boolean {
  // Heuristics: RN global markers
  return (
    !!globalThis.ReactNative ||
    typeof globalThis.nativeCallSyncHook === 'function' ||
    typeof globalThis.__BUNDLE_START_TIME__ !== 'undefined'
  );
}

function isNodeEnv(): boolean {
  return (
    typeof process !== 'undefined' && process.versions != null && process.versions.node != null
  );
}

// Lazily resolve vm module to avoid Metro bundler errors in React Native
import type * as vm from 'node:vm';

type NodeVM = typeof vm;
function getVm(): NodeVM | null {
  if (typeof require === 'undefined' || isReactNativeEnv()) {
    return null;
  }
  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    return require('node:vm');
  } catch {
    return null;
  }
}

function isWorkerCapable(): boolean {
  try {
    return typeof Worker === 'function' && typeof URL === 'function';
  } catch {
    return false;
  }
}

export type DefaultProviderOptions = {
  timeout?: number;
  /**
   * Force a specific sandbox mode. If not specified, auto-detects the best provider.
   * - 'vm': Node.js vm module (Node/Bun only)
   * - 'worker': Web Worker with QuickJS WASM
   * - 'rn': React Native QuickJS JSI binding
   * - 'none': No sandbox (dev-only, insecure)
   */
  sandbox?: 'vm' | 'worker' | 'rn' | 'none';
};

/**
 * DefaultJSEngineProvider - Auto-selects the best JS engine provider based on environment
 *
 * Selection priority (when sandbox option not specified):
 * 1. React Native: RNQuickJSProvider
 * 2. Node/Bun: VMProvider
 * 3. Worker capable: WorkerJSEngineProvider
 * 4. Fallback: NoSandboxProvider (dev-only, with warning)
 */
export class DefaultJSEngineProvider {
  static create(options?: DefaultProviderOptions) {
    const envInfo = {
      isRN: isReactNativeEnv(),
      isNode: isNodeEnv(),
      isWorkerCapable: isWorkerCapable(),
      hasVm: !!getVm(),
      hasQuickJS: !!resolveRNQuickJS(),
    };

    // 'none' is an explicit opt-out of sandboxing (dev-only)
    if (options?.sandbox === 'none') {
      console.log('[rill] Provider: NoSandboxProvider (explicit)', envInfo);
      return new NoSandboxProvider({ timeout: options?.timeout });
    }

    // Explicit provider selection
    if (options?.sandbox === 'rn') {
      const quickjsModule = resolveRNQuickJS();
      if (quickjsModule) {
        console.log('[rill] Provider: RNQuickJSProvider (explicit)', envInfo);
        return new RNQuickJSProvider(quickjsModule, { timeout: options?.timeout });
      }
      console.warn('[rill] RNQuickJSProvider requested but react-native-quickjs not available.');
    }

    if (options?.sandbox === 'vm') {
      if (isNodeEnv() && getVm()) {
        console.log('[rill] Provider: VMProvider (explicit)', envInfo);
        return new VMProvider({ timeout: options?.timeout });
      }
      console.warn('[rill] VMProvider requested but not available in this environment.');
    }

    if (options?.sandbox === 'worker') {
      if (isWorkerCapable() && !isReactNativeEnv()) {
        console.log('[rill] Provider: WorkerJSEngineProvider (explicit)', envInfo);
        const createWorker = () =>
          new Worker(new URL('./engine.worker.ts', import.meta.url), { type: 'module' });
        return new WorkerJSEngineProvider(createWorker, { timeout: options?.timeout });
      }
      console.warn('[rill] WorkerJSEngineProvider requested but Worker not available.');
    }

    // Auto-detect best provider when sandbox option is not specified
    // Priority: RN > VM > Worker

    // 1. React Native environment
    if (isReactNativeEnv()) {
      const quickjsModule = resolveRNQuickJS();
      if (quickjsModule) {
        console.log('[rill] Provider: RNQuickJSProvider (auto-detected RN)', envInfo);
        return new RNQuickJSProvider(quickjsModule, { timeout: options?.timeout });
      }
      // RN but no QuickJS - will fallback below
    }

    // 2. Node/Bun environment - prefer VMProvider (native, fast, supports timeout)
    if (isNodeEnv() && getVm()) {
      console.log('[rill] Provider: VMProvider (auto-detected Node)', envInfo);
      return new VMProvider({ timeout: options?.timeout });
    }

    // 3. Worker capable environment
    if (isWorkerCapable() && !isReactNativeEnv()) {
      console.log('[rill] Provider: WorkerJSEngineProvider (auto-detected Worker)', envInfo);
      const createWorker = () =>
        new Worker(new URL('./engine.worker.ts', import.meta.url), { type: 'module' });
      return new WorkerJSEngineProvider(createWorker, { timeout: options?.timeout });
    }

    // Fallback (dev only): This should rarely happen
    console.log('[rill] Provider: NoSandboxProvider (FALLBACK - no sandbox available!)', envInfo);
    return new NoSandboxProvider({ timeout: options?.timeout });
  }
}
