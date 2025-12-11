/**
 * DevTools unit tests
 */

import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';
import {
  ComponentInspector,
  OperationLogger,
  TimelineRecorder,
  DevTools,
  createDevTools,
} from './index';
import type { NodeInstance, OperationBatch } from '../types';

// Helper to wait for real time
const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

// ============ ComponentInspector 测试 ============

describe('ComponentInspector', () => {
  let inspector: ComponentInspector;

  beforeEach(() => {
    inspector = new ComponentInspector();
  });

  describe('buildTree', () => {
    it('should build tree from empty node map', () => {
      const nodeMap = new Map<number, NodeInstance>();
      const tree = inspector.buildTree(nodeMap, []);

      expect(tree).toEqual([]);
    });

    it('should build single node tree', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: { testID: 'root' }, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree).toHaveLength(1);
      expect(tree[0].id).toBe(1);
      expect(tree[0].type).toBe('View');
      expect(tree[0].depth).toBe(0);
    });

    it('should build nested tree', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [2, 3] }],
        [2, { id: 2, type: 'Text', props: { text: 'Hello' }, children: [] }],
        [3, { id: 3, type: 'Text', props: { text: 'World' }, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree).toHaveLength(1);
      expect(tree[0].children).toHaveLength(2);
      expect(tree[0].children[0].depth).toBe(1);
    });

    it('should respect maxDepth', () => {
      inspector = new ComponentInspector({ maxDepth: 1 });

      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [2] }],
        [2, { id: 2, type: 'View', props: {}, children: [3] }],
        [3, { id: 3, type: 'View', props: {}, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree[0].children).toHaveLength(1);
      expect(tree[0].children[0].children).toHaveLength(0); // 深度限制
    });

    it('should filter specified props', () => {
      inspector = new ComponentInspector({ filterProps: ['style', 'secret'] });

      const nodeMap = new Map<number, NodeInstance>([
        [
          1,
          {
            id: 1,
            type: 'View',
            props: { style: { flex: 1 }, testID: 'test', secret: '123' },
            children: [],
          },
        ],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree[0].props.style).toBe('[filtered]');
      expect(tree[0].props.testID).toBe('test');
      expect(tree[0].props.secret).toBe('[filtered]');
    });

    it('should hide functions by default', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [
          1,
          {
            id: 1,
            type: 'TouchableOpacity',
            props: { onPress: () => {}, testID: 'btn' },
            children: [],
          },
        ],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree[0].props.onPress).toBeUndefined();
      expect(tree[0].props.testID).toBe('btn');
    });

    it('should show functions when enabled', () => {
      inspector = new ComponentInspector({ showFunctions: true });

      const nodeMap = new Map<number, NodeInstance>([
        [
          1,
          {
            id: 1,
            type: 'TouchableOpacity',
            props: { onPress: () => {} },
            children: [],
          },
        ],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree[0].props.onPress).toBe('[Function]');
    });

    it('should convert serialized functions to [Callback]', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [
          1,
          {
            id: 1,
            type: 'TouchableOpacity',
            props: { onPress: { __type: 'function', __fnId: 'fn_1' } },
            children: [],
          },
        ],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree[0].props.onPress).toBe('[Callback]');
    });
  });

  describe('recordChange', () => {
    it('should highlight changed nodes', () => {
      inspector.recordChange(1);

      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree[0].highlighted).toBe(true);
    });

    it('should clear highlight after timeout', async () => {
      // Create inspector with short highlight duration for testing
      const testInspector = new ComponentInspector({ highlightDuration: 20 });
      testInspector.recordChange(1);

      // Verify it's highlighted initially
      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [] }],
      ]);
      expect(testInspector.buildTree(nodeMap, [1])[0].highlighted).toBe(true);

      // Wait for highlight to clear
      await sleep(30);

      const tree = testInspector.buildTree(nodeMap, [1]);
      expect(tree[0].highlighted).toBe(false);
    });

    it('should not highlight when disabled', () => {
      inspector = new ComponentInspector({ highlightChanges: false });
      inspector.recordChange(1);

      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);

      expect(tree[0].highlighted).toBe(false);
    });
  });

  describe('toText', () => {
    it('should generate text representation', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: { testID: 'root' }, children: [2] }],
        [2, { id: 2, type: 'Text', props: { text: 'Hello' }, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);
      const text = inspector.toText(tree);

      expect(text).toContain('<View');
      expect(text).toContain('<Text');
      expect(text).toContain('testID="root"');
    });

    it('should show highlighting marker', () => {
      inspector.recordChange(1);

      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);
      const text = inspector.toText(tree);

      expect(text).toContain('*');
    });
  });

  describe('toJSON', () => {
    it('should generate JSON representation', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [] }],
      ]);
      const tree = inspector.buildTree(nodeMap, [1]);
      const json = inspector.toJSON(tree);

      expect(() => JSON.parse(json)).not.toThrow();
      expect(JSON.parse(json)[0].type).toBe('View');
    });
  });
});

// ============ OperationLogger 测试 ============

describe('OperationLogger', () => {
  let logger: OperationLogger;

  beforeEach(() => {
    logger = new OperationLogger(10);
  });

  describe('log', () => {
    it('should log operation batch', () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: {} }],
      };

      logger.log(batch);

      expect(logger.getLogs()).toHaveLength(1);
      expect(logger.getLogs()[0].batchId).toBe(1);
    });

    it('should include duration when provided', () => {
      const batch: OperationBatch = {
        version: 1,
        batchId: 1,
        operations: [],
      };

      logger.log(batch, 50);

      expect(logger.getLogs()[0].duration).toBe(50);
    });

    it('should not log when disabled', () => {
      logger.setEnabled(false);

      logger.log({
        version: 1,
        batchId: 1,
        operations: [],
      });

      expect(logger.getLogs()).toHaveLength(0);
    });

    it('should respect max logs limit', () => {
      for (let i = 0; i < 15; i++) {
        logger.log({
          version: 1,
          batchId: i,
          operations: [],
        });
      }

      expect(logger.getLogs()).toHaveLength(10);
      expect(logger.getLogs()[0].batchId).toBe(5); // 前 5 个被移除
    });
  });

  describe('getRecentLogs', () => {
    it('should return recent N logs', () => {
      for (let i = 0; i < 5; i++) {
        logger.log({ version: 1, batchId: i, operations: [] });
      }

      const recent = logger.getRecentLogs(3);

      expect(recent).toHaveLength(3);
      expect(recent[0].batchId).toBe(2);
    });
  });

  describe('filterByType', () => {
    it('should filter logs by operation type', () => {
      logger.log({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'UPDATE', id: 1, props: {} },
        ],
      });
      logger.log({
        version: 1,
        batchId: 2,
        operations: [{ op: 'DELETE', id: 1 }],
      });

      const creates = logger.filterByType('CREATE');
      const updates = logger.filterByType('UPDATE');
      const deletes = logger.filterByType('DELETE');

      expect(creates).toHaveLength(1);
      expect(updates).toHaveLength(1);
      expect(deletes).toHaveLength(1);
    });
  });

  describe('filterByNodeId', () => {
    it('should filter logs by node ID', () => {
      logger.log({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'CREATE', id: 2, type: 'Text', props: {} },
        ],
      });

      const node1Ops = logger.filterByNodeId(1);
      const node2Ops = logger.filterByNodeId(2);

      expect(node1Ops).toHaveLength(1);
      expect(node2Ops).toHaveLength(1);
    });
  });

  describe('getStats', () => {
    it('should calculate statistics', () => {
      logger.log({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'UPDATE', id: 1, props: {} },
        ],
      });
      logger.log({
        version: 1,
        batchId: 2,
        operations: [{ op: 'DELETE', id: 1 }],
      });

      const stats = logger.getStats();

      expect(stats.totalLogs).toBe(2);
      expect(stats.totalOperations).toBe(3);
      expect(stats.avgOperationsPerBatch).toBe(1.5);
      expect(stats.operationCounts.CREATE).toBe(1);
      expect(stats.operationCounts.UPDATE).toBe(1);
      expect(stats.operationCounts.DELETE).toBe(1);
    });
  });

  describe('clear', () => {
    it('should clear all logs', () => {
      logger.log({ version: 1, batchId: 1, operations: [] });
      logger.clear();

      expect(logger.getLogs()).toHaveLength(0);
    });
  });

  describe('export', () => {
    it('should export logs as JSON', () => {
      logger.log({ version: 1, batchId: 1, operations: [] });

      const exported = logger.export();
      const parsed = JSON.parse(exported);

      expect(parsed.exportTime).toBeDefined();
      expect(parsed.stats).toBeDefined();
      expect(parsed.logs).toHaveLength(1);
    });
  });
});

// ============ TimelineRecorder 测试 ============

describe('TimelineRecorder', () => {
  let recorder: TimelineRecorder;

  beforeEach(() => {
    recorder = new TimelineRecorder(50);
  });

  describe('record', () => {
    it('should record events', () => {
      recorder.record('mount', { nodeId: 1, type: 'View' });

      const events = recorder.getEvents();

      expect(events).toHaveLength(1);
      expect(events[0].type).toBe('mount');
      expect(events[0].data.nodeId).toBe(1);
    });

    it('should include duration when provided', () => {
      recorder.record('batch', { batchId: 1 }, 50);

      expect(recorder.getEvents()[0].duration).toBe(50);
    });

    it('should not record when disabled', () => {
      recorder.setEnabled(false);
      recorder.record('mount', {});

      expect(recorder.getEvents()).toHaveLength(0);
    });

    it('should respect max events limit', () => {
      for (let i = 0; i < 60; i++) {
        recorder.record('update', { i });
      }

      expect(recorder.getEvents()).toHaveLength(50);
    });
  });

  describe('recordMount', () => {
    it('should record mount event', () => {
      recorder.recordMount(1, 'View');

      const events = recorder.getEventsByType('mount');

      expect(events).toHaveLength(1);
      expect(events[0].data.nodeId).toBe(1);
      expect(events[0].data.type).toBe('View');
    });
  });

  describe('recordUpdate', () => {
    it('should record update event', () => {
      recorder.recordUpdate(1, ['style', 'testID']);

      const events = recorder.getEventsByType('update');

      expect(events).toHaveLength(1);
      expect(events[0].data.changedProps).toEqual(['style', 'testID']);
    });
  });

  describe('recordUnmount', () => {
    it('should record unmount event', () => {
      recorder.recordUnmount(1);

      const events = recorder.getEventsByType('unmount');

      expect(events).toHaveLength(1);
      expect(events[0].data.nodeId).toBe(1);
    });
  });

  describe('recordBatch', () => {
    it('should record batch event', () => {
      recorder.recordBatch(1, 5, 10);

      const events = recorder.getEventsByType('batch');

      expect(events).toHaveLength(1);
      expect(events[0].data.batchId).toBe(1);
      expect(events[0].data.operationCount).toBe(5);
      expect(events[0].duration).toBe(10);
    });
  });

  describe('recordCallback', () => {
    it('should record callback event', () => {
      recorder.recordCallback('fn_1', [1, 'test']);

      const events = recorder.getEventsByType('callback');

      expect(events).toHaveLength(1);
      expect(events[0].data.fnId).toBe('fn_1');
      expect(events[0].data.argCount).toBe(2);
    });
  });

  describe('recordHostEvent', () => {
    it('should record host event', () => {
      recorder.recordHostEvent('REFRESH', { force: true });

      const events = recorder.getEventsByType('event');

      expect(events).toHaveLength(1);
      expect(events[0].data.eventName).toBe('REFRESH');
      expect(events[0].data.hasPayload).toBe(true);
    });
  });

  describe('getEventsInRange', () => {
    it('should return events in time range', () => {
      recorder.record('mount', { id: 1 });
      recorder.record('update', { id: 1 });
      recorder.record('unmount', { id: 1 });

      const events = recorder.getEvents();
      const startTs = events[0].timestamp;
      const endTs = events[1].timestamp;

      const rangeEvents = recorder.getEventsInRange(startTs, endTs);

      expect(rangeEvents.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('reset', () => {
    it('should clear events and reset start time', () => {
      recorder.record('mount', {});
      recorder.reset();

      expect(recorder.getEvents()).toHaveLength(0);
    });
  });

  describe('export', () => {
    it('should export timeline as JSON', () => {
      recorder.record('mount', { nodeId: 1 });

      const exported = recorder.export();
      const parsed = JSON.parse(exported);

      expect(parsed.exportTime).toBeDefined();
      expect(parsed.startTime).toBeDefined();
      expect(parsed.events).toHaveLength(1);
    });
  });
});

// ============ DevTools 测试 ============

describe('DevTools', () => {
  let devtools: DevTools;

  beforeEach(() => {
    devtools = new DevTools();
  });

  describe('enable/disable', () => {
    it('should be disabled by default', () => {
      expect(devtools.isEnabled()).toBe(false);
    });

    it('should enable all tools', () => {
      devtools.enable();

      expect(devtools.isEnabled()).toBe(true);
    });

    it('should disable all tools', () => {
      devtools.enable();
      devtools.disable();

      expect(devtools.isEnabled()).toBe(false);
    });
  });

  describe('onBatch', () => {
    it('should not process when disabled', () => {
      devtools.onBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: {} }],
      });

      expect(devtools.logger.getLogs()).toHaveLength(0);
    });

    it('should log batch when enabled', () => {
      devtools.enable();

      devtools.onBatch({
        version: 1,
        batchId: 1,
        operations: [{ op: 'CREATE', id: 1, type: 'View', props: {} }],
      });

      expect(devtools.logger.getLogs()).toHaveLength(1);
    });

    it('should record timeline events', () => {
      devtools.enable();

      devtools.onBatch({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'UPDATE', id: 1, props: { style: {} } },
        ],
      });

      const events = devtools.timeline.getEvents();

      expect(events.filter((e) => e.type === 'batch')).toHaveLength(1);
      expect(events.filter((e) => e.type === 'mount')).toHaveLength(1);
      expect(events.filter((e) => e.type === 'update')).toHaveLength(1);
    });
  });

  describe('onCallback', () => {
    it('should record callback when enabled', () => {
      devtools.enable();
      devtools.onCallback('fn_1', [1, 2]);

      const events = devtools.timeline.getEventsByType('callback');

      expect(events).toHaveLength(1);
    });
  });

  describe('onHostEvent', () => {
    it('should record host event when enabled', () => {
      devtools.enable();
      devtools.onHostEvent('REFRESH');

      const events = devtools.timeline.getEventsByType('event');

      expect(events).toHaveLength(1);
    });
  });

  describe('getComponentTree', () => {
    it('should build component tree', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [] }],
      ]);

      const tree = devtools.getComponentTree(nodeMap, [1]);

      expect(tree).toHaveLength(1);
      expect(tree[0].type).toBe('View');
    });
  });

  describe('getComponentTreeText', () => {
    it('should return text representation', () => {
      const nodeMap = new Map<number, NodeInstance>([
        [1, { id: 1, type: 'View', props: {}, children: [] }],
      ]);

      const text = devtools.getComponentTreeText(nodeMap, [1]);

      expect(text).toContain('View');
    });
  });

  describe('exportAll', () => {
    it('should export all debug data', () => {
      devtools.enable();
      devtools.onBatch({
        version: 1,
        batchId: 1,
        operations: [],
      });

      const exported = devtools.exportAll();
      const parsed = JSON.parse(exported);

      expect(parsed.exportTime).toBeDefined();
      expect(parsed.logs).toBeDefined();
      expect(parsed.timeline).toBeDefined();
    });
  });

  describe('reset', () => {
    it('should reset all tools', () => {
      devtools.enable();
      devtools.onBatch({ version: 1, batchId: 1, operations: [] });

      devtools.reset();

      expect(devtools.logger.getLogs()).toHaveLength(0);
      expect(devtools.timeline.getEvents()).toHaveLength(0);
    });
  });
});

// ============ createDevTools 测试 ============

describe('createDevTools', () => {
  it('should create DevTools with default config', () => {
    const devtools = createDevTools();

    expect(devtools).toBeInstanceOf(DevTools);
    expect(devtools.isEnabled()).toBe(false);
  });

  it('should create DevTools with custom config', () => {
    const devtools = createDevTools({
      inspector: { maxDepth: 5 },
      maxLogs: 50,
      maxTimelineEvents: 200,
    });

    expect(devtools).toBeInstanceOf(DevTools);
  });
});
