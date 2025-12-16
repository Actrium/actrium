import { describe, expect, it } from 'bun:test';
import type { JSEngineProvider } from './engine';
import { Engine } from './engine';

function createMockProvider(): JSEngineProvider {
  return {
    createRuntime: () => {
      const globals = new Map<string, unknown>();
      return {
        createContext: () => ({
          eval: (code: string) => {
            const globalNames = Array.from(globals.keys());
            const globalValues = Array.from(globals.values());
            // eslint-disable-next-line no-new-func
            const fn = new Function(...globalNames, `"use strict"; ${code}`);
            return fn(...globalValues);
          },
          setGlobal: (name: string, value: unknown) => {
            globals.set(name, value);
          },
          getGlobal: (name: string) => globals.get(name),
          dispose: () => {
            globals.clear();
          },
        }),
        dispose: () => {
          globals.clear();
        },
      };
    },
  };
}

function makeBatch(batchId: number, ops: number) {
  return {
    version: 1,
    batchId,
    operations: Array.from({ length: ops }, (_, i) => ({
      op: 'CREATE' as const,
      id: batchId * 1000 + i + 1,
      type: 'View',
      props: {},
    })),
  };
}

describe('Engine diagnostics timeline', () => {
  it('aggregates ops/skip/apply into activity.timeline buckets', async () => {
    const engine = new Engine({
      provider: createMockProvider(),
      receiverMaxBatchSize: 2,
      diagnostics: { activityHistoryMs: 10_000, activityBucketMs: 1000 },
    });
    await engine.loadBundle('globalThis.__noop = 1;');
    engine.createReceiver(() => {});

    const sendToHost = (engine as unknown as { sendToHostFn: ((b: any) => void) | null })
      .sendToHostFn;
    expect(typeof sendToHost).toBe('function');

    sendToHost!(makeBatch(1, 5));
    sendToHost!(makeBatch(2, 3));

    const d = engine.getDiagnostics();
    const timeline = d.activity.timeline;
    expect(timeline).toBeTruthy();
    expect(timeline!.bucketMs).toBe(1000);
    expect(timeline!.points.length).toBeGreaterThan(0);

    const totalOps = timeline!.points.reduce((sum, p) => sum + p.ops, 0);
    const totalSkipped = timeline!.points.reduce((sum, p) => sum + p.skippedOps, 0);
    expect(totalOps).toBe(8);
    // First batch 5 ops, receiverMaxBatchSize=2 => skipped 3; second batch 3 ops => skipped 1
    expect(totalSkipped).toBe(4);
    expect(timeline!.points.some((p) => p.applyDurationMsAvg != null)).toBe(true);
  });
});
