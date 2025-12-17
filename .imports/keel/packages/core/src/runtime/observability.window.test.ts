import { describe, expect, it } from 'bun:test';
import { Engine } from './engine';
import { createMockJSEngineProvider } from './test-utils';

describe('Host-side sliding window metrics (example)', () => {
  it('collects last N onMetric events in a sliding window', async () => {
    const provider = createMockJSEngineProvider();
    const window: Array<{ name: string; value: number }> = [];
    const N = 5;
    const engine = new Engine({
      quickjs: provider,
      onMetric: (n, v) => {
        window.push({ name: n, value: v });
        if (window.length > N) window.shift();
      },
    });

    await engine.loadBundle('console.log("ok")');
    engine.createReceiver(() => {});
    engine.sendEvent('PING');
    engine.getReceiver()!.render();

    expect(window.length).toBeGreaterThan(0);
    expect(window.length).toBeLessThanOrEqual(N);
  });
});
