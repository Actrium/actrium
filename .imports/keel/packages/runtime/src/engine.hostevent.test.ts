import { describe, expect, it } from 'bun:test';
import { Engine } from './engine';
import { createMockJSEngineProvider } from './test-utils';

describe('Engine host→guest events', () => {
  // Skipped: Needs async timing fixes - sendEvent is fire-and-forget
  it.skip('should inject __useHostEvent polyfill and trigger callback when host sends event', async () => {
    const engine = new Engine({ quickjs: createMockJSEngineProvider(), debug: false });

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
