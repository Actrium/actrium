import type { JSEngineContext, JSEngineProvider } from './engine';

export interface WorkerMessage {
  type: 'init' | 'eval' | 'setGlobal' | 'dispose';
  id?: string;
  code?: string;
  name?: string;
  value?: unknown;
  options?: { timeout?: number };
}

/**
 * Worker-specific context with async eval support.
 * Extends JSEngineContext with non-standard evalAsync for async-only execution.
 */
interface WorkerJSEngineContext extends JSEngineContext {
  /** Async code evaluation - Worker's primary execution method */
  evalAsync(code: string): Promise<unknown>;
}

/**
 * An implementation of JSEngineProvider that runs code in a Web Worker
 * using @sebastianwessel/quickjs WASM sandbox.
 *
 * Note: Workers are inherently async and cannot implement sync eval().
 * Engine detects evalAsync and uses it automatically.
 */
export class WorkerJSEngineProvider implements JSEngineProvider {
  constructor(
    private createWorker: () => Worker,
    private options?: { timeout?: number }
  ) {}

  async createRuntime() {
    const worker = this.createWorker();
    let interruptHandler: (() => boolean) | null = null;

    const pending = new Map<
      string,
      { resolve: (v: unknown) => void; reject: (e: unknown) => void }
    >();
    let msgId = 0;

    const post = (msg: WorkerMessage) => worker.postMessage(msg);

    worker.onmessage = (ev: MessageEvent) => {
      const { id, result, error } = ev.data as { id?: string; result?: unknown; error?: string };
      if (id && pending.has(id)) {
        const promise = pending.get(id)!;
        if (error) {
          promise.reject(new Error(error));
        } else {
          promise.resolve(result);
        }
        pending.delete(id);
      }
    };

    const call = (msg: Omit<WorkerMessage, 'id'>) => {
      const id = String(++msgId);
      return new Promise<unknown>((resolve, reject) => {
        pending.set(id, { resolve, reject });
        post({ ...msg, id });
      });
    };

    // Initialize worker and wait for ready
    await call({ type: 'init', options: { timeout: this.options?.timeout } });

    const context: WorkerJSEngineContext = {
      eval: (_code: string) => {
        // This provider is async-only.
        throw new Error('Use evalAsync with WorkerJSEngineProvider');
      },

      evalAsync: async (code: string) => {
        // Check interrupt handler before sending to worker
        if (interruptHandler?.()) {
          throw new Error('[WorkerJSEngineProvider] Execution interrupted by handler');
        }
        return await call({ type: 'eval', code });
      },

      setGlobal: (name: string, value: unknown) => {
        // Functions cannot be serialized via postMessage - worker has built-in shims
        if (typeof value === 'function') {
          // Silent skip - worker provides its own require, console, etc.
          return;
        }
        if (value && typeof value === 'object') {
          // Objects containing functions (like React) also can't be serialized
          const hasFunction = Object.values(value).some((v) => typeof v === 'function');
          if (hasFunction) return;
        }
        // Fire-and-forget for serializable setGlobal
        post({ type: 'setGlobal', name, value });
      },

      getGlobal: (_name: string): unknown => {
        // Worker cannot do synchronous getGlobal - return undefined
        return undefined;
      },

      dispose: () => {
        // Don't wait for dispose response - just terminate the worker
        post({ type: 'dispose' });
        worker.terminate();
        interruptHandler = null;
      },
    };

    const runtime = {
      createContext: () => context,
      dispose: () => context.dispose(),
    };

    return runtime;
  }
}
