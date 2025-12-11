import vm from 'node:vm';
import type { JSEngineProvider, JSEngineRuntime, JSEngineContext } from './engine';

/**
 * An implementation of JSEngineProvider that uses the Node.js `vm` module.
 * This provides a secure, native sandbox for executing code in Node.js/Bun environments.
 *
 * Timeout/Interrupt Support:
 * - Hard timeout: YES. Uses vm.Script.runInContext({ timeout }) which is a true hard interrupt.
 * - setInterruptHandler: Supported for interface consistency, but vm's built-in timeout
 *   is the primary mechanism for preventing runaway execution.
 */
export class VMProvider implements JSEngineProvider {
  private options: { timeout?: number };

  constructor(options?: { timeout?: number }) {
    this.options = options || {};
  }

  createRuntime(): JSEngineRuntime {
    const vmContext = vm.createContext({});
    let interruptHandler: (() => boolean) | null = null;

    const context: JSEngineContext = {
      eval: (code: string) => {
        // Check interrupt handler before eval
        if (interruptHandler && interruptHandler()) {
          throw new Error('[VMProvider] Execution interrupted by handler');
        }

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
        interruptHandler = null;
      },

      /**
       * Set interrupt handler. Note: vm module has built-in timeout support
       * which is the primary hard interrupt mechanism. This handler is checked
       * before each eval call for interface consistency.
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
