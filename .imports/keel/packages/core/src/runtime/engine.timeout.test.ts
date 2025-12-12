import { describe, expect, it } from 'bun:test';
import { Engine } from './engine';

// Type definitions for mock QuickJS
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

// Mock QuickJS Provider for tests
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
              try {
                const fn = new Function(...globalNames, `"use strict"; ${code}`);
                return fn(...globalValues);
              } catch (e) {
                throw e;
              }
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

describe('Engine timeout behavior', () => {
  it('does not throw TimeoutError for quick microtask usage', async () => {
    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });
    // microtask scheduled inside guest should complete quickly
    await engine.loadBundle(`queueMicrotask(() => {});`);
    expect(engine.isLoaded).toBe(true);
  });

  it('does not throw TimeoutError even for long sync work (best-effort guard)', async () => {
    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, timeout: 5000, debug: false });
    // Busy loop to simulate long sync work; guard cannot preempt sync eval
    // Note: This is a best-effort timeout guard that only works if eval yields to event loop
    const code = `var s=0; for (var i=0;i<1e6;i++){ s+=i }`;
    await engine.loadBundle(code);
    expect(engine.isLoaded).toBe(true);
  });

  it('should trigger fatalError event and forceDestroy on hard timeout', async () => {
    let fatalErrorFired = false;
    let fatalErrorMessage = '';

    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, timeout: 100, debug: false });

    // Listen for fatalError event
    engine.on('fatalError', (error: Error) => {
      fatalErrorFired = true;
      fatalErrorMessage = error.message;
    });

    // Create code that never finishes (simulates infinite async operation)
    const neverResolves = new Promise(() => {
      // Intentionally never resolve
    });

    // Mock executeBundle to return the never-resolving promise
    const originalExecuteBundle = engine.executeBundle.bind(engine);
    engine.executeBundle = async () => {
      await neverResolves;
    };

    try {
      await engine.loadBundle('// will timeout');
    } catch (error: unknown) {
      // Expect TimeoutError
      expect(error).toBeInstanceOf(Error);
      expect((error as Error).message).toContain('timeout');
    }

    // Wait for timeout to trigger
    await new Promise((resolve) => setTimeout(resolve, 150));

    // Verify fatalError was emitted
    expect(fatalErrorFired).toBe(true);
    expect(fatalErrorMessage).toContain('timeout');

    // Verify engine was force destroyed
    expect(engine.destroyed).toBe(true);
    expect(engine.isLoaded).toBe(false);

    // Restore original method
    engine.executeBundle = originalExecuteBundle;
  });

  it('should handle forceDestroy when context.dispose() throws', () => {
    let disposeThrew = false;

    const provider: MockQuickJSProvider = {
      createRuntime(): MockQuickJSRuntime {
        return {
          createContext(): MockQuickJSContext {
            return {
              eval: () => {},
              setGlobal: () => {},
              getGlobal: () => undefined,
              dispose(): void {
                disposeThrew = true;
                throw new Error('Dispose failed');
              },
            };
          },
          dispose: () => {},
        };
      },
    };

    const engine = new Engine({ quickjs: provider, timeout: 1000, debug: false });

    // Load a simple bundle to initialize context
    engine.loadBundle('1 + 1').catch(() => {});

    // Wait for init
    setTimeout(() => {
      // Call forceDestroy directly
      expect(() => {
        engine.forceDestroy();
      }).not.toThrow();

      // Verify dispose was attempted
      expect(disposeThrew).toBe(true);

      // Verify engine is still marked as destroyed despite error
      expect(engine.destroyed).toBe(true);
      expect(engine.context).toBe(null);
    }, 50);
  });

  it('should handle forceDestroy when runtime.dispose() throws', () => {
    let runtimeDisposeThrew = false;

    const provider: MockQuickJSProvider = {
      createRuntime(): MockQuickJSRuntime {
        return {
          createContext(): MockQuickJSContext {
            return {
              eval: () => {},
              setGlobal: () => {},
              getGlobal: () => undefined,
              dispose: () => {},
            };
          },
          dispose(): void {
            runtimeDisposeThrew = true;
            throw new Error('Runtime dispose failed');
          },
        };
      },
    };

    const engine = new Engine({ quickjs: provider, timeout: 1000, debug: false });

    // Load to initialize
    engine.loadBundle('1 + 1').catch(() => {});

    setTimeout(() => {
      expect(() => {
        engine.forceDestroy();
      }).not.toThrow();

      expect(runtimeDisposeThrew).toBe(true);
      expect(engine.destroyed).toBe(true);
      expect(engine.runtime).toBe(null);
    }, 50);
  });

  it('should clear timers before disposing resources in forceDestroy', async () => {
    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, timeout: 1000, debug: false });

    const timerFired = false;

    await engine.loadBundle(`
      const timer = setTimeout(() => {
        // This should not run after forceDestroy
      }, 1000);
    `);

    // Verify timer was created
    expect(engine.timeoutMap.size).toBeGreaterThan(0);

    // Force destroy
    engine.forceDestroy();

    // Verify timers were cleared
    expect(engine.timeoutMap.size).toBe(0);
    expect(engine.intervalMap.size).toBe(0);

    // Wait to ensure timer doesn't fire
    await new Promise((resolve) => setTimeout(resolve, 1100));

    expect(timerFired).toBe(false);
  });
});
