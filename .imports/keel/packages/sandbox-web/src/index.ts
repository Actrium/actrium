// rill/packages/sandbox-web/src/index.ts

// --- Temporary Interface Definitions ---
// These interfaces should eventually be imported from @rill/core
interface JSEngineContext {
  evaluate(script: string): any;
  evaluateAsync(script: string): Promise<any>;
  destroy(): void;
}

interface JSEngineRuntime {
  createContext(context?: object): Promise<JSEngineContext>;
  destroy(): void;
}

interface JSEngineProvider {
  createRuntime(options?: object): Promise<JSEngineRuntime>;
}
// --- End of Temporary Interface Definitions ---

type PendingRequest = {
  resolve: (value: any) => void;
  reject: (reason?: any) => void;
};

/**
 * Implements a JSI-like runtime using a Web Worker for sandboxing.
 */
class WWRuntime implements JSEngineRuntime {
  private worker: Worker;
  private nextRequestId = 0;
  private pendingRequests = new Map<string, PendingRequest>();

  constructor() {
    // In a bundled environment (Webpack, Vite), this pattern correctly creates a worker.
    // This assumes a build step will process this file.
    this.worker = new Worker(new URL('./worker.ts', import.meta.url));
    this.worker.onmessage = this.handleMessage.bind(this);
  }

  private handleMessage(event: MessageEvent<{ id: string; result?: any; error?: any }>): void {
    const { id, result, error } = event.data;
    const promise = this.pendingRequests.get(id);

    if (promise) {
      if (error) {
        promise.reject(Object.assign(new Error(), error));
      } else {
        promise.resolve(result);
      }
      this.pendingRequests.delete(id);
    }
  }

  async createContext(context?: object): Promise<JSEngineContext> {
    // The Web Worker is itself a single context. We can return `this` cast as a context.
    // The `context` object is not used in this simple implementation.
    return this as JSEngineContext;
  }

  /**
   * Synchronous evaluation is not supported in a Web Worker sandbox.
   * @throws {Error} Always throws an error.
   */
  evaluate(script: string): any {
    throw new Error('WebWorkerSandbox does not support synchronous evaluation. Please use evaluateAsync.');
  }

  /**
   * Asynchronously evaluates a script inside the Web Worker.
   * @param script The JavaScript code to execute.
   * @returns A promise that resolves with the result of the script.
   */
  evaluateAsync(script: string): Promise<any> {
    return new Promise((resolve, reject) => {
      const id = `req-${this.nextRequestId++}`;
      this.pendingRequests.set(id, { resolve, reject });
      this.worker.postMessage({ id, script });
    });
  }

  /**
   * Terminates the worker thread.
   */
  destroy(): void {
    this.worker.terminate();
    // Reject any pending promises
    for (const [id, promise] of this.pendingRequests.entries()) {
      promise.reject(new Error('Worker was terminated.'));
      this.pendingRequests.delete(id);
    }
  }
}

/**
 * Provider for creating Web Worker-based sandboxes.
 */
export class WWProvider implements JSEngineProvider {
  async createRuntime(options?: object): Promise<JSEngineRuntime> {
    return new WWRuntime();
  }
}
