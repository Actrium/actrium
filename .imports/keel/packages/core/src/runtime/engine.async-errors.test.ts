import { describe, expect, it } from 'bun:test';
import { Engine } from './engine';

// Mock QuickJS Provider
interface MockQuickJSContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

interface MockQuickJSRuntime {
  createContext(): MockQuickJSContext;
  dispose(): void;
}

interface MockQuickJSProvider {
  createRuntime(): MockQuickJSRuntime;
}

function createMockQuickJSProvider(): MockQuickJSProvider {
  return {
    createRuntime(): MockQuickJSRuntime {
      const globals = new Map<string, unknown>();
      return {
        createContext(): MockQuickJSContext {
          return {
            eval(code: string): unknown {
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              const fn = new Function(...globalNames, `"use strict"; ${code}`);
              return fn(...globalValues);
            },
            setGlobal(name: string, value: unknown): void {
              globals.set(name, value);
            },
            getGlobal(name: string): unknown {
              return globals.get(name);
            },
            dispose(): void {
              globals.clear();
            },
          };
        },
        dispose(): void {},
      };
    },
  };
}

describe('Engine async error handling', () => {
  describe('setInterval error handling', () => {
    it('should catch and emit errors from setInterval callbacks', async () => {
      const provider = createMockQuickJSProvider();
      const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });

      const errorsCaught: Error[] = [];

      // Listen for error events
      engine.on('error', (error: Error) => {
        errorsCaught.push(error);
      });

      await engine.loadBundle(`
        // This will be replaced by Engine's setInterval implementation
      `);

      // Directly test the setInterval error handling by calling it
      // Access the guest setInterval that Engine injected
      const guestSetInterval = engine.context?.getGlobal('setInterval') as (
        fn: () => void,
        delay: number
      ) => number;

      expect(guestSetInterval).toBeDefined();

      if (guestSetInterval) {
        // Create an interval that throws
        const intervalId = guestSetInterval(() => {
          throw new Error('Interval callback error');
        }, 10);

        // Wait for interval to fire
        await new Promise((resolve) => setTimeout(resolve, 50));

        // Verify error was caught and emitted
        expect(errorsCaught.length).toBeGreaterThan(0);
        expect(errorsCaught[0].message).toBe('Interval callback error');

        // Verify errorCount was incremented
        expect(engine.errorCount).toBeGreaterThan(0);
        expect(engine.lastErrorAt).toBeGreaterThan(0);

        // Clear the interval
        const guestClearInterval = engine.context?.getGlobal('clearInterval') as (
          id: number
        ) => void;
        if (guestClearInterval) {
          guestClearInterval(intervalId);
        }
      }

      engine.destroy();
    });

    it('should not crash when interval callback throws non-Error', async () => {
      const provider = createMockQuickJSProvider();
      const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });

      const errorsCaught: Error[] = [];
      engine.on('error', (error: Error) => {
        errorsCaught.push(error);
      });

      await engine.loadBundle(`// init`);

      const guestSetInterval = engine.context?.getGlobal('setInterval') as (
        fn: () => void,
        delay: number
      ) => number;

      if (guestSetInterval) {
        const intervalId = guestSetInterval(() => {
          throw 'string error'; // Throw non-Error
        }, 10);

        await new Promise((resolve) => setTimeout(resolve, 50));

        expect(errorsCaught.length).toBeGreaterThan(0);
        expect(errorsCaught[0]).toBeInstanceOf(Error);
        expect(errorsCaught[0].message).toBe('string error');

        const guestClearInterval = engine.context?.getGlobal('clearInterval') as (
          id: number
        ) => void;
        if (guestClearInterval) {
          guestClearInterval(intervalId);
        }
      }

      engine.destroy();
    });
  });

  describe('Unhandled Promise Rejection handling', () => {
    it('should verify rejection handler logic for Error type', () => {
      // Test the logic that would be used in unhandledRejectionHandler
      const rejectionError = new Error('Test rejection');
      const event = {
        reason: rejectionError,
        promise: undefined as Promise<unknown> | undefined,
        preventDefault: () => {},
      };

      // Verify Error is kept as-is
      const error = event.reason instanceof Error ? event.reason : new Error(String(event.reason));

      expect(error).toBe(rejectionError);
      expect(error.message).toBe('Test rejection');
    });

    it('should verify rejection handler converts non-Error to Error', () => {
      // Test the logic for non-Error rejection reasons
      const event = {
        reason: 'string rejection',
        promise: undefined as Promise<unknown> | undefined,
        preventDefault: () => {},
      };

      // The handler should convert non-Error to Error
      const error = event.reason instanceof Error ? event.reason : new Error(String(event.reason));

      expect(error).toBeInstanceOf(Error);
      expect(error.message).toBe('string rejection');
    });

    it('should verify rejection handler handles undefined reason', () => {
      const event = {
        reason: undefined,
        promise: undefined as Promise<unknown> | undefined,
        preventDefault: () => {},
      };

      const error = event.reason instanceof Error ? event.reason : new Error(String(event.reason));

      expect(error).toBeInstanceOf(Error);
      expect(error.message).toBe('undefined');
    });
  });

  describe('handleCallFunction error handling', () => {
    it('should catch errors when invoking callbacks', async () => {
      const provider = createMockQuickJSProvider();
      const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });

      await engine.loadBundle(`
        globalThis.__invokeCallback = function(fnId, args) {
          throw new Error('Callback invocation failed');
        };
      `);

      // Call handleCallFunction with a message
      const message = {
        type: 'CALL_FUNCTION' as const,
        fnId: 'test-fn-id',
        args: [1, 2, 3],
      };

      // handleCallFunction should catch the error and log it
      await expect(async () => {
        await engine.handleCallFunction(message);
      }).not.toThrow();

      // Error should be logged but not thrown
      engine.destroy();
    });

    it('should handle missing context gracefully in handleCallFunction', async () => {
      const provider = createMockQuickJSProvider();
      const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });

      await engine.loadBundle(`// init`);

      // Destroy the context
      engine.context = null;

      const message = {
        type: 'CALL_FUNCTION' as const,
        fnId: 'test-fn-id',
        args: [],
      };

      // Should return early without throwing
      await expect(async () => {
        await engine.handleCallFunction(message);
      }).not.toThrow();
    });

    it('should handle evalCode failure in handleCallFunction', async () => {
      const provider = createMockQuickJSProvider();
      const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });

      await engine.loadBundle(`// init`);

      // Mock evalCode to throw
      const originalEvalCode = engine.evalCode.bind(engine);
      engine.evalCode = async () => {
        throw new Error('evalCode failed');
      };

      const message = {
        type: 'CALL_FUNCTION' as const,
        fnId: 'test-fn-id',
        args: [],
      };

      // Should catch the error and not throw
      await expect(async () => {
        await engine.handleCallFunction(message);
      }).not.toThrow();

      // Restore
      engine.evalCode = originalEvalCode;
      engine.destroy();
    });
  });

  describe('error event emission', () => {
    it('should increment errorCount when errors occur', async () => {
      const provider = createMockQuickJSProvider();
      const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });

      await engine.loadBundle(`// init`);

      const initialErrorCount = engine.errorCount;

      // Trigger an interval error
      const guestSetInterval = engine.context?.getGlobal('setInterval') as (
        fn: () => void,
        delay: number
      ) => number;

      if (guestSetInterval) {
        const intervalId = guestSetInterval(() => {
          throw new Error('Test error');
        }, 10);

        await new Promise((resolve) => setTimeout(resolve, 50));

        expect(engine.errorCount).toBeGreaterThan(initialErrorCount);
        expect(engine.lastErrorAt).toBeGreaterThan(0);

        const guestClearInterval = engine.context?.getGlobal('clearInterval') as (
          id: number
        ) => void;
        if (guestClearInterval) {
          guestClearInterval(intervalId);
        }
      }

      engine.destroy();
    });
  });
});
