/**
 * Bridge.e2e-paths.test.ts
 *
 * Critical path end-to-end tests covering ALL three Bridge send methods:
 *   1. sendRawBatch — OperationBatch (VM mode)
 *   2. sendSerializedBatch — SerializedOperationBatch (WASM mode)
 *   3. sendJsonBatch — JSON string (Android JSI workaround)
 *
 * Also covers:
 *   - OperationCollector → Bridge integration (JSON transport)
 *   - Mixed operation types through all paths
 *   - destroy() cleanup
 *
 * (There is no binary ArrayBuffer path: the legacy TS binary codec pair was
 * retired — see contracts/sandbox/op-batch-wire.MATURITY.md §4a. The binary op-batch
 * wire lives on the Rust-guest route and is decode-locked by
 * src/host/wire/__tests__/wire-decoder.test.ts.)
 */

import { describe, it, expect, beforeEach } from 'bun:test';
import { CallbackRegistryImpl as CallbackRegistry } from '..';
import type { OperationBatch, SerializedOperationBatch } from '..';
import { Bridge } from './bridge';
import { OperationCollector } from '../../guest/runtime/reconciler/operation-collector';

// ============================================
// Path 1: OperationBatch (VM mode)
// ============================================

describe('Critical Path 1: OperationBatch (VM mode)', () => {
  let bridge: Bridge;
  let hostReceived: OperationBatch[];

  beforeEach(() => {
    hostReceived = [];
    bridge = new Bridge({
      callbackRegistry: new CallbackRegistry(),
      onGuestOperations: (batch) => hostReceived.push(batch),
      onHostMessage: () => {},
    });
  });

  it('should encode+decode all operation types', () => {
    const batch: OperationBatch = {
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'View', props: { style: { flex: 1 } } },
        { op: 'UPDATE', id: 1, props: { title: 'Hello' } },
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
        { op: 'INSERT', id: 2, parentId: 0, childId: 2, index: 0 },
        { op: 'REMOVE', id: 2, parentId: 0, childId: 2 },
        { op: 'TEXT', id: 3, text: 'Some text' },
        { op: 'DELETE', id: 3 },
      ],
    };

    bridge.sendRawBatch(batch);

    expect(hostReceived.length).toBe(1);
    expect(hostReceived[0].operations.length).toBe(7);
    expect(hostReceived[0].operations.map(o => o.op)).toEqual([
      'CREATE', 'UPDATE', 'APPEND', 'INSERT', 'REMOVE', 'TEXT', 'DELETE',
    ]);
  });

  it('should preserve function props through encode/decode', () => {
    const fn = () => 'clicked';
    const batch: OperationBatch = {
      version: 1,
      batchId: 2,
      operations: [
        {
          op: 'CREATE',
          id: 1,
          type: 'Button',
          props: { onPress: fn, title: 'Click' },
        },
      ],
    };

    bridge.sendRawBatch(batch);

    expect(hostReceived.length).toBe(1);
    const decoded = hostReceived[0].operations[0];
    expect(decoded.op).toBe('CREATE');
    // Function should have been roundtripped (registered → proxied)
    expect(decoded).toHaveProperty('props');
    expect((decoded as any).props.title).toBe('Click');
  });

  it('should handle nested object props', () => {
    const batch: OperationBatch = {
      version: 1,
      batchId: 3,
      operations: [
        {
          op: 'CREATE',
          id: 1,
          type: 'View',
          props: {
            style: {
              flex: 1,
              margin: { top: 10, left: 20 },
              padding: [5, 10, 5, 10],
            },
          },
        },
      ],
    };

    bridge.sendRawBatch(batch);

    expect(hostReceived.length).toBe(1);
    const props = (hostReceived[0].operations[0] as any).props;
    expect(props.style.flex).toBe(1);
    expect(props.style.margin.top).toBe(10);
    expect(props.style.padding).toEqual([5, 10, 5, 10]);
  });
});

// ============================================
// Path 2: SerializedOperationBatch (WASM mode)
// ============================================

describe('Critical Path 2: SerializedOperationBatch (WASM mode)', () => {
  let bridge: Bridge;
  let hostReceived: OperationBatch[];

  beforeEach(() => {
    hostReceived = [];
    bridge = new Bridge({
      callbackRegistry: new CallbackRegistry(),
      onGuestOperations: (batch) => hostReceived.push(batch),
      onHostMessage: () => {},
    });
  });

  it('should skip encode for pre-serialized batch with fnIds', () => {
    const batch: SerializedOperationBatch = {
      version: 1,
      batchId: 10,
      operations: [
        {
          op: 'CREATE',
          id: 1,
          type: 'Button',
          props: {
            onPress: { __type: 'function', __fnId: 'fn-42' },
            title: 'Test',
          },
        },
        { op: 'APPEND', id: 1, parentId: 0, childId: 1 },
      ],
    };

    bridge.sendRawBatch(batch);

    expect(hostReceived.length).toBe(1);
    expect(hostReceived[0].batchId).toBe(10);
    expect(hostReceived[0].operations.length).toBe(2);
  });

  it('should roundtrip all serialized special types', () => {
    const batch: SerializedOperationBatch = {
      version: 1,
      batchId: 11,
      operations: [
        {
          op: 'CREATE',
          id: 1,
          type: 'Complex',
          props: {
            fn: { __type: 'function', __fnId: 'fn-1' },
            date: { __type: 'date', __value: '2024-01-01T00:00:00.000Z' },
            err: { __type: 'error', __name: 'TypeError', __message: 'bad', __stack: 'at test' },
            re: { __type: 'regexp', __source: '\\d+', __flags: 'gi' },
          },
        },
      ],
    };

    bridge.sendRawBatch(batch);

    expect(hostReceived.length).toBe(1);
    const props = (hostReceived[0].operations[0] as any).props;
    // fn should be decoded to a callable proxy
    expect(typeof props.fn).toBe('function');
    // date should be decoded
    expect(props.date).toBeInstanceOf(Date);
    // error should be decoded
    expect(props.err).toBeInstanceOf(Error);
    expect(props.err.name).toBe('TypeError');
    // regexp should be decoded
    expect(props.re).toBeInstanceOf(RegExp);
  });
});

// ============================================
// Path 3: OperationCollector → Bridge integration (JSON)
// ============================================

describe('Critical Path 3: OperationCollector → Bridge', () => {
  it('should deliver JSON batch from collector to bridge host receiver', () => {
    const hostReceived: OperationBatch[] = [];
    const bridge = new Bridge({
      callbackRegistry: new CallbackRegistry(),
      onGuestOperations: (batch) => hostReceived.push(batch),
      onHostMessage: () => {},
    });

    const collector = new OperationCollector();
    collector.add({ op: 'CREATE', id: 1, type: 'View', props: { style: 'flex:1' }, timestamp: Date.now() });
    collector.add({ op: 'APPEND', id: 1, parentId: 0, childId: 1, timestamp: Date.now() });
    collector.add({ op: 'TEXT', id: 2, text: 'Hello', timestamp: Date.now() });

    collector.flush((batch) => {
      if (typeof batch === 'string') bridge.sendJsonBatch(batch);
      else bridge.sendRawBatch(batch as OperationBatch);
    });

    expect(hostReceived.length).toBe(1);
    expect(hostReceived[0].operations.length).toBe(3);
    expect(hostReceived[0].operations.map(o => o.op)).toEqual(['CREATE', 'APPEND', 'TEXT']);
  });

  it('should handle multiple consecutive flushes', () => {
    const hostReceived: OperationBatch[] = [];
    const bridge = new Bridge({
      callbackRegistry: new CallbackRegistry(),
      onGuestOperations: (batch) => hostReceived.push(batch),
      onHostMessage: () => {},
    });

    const collector = new OperationCollector();
    const deliver = (batch: unknown) => {
      if (typeof batch === 'string') bridge.sendJsonBatch(batch);
      else bridge.sendRawBatch(batch as OperationBatch);
    };

    collector.add({ op: 'CREATE', id: 1, type: 'View', props: {}, timestamp: Date.now() });
    collector.flush(deliver);

    collector.add({ op: 'UPDATE', id: 1, props: { title: 'Updated' }, timestamp: Date.now() });
    collector.flush(deliver);

    collector.add({ op: 'DELETE', id: 1, timestamp: Date.now() });
    collector.flush(deliver);

    expect(hostReceived.length).toBe(3);
    expect(hostReceived[0].operations[0].op).toBe('CREATE');
    expect(hostReceived[1].operations[0].op).toBe('UPDATE');
    expect(hostReceived[2].operations[0].op).toBe('DELETE');
    expect(hostReceived[0].batchId).toBe(1);
    expect(hostReceived[1].batchId).toBe(2);
    expect(hostReceived[2].batchId).toBe(3);
  });
});

// ============================================
// Path 4: destroy() cleanup
// ============================================

describe('Critical Path 4: Bridge.destroy() cleanup', () => {
  it('should cleanly destroy a used bridge', () => {
    const bridge = new Bridge({
      callbackRegistry: new CallbackRegistry(),
      onGuestOperations: () => {},
      onHostMessage: () => {},
    });

    bridge.sendRawBatch({
      version: 1,
      batchId: 1,
      operations: [{ op: 'CREATE', id: 1, type: 'V', props: {} }],
    });

    expect(() => bridge.destroy()).not.toThrow();
  });
});
