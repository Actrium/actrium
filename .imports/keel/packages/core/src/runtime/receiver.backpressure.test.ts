import { describe, it, expect } from 'bun:test';
import { Receiver } from './receiver';
import { createRegistry } from './registry';

function makeBatch(total: number){
  return { version: 1, batchId: 1, operations: Array.from({length: total}, (_, i) => ({ op: 'CREATE', id: i+1, type: 'View', props: {} })) } as any;
}

describe('Receiver backpressure', () => {
  it('should send backpressure event when operations exceed limit', () => {
    const registry = createRegistry();
    const sent: any[] = [];
    const recv = new Receiver(registry, (m) => sent.push(m), () => {}, { maxBatchSize: 10 });

    recv.applyBatch(makeBatch(25));

    const bp = sent.find(m => m.type === 'HOST_EVENT' && m.eventName === 'RECEIVER_BACKPRESSURE');
    expect(bp).toBeTruthy();
    expect(bp.payload.skipped).toBe(15);
  });
});
