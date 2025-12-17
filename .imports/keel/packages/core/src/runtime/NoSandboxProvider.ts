import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from './engine';

// Augment globalThis for budget tracking
declare global {
  // eslint-disable-next-line no-var
  var __rill_budget_start: number | undefined;
  // eslint-disable-next-line no-var
  var __rill_budget_timeout: number | undefined;
  // eslint-disable-next-line no-var
  var __rill_budget_ops: number | undefined;
  // eslint-disable-next-line no-var
  var __checkBudget: (() => void) | undefined;
}

/**
 * An implementation of JSEngineProvider that runs code directly in the host context
 * using `eval`. This is insecure and should only be used with trusted code, but
 * offers maximum performance and easier debugging.
 *
 * Timeout Support:
 * - Cooperative timeout via __checkBudget() that guest code can call periodically.
 * - Hard timeout: NOT supported. Native eval cannot be interrupted externally.
 *   For production use with untrusted code, use VMProvider or WorkerJSEngineProvider.
 */
export class NoSandboxProvider implements JSEngineProvider {
  private options: { timeout?: number };

  constructor(options?: { timeout?: number }) {
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    let budgetCheckInjected = false;

    const context: JSEngineContext = {
      eval: (code: string) => {
        // Inject budget checker on first eval if timeout is set
        if (this.options.timeout && !budgetCheckInjected) {
          budgetCheckInjected = true;
          const timeout = this.options.timeout;

          // Inject a cooperative budget checker
          // Guest code can call __checkBudget() to cooperatively check timeout
          globalThis.__rill_budget_start = Date.now();
          globalThis.__rill_budget_timeout = timeout;
          globalThis.__rill_budget_ops = 0;
          globalThis.__checkBudget = () => {
            globalThis.__rill_budget_ops++;
            // Check every 1000 ops to avoid performance overhead
            if (globalThis.__rill_budget_ops % 1000 === 0) {
              const elapsed = Date.now() - globalThis.__rill_budget_start;
              if (elapsed > globalThis.__rill_budget_timeout) {
                throw new Error(
                  `[NoSandboxProvider] Cooperative timeout exceeded: ${elapsed}ms > ${globalThis.__rill_budget_timeout}ms`
                );
              }
            }
          };
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
        delete globalThis.__rill_budget_start;
        delete globalThis.__rill_budget_timeout;
        delete globalThis.__rill_budget_ops;
        delete globalThis.__checkBudget;
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
