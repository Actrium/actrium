import { describe, it, expect } from 'vitest';
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
    const engine = new Engine({ quickjs: provider, timeout: 1, debug: false });
    // microtask scheduled inside plugin should complete quickly
    await engine.loadBundle(`queueMicrotask(() => {});`);
    expect(engine.isLoaded).toBe(true);
  });

  it('does not throw TimeoutError even for long sync work (best-effort guard)', async () => {
    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, timeout: 1, debug: false });
    // Busy loop to simulate long sync work; guard cannot preempt sync eval
    const code = `var s=0; for (var i=0;i<5e6;i++){ s+=i }`;
    await engine.loadBundle(code);
    expect(engine.isLoaded).toBe(true);
  });
});
