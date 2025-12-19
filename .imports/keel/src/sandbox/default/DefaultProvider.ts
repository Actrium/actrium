/**
 * DefaultProvider - Auto-selects the best JS engine provider based on environment
 *
 * For Web/Node environments:
 * 1. Node/Bun: VMProvider
 * 2. Browser with Worker support: WorkerProvider
 * 3. No fallback - throws error if no provider available
 */

import { VMProvider } from '../providers/VMProvider';
import { WorkerProvider } from '../providers/WorkerProvider';
import { SandboxType } from '../types/provider';

function isNodeEnv(): boolean {
  return (
    typeof process !== 'undefined' && process.versions != null && process.versions.node != null
  );
}

// Lazily resolve vm module
import type * as vm from 'node:vm';
type NodeVM = typeof vm;

function getVm(): NodeVM | null {
  if (typeof require === 'undefined') {
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
   * Force a specific sandbox type. If not specified, auto-detects the best provider.
   * Available types for Web/Node: SandboxType.VM, SandboxType.Worker
   */
  sandbox?: SandboxType.VM | SandboxType.Worker;
};

/**
 * DefaultProvider - Auto-selects the best JS engine provider based on environment
 *
 * Selection priority (when sandbox option not specified):
 * 1. Node/Bun: VMProvider
 * 2. Browser with Worker: WorkerProvider
 * 3. Error: No fallback (throws if no provider available)
 */
export class DefaultProvider {
  static create(options?: DefaultProviderOptions) {
    const envInfo = {
      isNode: isNodeEnv(),
      isWorkerCapable: isWorkerCapable(),
      hasVm: !!getVm(),
    };

    // Build provider options, only including timeout if defined
    const providerOptions =
      options?.timeout !== undefined ? { timeout: options.timeout } : undefined;

    // Explicit provider selection
    if (options?.sandbox === SandboxType.VM) {
      if (isNodeEnv() && getVm()) {
        return new VMProvider(providerOptions);
      }
      throw new Error(
        '[DefaultProvider] VMProvider requested but not available in this environment.'
      );
    }

    if (options?.sandbox === SandboxType.Worker) {
      if (isWorkerCapable()) {
        // Note: WorkerProvider does not support timeout option
        return new WorkerProvider();
      }
      throw new Error('[DefaultProvider] WorkerProvider requested but Worker not available.');
    }

    // Auto-detect best provider

    // 1. Node/Bun environment - prefer VMProvider (native, fast, supports timeout)
    if (isNodeEnv() && getVm()) {
      return new VMProvider(providerOptions);
    }

    // 2. Worker capable environment
    if (isWorkerCapable()) {
      // Note: WorkerProvider does not support timeout option
      return new WorkerProvider();
    }

    // No suitable provider available
    throw new Error(
      `[DefaultProvider] No suitable JS sandbox provider found. ` +
        `Environment: ${JSON.stringify(envInfo)}. ` +
        `Install appropriate dependencies or run in a supported environment.`
    );
  }
}
