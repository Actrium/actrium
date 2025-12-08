import { describe, it, expect, vi } from 'vitest';
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

describe('Observability & Health', () => {
  it('tracks errors and exposes getHealth', async () => {
    const provider = createMockQuickJSProvider();
    const engine = new Engine({ quickjs: provider, debug: false });
    await expect(engine.loadBundle('throw new Error("boom")')).rejects.toThrow();
    const health = engine.getHealth();
    expect(health.errorCount).toBeGreaterThan(0);
    expect(typeof health.lastErrorAt === 'number' || health.lastErrorAt === null).toBeTruthy();
  });

  it('emits metrics for sendToSandbox and receiver', async () => {
    const provider = createMockQuickJSProvider();
    const onMetric = vi.fn();
    const engine = new Engine({ quickjs: provider, debug: false, onMetric });
    await engine.loadBundle('console.log("hi")');
    engine.createReceiver(() => {});
    engine.sendEvent('PING', null);
    // render metrics
    engine.getReceiver()!.render();
    const names = onMetric.mock.calls.map((c) => c[0]);
    expect(names).toContain('engine.sendToSandbox');
    expect(names).toContain('receiver.render');
  });
});
