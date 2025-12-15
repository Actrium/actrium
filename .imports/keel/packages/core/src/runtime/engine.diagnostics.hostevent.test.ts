import { describe, expect, it } from 'bun:test';

// Minimal provider stub to avoid DefaultJSEngineProvider side effects in unit tests.
class MockContext {
  eval(_code: string): unknown {
    return undefined;
  }
  evalAsync?(_code: string): Promise<unknown> {
    return Promise.resolve(undefined);
  }
  setGlobal(_name: string, _value: unknown): void {}
  getGlobal(_name: string): unknown {
    return undefined;
  }
  dispose(): void {}
}
class MockRuntime {
  createContext() {
    return new MockContext();
  }
  dispose(): void {}
}
const MockQuickJSProvider = {
  createRuntime() {
    return new MockRuntime();
  },
};

import { Engine } from './engine';

describe('Engine diagnostics - host events', () => {
  it('records last host→guest event info', () => {
    const engine = new Engine({ quickjs: MockQuickJSProvider as any, debug: false });

    engine.sendEvent('HOST_VISIBILITY', { visible: false } as any);

    const d = engine.getDiagnostics();
    expect(d.host.lastEventName).toBe('HOST_VISIBILITY');
    expect(typeof d.host.lastEventAt).toBe('number');
    expect(d.host.lastPayloadBytes).not.toBeNull();
  });
});
