import { describe, it, expect, vi } from 'vitest';
import { Receiver } from './receiver';
import { createRegistry } from './registry';
import React from 'react';

describe('Receiver metrics with INSERT/REORDER', () => {
  it('reports render metrics after complex reorder', () => {
    const registry = createRegistry();
    const onMetric = vi.fn();
    const Dummy = (props: any) => React.createElement('div', props, props.children);
    registry.register('View', Dummy as any);

    const receiver = new Receiver(registry, () => {}, () => {}, { onMetric });

    const ops: any[] = [];
    // parent and children
    ops.push({ op: 'CREATE', id: 1, type: 'View', props: {} });
    for (let i = 0; i < 5; i++) {
      ops.push({ op: 'CREATE', id: 10 + i, type: 'View', props: {} });
      ops.push({ op: 'APPEND', parentId: 1, childId: 10 + i });
    }
    // insert a new child at index 2
    ops.push({ op: 'CREATE', id: 99, type: 'View', props: {} });
    ops.push({ op: 'INSERT', parentId: 1, childId: 99, index: 2 });
    // make parent root
    ops.push({ op: 'APPEND', parentId: 0, childId: 1 });
    // reorder children
    ops.push({ op: 'REORDER', parentId: 1, childIds: [99, 14, 13, 12, 11, 10] });

    receiver.applyBatch({ operations: ops } as any);
    receiver.render();

    const names = onMetric.mock.calls.map(c => c[0]);
    expect(names).toContain('receiver.render');
  });
});
