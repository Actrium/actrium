/**
 * @rill/sandbox-web
 *
 * Web Worker-based JavaScript sandbox for browser environments.
 */

export interface WorkerMessage {
  type: 'eval' | 'setGlobal' | 'getGlobal' | 'dispose';
  id?: string;
  code?: string;
  name?: string;
  // Reason: Worker message value can be any serializable type
  value?: unknown;
}

export interface WorkerResponse {
  id: string;
  // Reason: eval() and getGlobal() return arbitrary types
  result?: unknown;
  error?: { name: string; message: string; stack?: string };
}

/**
 * JSEngineContext interface - matches @rill/sandbox types
 */
export interface JSEngineContext {
  // Reason: eval() returns arbitrary type from dynamic code execution
  eval(code: string): unknown;
  // Reason: setGlobal accepts any serializable value
  setGlobal(name: string, value: unknown): void;
  // Reason: getGlobal returns arbitrary type from sandbox globals
  getGlobal(name: string): unknown;
  dispose(): void;
}

/**
 * Extended context with async eval support
 */
export interface WorkerContext extends JSEngineContext {
  // Reason: evalAsync returns arbitrary type from dynamic code execution
  evalAsync(code: string): Promise<unknown>;
}

/**
 * JSEngineRuntime interface
 */
export interface JSEngineRuntime {
  createContext(): WorkerContext;
  dispose(): void;
}

/**
 * JSEngineProvider interface
 */
export interface JSEngineProvider {
  createRuntime(): Promise<JSEngineRuntime>;
}

/**
 * WorkerProvider - Web Worker-based sandbox provider
 *
 * Uses Web Worker for isolation and new Function() for code execution.
 * Only supports async evaluation (evalAsync).
 */
export class WorkerProvider implements JSEngineProvider {
  private workerFactory: (() => Worker) | undefined;

  constructor(createWorker?: () => Worker) {
    this.workerFactory = createWorker;
  }

  async createRuntime(): Promise<JSEngineRuntime> {
    const worker = this.workerFactory
      ? this.workerFactory()
      : new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });

    // Reason: Promise resolves/rejects with arbitrary types from worker
    const pending = new Map<
      string,
      { resolve: (v: unknown) => void; reject: (e: unknown) => void }
    >();
    let msgId = 0;

    const post = (msg: WorkerMessage) => worker.postMessage(msg);

    worker.onmessage = (ev: MessageEvent<WorkerResponse>) => {
      const { id, result, error } = ev.data;
      const promise = pending.get(id);
      if (promise) {
        if (error) {
          const err = new Error(error.message);
          err.name = error.name;
          if (error.stack) err.stack = error.stack;
          promise.reject(err);
        } else {
          promise.resolve(result);
        }
        pending.delete(id);
      }
    };

    // Reason: Worker call returns arbitrary type from eval/getGlobal
    const call = (msg: Omit<WorkerMessage, 'id'>): Promise<unknown> => {
      const id = String(++msgId);
      return new Promise<unknown>((resolve, reject) => {
        pending.set(id, { resolve, reject });
        post({ ...msg, id });
      });
    };

    // Reason: Global variables can be any serializable type
    const globals = new Map<string, unknown>();

    const context: WorkerContext = {
      eval: (_code: string) => {
        throw new Error('[WorkerProvider] Use evalAsync - sync eval not supported in Worker');
      },

      evalAsync: async (code: string) => {
        return await call({ type: 'eval', code });
      },

      // Reason: setGlobal accepts any serializable value for sandbox globals
      setGlobal: (name: string, value: unknown) => {
        // Functions cannot be serialized via postMessage
        if (typeof value === 'function') {
          return;
        }
        if (value && typeof value === 'object') {
          // Check if object contains functions (which can't be serialized)
          try {
            const serialized = JSON.stringify(value);
            if (serialized === '{}' && Object.keys(value as object).length > 0) {
              return;
            }
          } catch {
            return;
          }
        }
        globals.set(name, value);
        post({ type: 'setGlobal', name, value });
      },

      // Reason: getGlobal returns arbitrary type from sandbox globals
      getGlobal: (name: string): unknown => {
        return globals.get(name);
      },

      dispose: () => {
        post({ type: 'dispose' });
        worker.terminate();
        for (const [, promise] of pending) {
          promise.reject(new Error('Worker was terminated'));
        }
        pending.clear();
        globals.clear();
      },
    };

    return {
      createContext: () => context,
      dispose: () => context.dispose(),
    };
  }
}

// Re-export for convenience
export default WorkerProvider;
