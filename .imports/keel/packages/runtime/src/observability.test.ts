import { describe, expect, it, mock } from 'bun:test';
import { Engine } from './engine';
import { createMockJSEngineProvider } from './test-utils';

describe('Observability & Health', () => {
  it('tracks errors and exposes getHealth', async () => {
    const provider = createMockJSEngineProvider();
    const engine = new Engine({ quickjs: provider, debug: false });
    await expect(engine.loadBundle('throw new Error("boom")')).rejects.toThrow();
    const health = engine.getHealth();
    expect(health.errorCount).toBeGreaterThan(0);
    expect(typeof health.lastErrorAt === 'number' || health.lastErrorAt === null).toBeTruthy();
  });

  it('emits metrics for sendToSandbox and receiver', async () => {
    const provider = createMockJSEngineProvider();
    const onMetric = mock();
    const engine = new Engine({ quickjs: provider, debug: false, onMetric });
    await engine.loadBundle('console.log("hi")');
    engine.createReceiver(() => {});
    // Use sendToSandbox directly to ensure we wait for the async operation
    await engine.sendToSandbox({ type: 'HOST_EVENT', eventName: 'PING', payload: null });
    // render metrics
    engine.getReceiver()!.render();
    const names = onMetric.mock.calls.map((c) => c[0]);
    expect(names).toContain('engine.sendToSandbox');
    expect(names).toContain('receiver.render');
  });
});
