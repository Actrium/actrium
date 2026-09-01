import { describe, expect, it } from 'bun:test';
import { Receiver } from '../../receiver';
import { ComponentRegistry } from '../../registry';

function makeReceiver() {
  const registry = new ComponentRegistry();
  return new Receiver(
    // biome-ignore lint/suspicious/noExplicitAny: Test helper uses minimal registry
    registry as any,
    () => {},
    () => {},
    { debug: false }
  );
}

describe('Receiver - DELETE robustness', () => {
  it('should delete node even if parent mapping is missing (fallback scan)', () => {
    const r = makeReceiver();

    // Build a simple tree: root -> 1 -> 2
    r.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'CREATE', id: 2, type: 'View', props: {} },
        { op: 'APPEND', parentId: 0, childId: 1 },
        { op: 'APPEND', parentId: 1, childId: 2 },
      ],
      // biome-ignore lint/suspicious/noExplicitAny: Test batch with dynamic structure
    } as any);

    // Simulate protocol violation / corrupted index
    // biome-ignore lint/suspicious/noExplicitAny: Test accesses internal receiver state
    (r as any).parentByChildId.delete(2);

    // DELETE without REMOVE should still remove 2 from 1.children via fallback
    r.applyBatch({
      version: 1,
      batchId: 2,
      operations: [{ op: 'DELETE', id: 2 }],
      // biome-ignore lint/suspicious/noExplicitAny: Test batch with dynamic structure
    } as any);

    const nodes = r.getNodes().map((n) => n.id);
    expect(nodes.includes(2)).toBe(false);

    const parent = r.getNodes().find((n) => n.id === 1)!;
    expect(parent.children.includes(2)).toBe(false);
  });

  it('clear() should clear parentByChildId', () => {
    const r = makeReceiver();

    r.applyBatch({
      version: 1,
      batchId: 1,
      operations: [
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'APPEND', parentId: 0, childId: 1 },
      ],
      // biome-ignore lint/suspicious/noExplicitAny: Test batch with dynamic structure
    } as any);

    // biome-ignore lint/suspicious/noExplicitAny: Test accesses internal receiver state
    expect((r as any).parentByChildId.size).toBe(1);
    r.clear();
    // biome-ignore lint/suspicious/noExplicitAny: Test accesses internal receiver state
    expect((r as any).parentByChildId.size).toBe(0);
  });
});
