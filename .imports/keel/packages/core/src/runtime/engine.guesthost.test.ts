import { describe, expect, it } from 'bun:test';

class MockContext {
  eval(code: string) {
    return (0, eval)(code);
  }
  setGlobal(n: string, v: unknown) {
    globalThis[n] = v;
  }
  getGlobal(n: string) {
    return globalThis[n];
  }
  dispose() {
    /* noop */
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

describe('Guest→Host message flow', () => {
  it('should deliver __sendEventToHost to engine.on("message")', async () => {
    const engine = new Engine({ quickjs: MockQuickJSProvider as any, debug: false });

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
