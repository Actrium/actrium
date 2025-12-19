import { describe, expect, it } from 'bun:test';
import { OperationMerger } from './performance';

function ops(arr: any[]): any[] {
  return arr as any[];
}

describe('OperationMerger advanced rules', () => {
  it('should drop CREATE then DELETE of same id within batch', () => {
    const merger = new OperationMerger();
    const merged = merger.merge(
      ops([
        { op: 'CREATE', id: 1, type: 'View', props: {} },
        { op: 'DELETE', id: 1 },
      ])
    );
    expect(merged.find((o) => o.op === 'CREATE')).toBeUndefined();
    expect(merged.find((o) => o.op === 'DELETE')).toBeUndefined();
  });

  it('should keep only last INSERT per child', () => {
    const merger = new OperationMerger();
    const merged = merger.merge(
      ops([
        { op: 'INSERT', id: 2, parentId: 10, childId: 5, index: 0 },
        { op: 'INSERT', id: 3, parentId: 10, childId: 5, index: 2 },
      ])
    );
    const inserts = merged.filter((o) => o.op === 'INSERT');
    expect(inserts.length).toBe(1);
    expect(inserts[0].index).toBe(2);
  });

  it('should keep only last REORDER per parent', () => {
    const merger = new OperationMerger();
    const merged = merger.merge(
      ops([
        { op: 'REORDER', id: 4, parentId: 20, childIds: [1, 2, 3] },
        { op: 'REORDER', id: 5, parentId: 20, childIds: [3, 2, 1] },
      ])
    );
    const reorders = merged.filter((o) => o.op === 'REORDER');
    expect(reorders.length).toBe(1);
    expect(reorders[0].childIds).toEqual([3, 2, 1]);
  });
});
