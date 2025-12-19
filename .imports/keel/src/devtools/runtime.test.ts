import { describe, test, expect, beforeEach } from 'bun:test';
import { RuntimeCollector, createRuntimeCollector } from './runtime';

describe('RuntimeCollector', () => {
  let collector: RuntimeCollector;

  beforeEach(() => {
    collector = createRuntimeCollector();
  });

  describe('enable/disable', () => {
    test('should be disabled by default', () => {
      expect(collector.isEnabled()).toBe(false);
    });

    test('should enable and disable', () => {
      collector.enable();
      expect(collector.isEnabled()).toBe(true);

      collector.disable();
      expect(collector.isEnabled()).toBe(false);
    });
  });

  describe('logBatch', () => {
    test('should not log when disabled', () => {
      collector.logBatch({ batchId: 1, operations: [{ op: 'CREATE', id: 1, type: 'View' }] });
      expect(collector.getLogs()).toHaveLength(0);
    });

    test('should log batch when enabled', () => {
      collector.enable();
      collector.logBatch({ batchId: 1, operations: [{ op: 'CREATE', id: 1, type: 'View' }] });

      const logs = collector.getLogs();
      expect(logs).toHaveLength(1);
      expect(logs[0].batchId).toBe(1);
      expect(logs[0].operations).toHaveLength(1);
    });

    test('should include duration', () => {
      collector.enable();
      collector.logBatch({ batchId: 1, operations: [] }, 5.5);

      const logs = collector.getLogs();
      expect(logs[0].duration).toBe(5.5);
    });

    test('should respect maxLogs config', () => {
      const c = createRuntimeCollector({ maxLogs: 2 });
      c.enable();

      c.logBatch({ batchId: 1, operations: [] });
      c.logBatch({ batchId: 2, operations: [] });
      c.logBatch({ batchId: 3, operations: [] });

      const logs = c.getLogs();
      expect(logs).toHaveLength(2);
      expect(logs[0].batchId).toBe(2);
      expect(logs[1].batchId).toBe(3);
    });
  });

  describe('getRecentLogs', () => {
    test('should return last N logs', () => {
      collector.enable();
      collector.logBatch({ batchId: 1, operations: [] });
      collector.logBatch({ batchId: 2, operations: [] });
      collector.logBatch({ batchId: 3, operations: [] });

      const recent = collector.getRecentLogs(2);
      expect(recent).toHaveLength(2);
      expect(recent[0].batchId).toBe(2);
      expect(recent[1].batchId).toBe(3);
    });
  });

  describe('getOperationStats', () => {
    test('should compute operation statistics', () => {
      collector.enable();
      collector.logBatch({
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View' },
          { op: 'CREATE', id: 2, type: 'Text' },
          { op: 'APPEND', id: 1, parentId: 0 },
        ],
      });
      collector.logBatch({
        batchId: 2,
        operations: [{ op: 'UPDATE', id: 1, props: { style: {} } }],
      });

      const stats = collector.getOperationStats();
      expect(stats.totalLogs).toBe(2);
      expect(stats.totalOperations).toBe(4);
      expect(stats.operationCounts['CREATE']).toBe(2);
      expect(stats.operationCounts['APPEND']).toBe(1);
      expect(stats.operationCounts['UPDATE']).toBe(1);
      expect(stats.avgOperationsPerBatch).toBe(2);
    });
  });

  describe('timeline', () => {
    test('should record batch events', () => {
      collector.enable();
      collector.logBatch({ batchId: 1, operations: [{ op: 'CREATE' }] }, 2.5);

      const timeline = collector.getTimeline();
      expect(timeline).toHaveLength(1);
      expect(timeline[0].type).toBe('batch');
      expect(timeline[0].data).toEqual({ batchId: 1, operationCount: 1 });
      expect(timeline[0].duration).toBe(2.5);
    });

    test('should record callback events', () => {
      collector.enable();
      collector.recordCallback('fn_123', [1, 2, 3]);

      const timeline = collector.getTimeline();
      expect(timeline).toHaveLength(1);
      expect(timeline[0].type).toBe('callback');
      expect(timeline[0].data).toEqual({ fnId: 'fn_123', argCount: 3 });
    });

    test('should record host events', () => {
      collector.enable();
      collector.recordHostEvent('onPress', { x: 10 });

      const timeline = collector.getTimeline();
      expect(timeline).toHaveLength(1);
      expect(timeline[0].type).toBe('event');
      expect(timeline[0].data).toEqual({ eventName: 'onPress', hasPayload: true });
    });

    test('should respect maxTimelineEvents config', () => {
      const c = createRuntimeCollector({ maxTimelineEvents: 2 });
      c.enable();

      c.recordCallback('fn_1', []);
      c.recordCallback('fn_2', []);
      c.recordCallback('fn_3', []);

      const timeline = c.getTimeline();
      expect(timeline).toHaveLength(2);
    });
  });

  describe('buildTree', () => {
    test('should build tree from node map', () => {
      const nodeMap = new Map([
        [1, { id: 1, type: 'View', props: { style: {} }, children: [2, 3] }],
        [2, { id: 2, type: 'Text', props: { text: 'Hello' }, children: [] }],
        [3, { id: 3, type: 'Text', props: { text: 'World' }, children: [] }],
      ]);

      const tree = collector.buildTree(nodeMap, [1]);
      expect(tree).toHaveLength(1);
      expect(tree[0].type).toBe('View');
      expect(tree[0].children).toHaveLength(2);
      expect(tree[0].children[0].type).toBe('Text');
      expect(tree[0].depth).toBe(0);
      expect(tree[0].children[0].depth).toBe(1);
    });

    test('should filter callback props', () => {
      const nodeMap = new Map([
        [
          1,
          {
            id: 1,
            type: 'Button',
            props: { onPress: { __type: 'function', __fnId: 'fn_1' }, title: 'Click' },
            children: [],
          },
        ],
      ]);

      const tree = collector.buildTree(nodeMap, [1]);
      expect(tree[0].props['onPress']).toBe('[Callback]');
      expect(tree[0].props['title']).toBe('Click');
    });

    test('should handle missing nodes', () => {
      const nodeMap = new Map([[1, { id: 1, type: 'View', props: {}, children: [999] }]]);

      const tree = collector.buildTree(nodeMap, [1]);
      expect(tree[0].children).toHaveLength(0);
    });

    test('should limit depth to prevent infinite recursion', () => {
      // Create a very deep tree
      const nodeMap = new Map<
        number,
        { id: number; type: string; props: {}; children: number[] }
      >();
      for (let i = 0; i < 100; i++) {
        nodeMap.set(i, { id: i, type: 'View', props: {}, children: i < 99 ? [i + 1] : [] });
      }

      const tree = collector.buildTree(nodeMap, [0]);
      // Should stop at depth 50
      let node = tree[0];
      let depth = 0;
      while (node && node.children.length > 0) {
        node = node.children[0];
        depth++;
      }
      expect(depth).toBeLessThanOrEqual(50);
    });
  });

  describe('treeToText', () => {
    test('should format tree as text', () => {
      const tree = [
        {
          id: 1,
          type: 'View',
          props: { style: {} },
          children: [{ id: 2, type: 'Text', props: { text: 'Hello' }, children: [], depth: 1 }],
          depth: 0,
        },
      ];

      const text = collector.treeToText(tree);
      expect(text).toContain('<View');
      expect(text).toContain('<Text');
      expect(text).toContain('└─');
    });
  });

  describe('sandboxStatus', () => {
    test('should track sandbox status', () => {
      collector.updateSandboxStatus({ state: 'running' });

      const status = collector.getSandboxStatus();
      expect(status.state).toBe('running');
      expect(status.startTime).toBeDefined();
      expect(status.lastActivityTime).toBeDefined();
    });

    test('should count errors', () => {
      collector.recordSandboxError();
      collector.recordSandboxError();

      const status = collector.getSandboxStatus();
      expect(status.errorCount).toBe(2);
    });
  });

  describe('getMetrics', () => {
    test('should return metrics', () => {
      collector.enable();
      collector.logBatch({ batchId: 1, operations: [] }, 5);
      collector.logBatch({ batchId: 2, operations: [] }, 10);

      const nodeMap = new Map([[1, { id: 1, type: 'View', props: {}, children: [] }]]);
      const metrics = collector.getMetrics(nodeMap);

      expect(metrics.operationProcessingTime).toBe(7.5);
      expect(metrics.nodeCount).toBe(1);
    });
  });

  describe('clear', () => {
    test('should clear all data', () => {
      collector.enable();
      collector.logBatch({ batchId: 1, operations: [] });
      collector.recordCallback('fn_1', []);

      collector.clear();

      expect(collector.getLogs()).toHaveLength(0);
      expect(collector.getTimeline()).toHaveLength(0);
    });
  });
});
