import { describe, expect, it } from 'bun:test';

// Use isolated scope to prevent global pollution
function createMockProvider() {
  const globals = new Map<string, unknown>();
  const sandboxGlobalThis: Record<string, unknown> = {};

  class MockContext {
    eval(code: string) {
      // Build parameter list: explicit globals + sandboxed globalThis
      const globalNames = Array.from(globals.keys());
      const globalValues = Array.from(globals.values());

      // Wrap code to use sandboxed globalThis instead of real globalThis
      const wrappedCode = `
        "use strict";
        var globalThis = arguments[arguments.length - 1];
        ${code}
      `;

      const fn = new Function(...globalNames, wrappedCode);
      return fn(...globalValues, sandboxGlobalThis);
    }
    setGlobal(n: string, v: unknown) {
      globals.set(n, v);
      sandboxGlobalThis[n] = v;
    }
    getGlobal(n: string) {
      if (n in sandboxGlobalThis) {
        return sandboxGlobalThis[n];
      }
      return globals.get(n);
    }
    dispose() {
      globals.clear();
      for (const key of Object.keys(sandboxGlobalThis)) {
        delete sandboxGlobalThis[key];
      }
    }
  }

  class MockRuntime {
    createContext() {
      return new MockContext();
    }
    dispose() {}
  }

  return {
    createRuntime() {
      return new MockRuntime();
    },
  };
}

import { Engine } from './engine';

describe('Guest→Host message flow', () => {
  it('should deliver __sendEventToHost to engine.on("message")', async () => {
    const mockProvider = createMockProvider();
    const engine = new Engine({ quickjs: mockProvider as any, debug: false });

    const messages: any[] = [];
    engine.on('message', (m) => messages.push(m));

    const bundle = `
      if (typeof __sendEventToHost === 'function') {
        __sendEventToHost('HELLO', { a: 1 });
      }
    `;

    await engine.loadBundle(bundle);

    expect(messages.length).toBe(1);
    expect(messages[0]).toEqual({ event: 'HELLO', payload: { a: 1 } });

    engine.destroy();
  });
});
