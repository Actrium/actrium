import vm from 'node:vm';
import type { JSEngineContext, JSEngineProvider, JSEngineRuntime } from './engine';

/**
 * An implementation of JSEngineProvider that uses the Node.js `vm` module.
 * This provides a secure, native sandbox for executing code in Node.js/Bun environments.
 *
 * Timeout Support:
 * - Hard timeout: YES. Uses vm.Script.runInContext({ timeout }) which is a true hard interrupt.
 */
export class VMProvider implements JSEngineProvider {
  private options: { timeout?: number };

  constructor(options?: { timeout?: number }) {
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    const vmContext = vm.createContext({});

    const context: JSEngineContext = {
      eval: (code: string) => {
        const script = new vm.Script(code);
        // vm.Script timeout provides hard interrupt capability
        return script.runInContext(vmContext, { timeout: this.options.timeout });
      },

      setGlobal: (name: string, value: unknown) => {
        vmContext[name] = value;
      },

      getGlobal: (name: string) => {
        return vmContext[name];
      },

      dispose: () => {
        // In vm, the context is just an object that can be garbage collected.
        // Clear all properties to help GC
        for (const key of Object.keys(vmContext)) {
          delete vmContext[key];
        }
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
