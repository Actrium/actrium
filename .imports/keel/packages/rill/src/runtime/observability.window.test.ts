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

describe('Host-side sliding window metrics (example)', () => {
  it('collects last N onMetric events in a sliding window', async () => {
    const provider = createMockQuickJSProvider();
    const window: Array<{ name: string; value: number }> = [];
    const N = 5;
    const engine = new Engine({ quickjs: provider, onMetric: (n, v) => {
      window.push({ name: n, value: v });
      if (window.length > N) window.shift();
    }});

    await engine.loadBundle('console.log("ok")');
    engine.createReceiver(() => {});
    engine.sendEvent('PING');
    engine.getReceiver()!.render();

    expect(window.length).toBeGreaterThan(0);
    expect(window.length).toBeLessThanOrEqual(N);
  });
});
