import { describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';
import { createMockJSEngineProvider } from '../test-utils';

describe('Engine host→guest events', () => {
  it('should inject __useHostEvent polyfill and trigger callback when host sends event', async () => {
    const engine = new Engine({ quickjs: createMockJSEngineProvider(), debug: false });

    // load a tiny bundle using SDK-like API
    // Note: Must use globalThis.xxx in strict mode (mock provider shadows globalThis)
    const bundle = `
      if (globalThis.__useHostEvent) {
        globalThis.__useHostEvent('PING', (payload) => {
          globalThis.__PING_PAYLOAD = payload;
        });
      }
    `;

    await engine.loadBundle(bundle);

    // host sends event (sendEvent dispatches to sandbox via sendToSandbox)
    // biome-ignore lint/suspicious/noExplicitAny: Test event has dynamic structure
    engine.sendEvent('PING', { ok: 1 } as any);

    // Wait for event to be processed (sendEvent is async/fire-and-forget)
    await new Promise((resolve) => setTimeout(resolve, 50));

    // verify callback executed in sandbox (must use getGlobal, not Host globalThis)
    const payload = engine.context?.getGlobal('__PING_PAYLOAD');
    expect(payload).toEqual({ ok: 1 });

    engine.destroy();
  });
});
