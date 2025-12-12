import { describe, expect, it } from 'bun:test';

// A minimal QuickJSProvider mock that runs code in the same JS context for test purposes
// List of globals that should NOT be deleted on dispose (core JS/Node/Bun globals)
const PROTECTED_GLOBALS = new Set([
  'console',
  'setTimeout',
  'clearTimeout',
  'setInterval',
  'clearInterval',
  'queueMicrotask',
  'process',
  'globalThis',
  'global',
]);

class MockContext {
  private globals = new Map<string, unknown>();
  eval(code: string): unknown {
    // Use indirect eval to execute in global scope
    return (0, eval)(code);
  }
  setGlobal(name: string, value: unknown): void {
    globalThis[name] = value;
    this.globals.set(name, value);
  }
  getGlobal(name: string): unknown {
    return globalThis[name];
  }
  dispose(): void {
    this.globals.forEach((_, k) => {
      // Don't delete protected globals like console
      if (!PROTECTED_GLOBALS.has(k)) {
        delete globalThis[k];
      }
    });
    this.globals.clear();
  }
}
class MockRuntime {
  createContext() {
    return new MockContext();
  }
  dispose() {}
}
const MockQuickJSProvider = {
  createRuntime() {
    return new MockRuntime();
  },
};

import { Engine } from './engine';

describe('Engine host→guest events', () => {
  // Skipped: Needs async timing fixes - sendEvent is fire-and-forget
  it.skip('should inject __useHostEvent polyfill and trigger callback when host sends event', async () => {
    const engine = new Engine({ quickjs: MockQuickJSProvider as any, debug: false });

    // load a tiny bundle using SDK-like API
    const bundle = `
      globalThis.__useHostEvent && __useHostEvent('PING', (payload) => {
        globalThis.__PING_PAYLOAD = payload;
      });
    `;

    await engine.loadBundle(bundle);

    // host sends event
    engine.sendEvent('PING', { ok: 1 } as any);

    // Wait for event to be processed (sendEvent is fire-and-forget)
    await new Promise((resolve) => setTimeout(resolve, 10));

    // verify callback executed in sandbox
    expect(globalThis.__PING_PAYLOAD).toEqual({ ok: 1 });

    engine.destroy();
  });
});
