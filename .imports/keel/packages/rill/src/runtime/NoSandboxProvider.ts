import type { JSEngineProvider, JSEngineRuntime, JSEngineContext } from './engine';

/**
 * An implementation of JSEngineProvider that runs code directly in the host context
 * using `eval`. This is insecure and should only be used with trusted code, but
 * offers maximum performance and easier debugging.
 *
 * Timeout/Interrupt Support:
 * - setInterruptHandler: Supported, but only effective if guest code cooperatively
 *   calls __checkBudget() periodically (e.g., in loops).
 * - Hard timeout: NOT supported. Native eval cannot be interrupted externally.
 *   For production use with untrusted code, use VMProvider or WorkerJSEngineProvider.
 */
export class NoSandboxProvider implements JSEngineProvider {
  private options: { timeout?: number };

  constructor(options?: { timeout?: number }) {
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    let interruptHandler: (() => boolean) | null = null;
    let budgetCheckInjected = false;

    const context: JSEngineContext = {
      eval: (code: string) => {
        // Inject budget checker on first eval if timeout is set
        if (this.options.timeout && !budgetCheckInjected) {
          budgetCheckInjected = true;
          const timeout = this.options.timeout;

          // Inject a cooperative budget checker
          // Guest code can call __checkBudget() to cooperatively check timeout
          (globalThis as any).__rill_budget_start = Date.now();
          (globalThis as any).__rill_budget_timeout = timeout;
          (globalThis as any).__rill_budget_ops = 0;
          (globalThis as any).__checkBudget = function() {
            const g = globalThis as any;
            g.__rill_budget_ops++;
            // Check every 1000 ops to avoid performance overhead
            if (g.__rill_budget_ops % 1000 === 0) {
              const elapsed = Date.now() - g.__rill_budget_start;
              if (elapsed > g.__rill_budget_timeout) {
                throw new Error(`[NoSandboxProvider] Cooperative timeout exceeded: ${elapsed}ms > ${g.__rill_budget_timeout}ms`);
              }
            }
          };
        }

        // Check interrupt handler before eval (won't help during eval, but catches between calls)
        if (interruptHandler && interruptHandler()) {
          throw new Error('[NoSandboxProvider] Execution interrupted by handler');
        }

        // Direct, unscoped eval in the global context.
        return (0, eval)(code);
      },

      setGlobal: (name: string, value: unknown) => {
        (globalThis as Record<string, unknown>)[name] = value;
      },

      getGlobal: (name: string) => {
        return (globalThis as Record<string, unknown>)[name];
      },

      dispose: () => {
        // Clean up injected globals
        delete (globalThis as any).__rill_budget_start;
        delete (globalThis as any).__rill_budget_timeout;
        delete (globalThis as any).__rill_budget_ops;
        delete (globalThis as any).__checkBudget;
        interruptHandler = null;
      },

      /**
       * Set interrupt handler. Note: This is only checked BETWEEN eval calls,
       * not during synchronous execution. For cooperative timeout during execution,
       * guest code must call __checkBudget() periodically.
       */
      setInterruptHandler: (handler: () => boolean) => {
        interruptHandler = handler;
      },

      clearInterruptHandler: () => {
        interruptHandler = null;
      },
    };

    return {
      createContext: () => context,
      dispose: () => {
        context.dispose();
      },
    };
  }
}
