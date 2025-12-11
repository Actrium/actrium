import { describe, it, expect, mock } from 'bun:test';
import { Receiver } from './receiver';
import { createRegistry } from './registry';

function makeBatch(n: number) {
  return {
    operations: Array.from({ length: n }, (_, i) => ({ op: 'CREATE', id: i + 1, type: 'View', props: {} as any }))
  } as any;
}

describe('Receiver limits & metrics', () => {
  it('applies at most maxBatchSize operations and reports metrics', () => {
    const registry = createRegistry();
    const onMetric = mock();
    const receiver = new Receiver(registry, () => {}, () => {}, { maxBatchSize: 10, onMetric });
    receiver.applyBatch(makeBatch(25));
    const names = onMetric.mock.calls.map(c => c[0]);
    expect(names).toContain('receiver.applyBatch');
    const last = onMetric.mock.calls.reverse().find(c => c[0] === 'receiver.applyBatch');
    const extra = last?.[2] as any;
    expect(extra.applied).toBe(10);
    expect(extra.skipped).toBe(15);
    expect(extra.total).toBe(25);
  });
});
