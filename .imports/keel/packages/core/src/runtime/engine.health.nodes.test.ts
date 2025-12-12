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

function makeBatch(n: number) {
  return {
    operations: Array.from({ length: n }, (_, i) => ({
      op: 'CREATE',
      id: i + 1,
      type: 'View',
      props: {} as any,
    })),
  } as any;
}

describe('Engine health receiverNodes', () => {
  it('reflects receiver node count after applyBatch', async () => {
    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, debug: false });
    await engine.loadBundle('console.log("ok")');
    const receiver = engine.createReceiver(() => {});
    receiver.applyBatch(makeBatch(7));
    const health = engine.getHealth();
    expect(health.receiverNodes).toBe(7);
  });
});
