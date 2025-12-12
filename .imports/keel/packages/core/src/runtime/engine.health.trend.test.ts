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

describe('Engine health trend', () => {
  it('increments errorCount on failure, then can recover and load successfully', async () => {
    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, debug: false });

    await expect(engine.loadBundle('throw new Error("e1")')).rejects.toThrow();
    const h1 = engine.getHealth();
    expect(h1.errorCount).toBeGreaterThan(0);
    expect(h1.loaded).toBe(false);

    await engine.loadBundle('console.log("ok")');
    const h2 = engine.getHealth();
    expect(h2.loaded).toBe(true);
    expect(h2.errorCount).toBeGreaterThan(0);
    expect(typeof h2.lastErrorAt === 'number' || h2.lastErrorAt === null).toBeTruthy();
  });
});
