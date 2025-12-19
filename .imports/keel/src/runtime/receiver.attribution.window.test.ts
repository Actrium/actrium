import { describe, expect, it } from 'bun:test';
import { Receiver } from './receiver';
import { createRegistry } from './registry';

function batch1() {
  return {
    batchId: 1,
    operations: [
      { op: 'CREATE', id: 1, type: 'View', props: {} },
      { op: 'CREATE', id: 2, type: 'Text', props: {} },
      { op: 'APPEND', id: 2, parentId: 1, childId: 2 },
      { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
    ],
  } as any;
}

function batch2() {
  return {
    batchId: 2,
    operations: [
      { op: 'UPDATE', id: 2, props: {} },
      { op: 'TEXT', id: 2, text: 'hi' },
    ],
  } as any;
}

describe('Receiver attribution window', () => {
  it('aggregates recent batches into a window summary', () => {
    const registry = createRegistry();
    const receiver = new Receiver(
      registry,
      () => {},
      () => {},
      { attributionWindowMs: 10_000 }
    );

    const realNow = Date.now;
    let now = 0;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (Date as any).now = () => now;
    try {
      receiver.applyBatch(batch1());
      now += 1000;
      receiver.applyBatch(batch2());
      now += 1000;

      const stats = receiver.getStats();
      expect(stats.attribution).not.toBeNull();
      const w = stats.attribution!;

      expect(w.sampleCount).toBe(2);
      expect(w.total).toBe(6);
      expect(w.applied).toBe(6);
      expect(w.skipped).toBe(0);
      expect(w.failed).toBe(0);
      expect(w.nodeDelta).toBe(2);

      expect(w.opCounts.CREATE).toBe(2);
      expect(w.opCounts.APPEND).toBe(2);
      expect(w.opCounts.UPDATE).toBe(1);
      expect(w.opCounts.TEXT).toBe(1);

      // Text: 2(create+append) + 2(update+text) = 4
      // View: 1(create) + 2(append) = 3
      expect(w.topNodeTypes[0]?.type).toBe('Text');
      expect(w.topNodeTypes[0]?.ops).toBe(4);
      expect(w.topNodeTypes[1]?.type).toBe('View');
      expect(w.topNodeTypes[1]?.ops).toBe(3);
    } finally {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (Date as any).now = realNow;
    }
  });

  it('trims attribution history by time window', () => {
    const registry = createRegistry();
    const receiver = new Receiver(
      registry,
      () => {},
      () => {},
      {
        attributionWindowMs: 1000,
        attributionHistoryMs: 1500,
      }
    );

    const realNow = Date.now;
    let now = 0;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (Date as any).now = () => now;
    try {
      receiver.applyBatch(batch1());
      now = 2000; // Exceeds historyMs (1500), triggers trim
      receiver.applyBatch(batch2());

      const stats = receiver.getStats();
      expect(stats.attribution).not.toBeNull();
      const w = stats.attribution!;

      // batch1 has been trimmed, only batch2 remains in window
      expect(w.sampleCount).toBe(1);
      expect(w.total).toBe(2);
      expect(w.opCounts.UPDATE).toBe(1);
      expect(w.opCounts.TEXT).toBe(1);
    } finally {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (Date as any).now = realNow;
    }
  });
});
