import { describe, it, expect, mock, beforeAll } from 'bun:test';
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

function buildBundle(code: string) {
  // Use var instead of const to avoid redeclaration error since React is already injected as global
  return `
    var _React = require('react');
    var _jsx = require('react/jsx-runtime');
    var _reconciler = require('rill/reconciler');
    ${code}
  `;
}

describe('Engine enhanced behaviors', () => {
  let provider: MockQuickJSProvider;
  beforeAll(() => {
    provider = createMockQuickJSProvider();
  });
  it('enforces require whitelist', async () => {
    const engine = new Engine({ quickjs: provider, debug: false, requireWhitelist: ['react'] });
    const src = "require('react-native')";
    await expect(engine.loadBundle(src)).rejects.toThrow(/Unsupported require/);
  });

  it('reports metrics via onMetric', async () => {
    const onMetric = mock();
    const engine = new Engine({ quickjs: provider, debug: false, onMetric });
    const src = buildBundle(`console.log('hello')`);
    await engine.loadBundle(src);
    // Should have at least these metrics
    const names = onMetric.mock.calls.map((c) => c[0]);
    expect(names).toContain('engine.initializeRuntime');
    expect(names).toContain('engine.executeBundle');
  });

  it('throws ExecutionError for runtime errors', async () => {
    const engine = new Engine({ quickjs: provider, debug: false });
    const src = buildBundle(`throw new Error('boom')`);
    await expect(engine.loadBundle(src)).rejects.toThrow('boom');
  });
});
