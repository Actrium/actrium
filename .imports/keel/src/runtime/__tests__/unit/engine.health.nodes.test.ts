import { describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';
import { createMockJSEngineProvider } from '../test-utils';

function makeBatch(n: number) {
  return {
    operations: Array.from({ length: n }, (_, i) => ({
      op: 'CREATE',
      id: i + 1,
      type: 'View',
      // biome-ignore lint/suspicious/noExplicitAny: Test node with dynamic structure
      props: {} as any,
    })),
    // biome-ignore lint/suspicious/noExplicitAny: Test data has dynamic structure
  } as any;
}

describe('Engine health receiverNodes', () => {
  it('reflects receiver node count after applyBatch', async () => {
    const provider = createMockJSEngineProvider();
    const engine = new Engine({ quickjs: provider, debug: false });
    await engine.loadBundle('console.log("ok")');
    const receiver = engine.createReceiver(() => {});
    receiver.applyBatch(makeBatch(7));
    const health = engine.getHealth();
    expect(health.receiverNodes).toBe(7);
  });
});
