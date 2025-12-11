import type { JSEngineProvider, JSEngineContext } from './engine';

export interface WorkerMessage {
  type: 'init' | 'eval' | 'setGlobal' | 'getGlobal' | 'dispose';
  id?: string;
  code?: string;
  name?: string;
  value?: unknown;
  options?: { timeout?: number };
}

/**
 * An implementation of JSEngineProvider that runs code in a Web Worker
 * using @sebastianwessel/quickjs WASM sandbox.
 *
 * Timeout/Interrupt Support:
 * - Hard timeout: YES. QuickJS WASM supports executionTimeout with setInterruptHandler.
 * - setInterruptHandler: The actual interrupt handler is managed inside the worker.
 *   This interface method allows for pre-eval checks on the main thread.
 */
export class WorkerJSEngineProvider implements JSEngineProvider {
  constructor(private createWorker: () => Worker, private options?: { timeout?: number }) {}

  async createRuntime() {
    const worker = this.createWorker();
    let interruptHandler: (() => boolean) | null = null;

    const pending = new Map<string, { resolve: (v: unknown) => void; reject: (e: unknown) => void }>();
    let msgId = 0;

    const post = (msg: WorkerMessage) => worker.postMessage(msg);

    worker.onmessage = (ev: MessageEvent) => {
      const { id, result, error } = ev.data;
      if (id && pending.has(id)) {
        if (error) {
          pending.get(id)!.reject(new Error(error));
        } else {
          pending.get(id)!.resolve(result);
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

    const context: JSEngineContext = {
      eval: (code: string) => {
        // This provider is async-only.
        throw new Error('Use evalAsync with WorkerJSEngineProvider');
      },

      evalAsync: async (code: string) => {
        // Check interrupt handler before sending to worker
        if (interruptHandler && interruptHandler()) {
          throw new Error('[WorkerJSEngineProvider] Execution interrupted by handler');
        }
        return await call({ type: 'eval', code });
      },

      setGlobal: (name: string, value: unknown) => {
        // Skip non-serializable values (functions, React objects, etc.)
        // Worker sandbox should provide its own shims for these
        if (typeof value === 'function') return;
        if (value && typeof value === 'object') {
          // Check if object contains functions (like console, React)
          const hasFunction = Object.values(value).some(v => typeof v === 'function');
          if (hasFunction) return;
        }
        // Fire-and-forget for serializable setGlobal
        post({ type: 'setGlobal', name, value });
      },

      getGlobal: async (name: string) => {
        return await call({ type: 'getGlobal', name });
      },

      dispose: () => {
        post({ type: 'dispose' });
        worker.terminate();
        interruptHandler = null;
      },

      /**
       * Set interrupt handler. Note: The actual hard interrupt is handled by
       * QuickJS executionTimeout inside the worker. This handler is checked
       * before each evalAsync call on the main thread.
       */
      setInterruptHandler: (handler: () => boolean) => {
        interruptHandler = handler;
      },

      clearInterruptHandler: () => {
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
