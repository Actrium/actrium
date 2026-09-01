/**
 * operation-collector.test.ts
 *
 * OperationCollector tests — collect/flush semantics on the JSON transport
 * (the collector's only output; the legacy binary-encoding mode was retired,
 * see contracts/sandbox/op-batch-wire.MATURITY.md §4a).
 */

import { describe, it, expect, beforeEach } from 'bun:test';
import { OperationCollector } from './operation-collector';
import type { SerializedOperationBatch } from '../../../shared/types';

/** Parse flush output (a JSON string). */
function parseBatch(raw: unknown): SerializedOperationBatch {
  if (typeof raw === 'string') return JSON.parse(raw);
  return raw as SerializedOperationBatch;
}

describe('OperationCollector', () => {
  let collector: OperationCollector;

  beforeEach(() => {
    collector = new OperationCollector();
  });

  it('should collect and flush operations', () => {
    collector.add({ op: 'CREATE', id: 1, type: 'View', props: {}, timestamp: Date.now() });
    collector.add({ op: 'APPEND', id: 1, parentId: 0, childId: 1, timestamp: Date.now() });

    let received: unknown = null;
    collector.flush((raw) => { received = raw; });

    expect(received).not.toBeNull();
    const batch = parseBatch(received);
    expect(batch.operations.length).toBe(2);
    expect(batch.batchId).toBe(1);
  });

  it('should not flush empty operations', () => {
    let called = false;
    collector.flush(() => { called = true; });
    expect(called).toBe(false);
  });

  it('should send a JSON string (never a binary buffer)', () => {
    collector.add({ op: 'CREATE', id: 1, type: 'View', props: {}, timestamp: Date.now() });

    let received: unknown = null;
    collector.flush((raw) => { received = raw; });

    // JSON string for JSI stack safety; the collector has no binary output.
    expect(received).not.toBeInstanceOf(ArrayBuffer);
    expect(typeof received).toBe('string');
    const batch = parseBatch(received);
    expect(batch.operations).toHaveLength(1);
  });

  it('should increment batchId', () => {
    collector.add({ op: 'CREATE', id: 1, type: 'View', props: {}, timestamp: Date.now() });
    let batch1: SerializedOperationBatch | null = null;
    collector.flush((b) => { batch1 = parseBatch(b); });

    collector.add({ op: 'DELETE', id: 1, timestamp: Date.now() });
    let batch2: SerializedOperationBatch | null = null;
    collector.flush((b) => { batch2 = parseBatch(b); });

    expect(batch1!.batchId).toBe(1);
    expect(batch2!.batchId).toBe(2);
  });

  it('should report pendingCount', () => {
    expect(collector.pendingCount).toBe(0);
    collector.add({ op: 'CREATE', id: 1, type: 'View', props: {}, timestamp: Date.now() });
    expect(collector.pendingCount).toBe(1);
    collector.add({ op: 'DELETE', id: 1, timestamp: Date.now() });
    expect(collector.pendingCount).toBe(2);
  });
});
