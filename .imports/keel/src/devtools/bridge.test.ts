import { describe, test, expect, beforeEach, mock } from 'bun:test';
import { DevToolsBridge, createBridge } from './bridge';
import { createRuntimeCollector } from './runtime';
import type { GuestDebugMessage } from './types';

describe('DevToolsBridge', () => {
  let bridge: DevToolsBridge;

  beforeEach(() => {
    bridge = createBridge();
  });

  describe('enable/disable', () => {
    test('should be disabled by default', () => {
      expect(bridge.isEnabled()).toBe(false);
    });

    test('should enable and disable', () => {
      bridge.enable();
      expect(bridge.isEnabled()).toBe(true);

      bridge.disable();
      expect(bridge.isEnabled()).toBe(false);
    });
  });

  describe('connectRuntime', () => {
    test('should connect runtime collector', () => {
      const collector = createRuntimeCollector();
      const nodeMap = new Map([[1, { id: 1, type: 'View', props: {}, children: [] }]]);

      bridge.connectRuntime(collector, nodeMap, [1]);
      bridge.enable();

      const tree = bridge.getHostTree();
      expect(tree).toHaveLength(1);
      expect(tree[0].type).toBe('View');
    });

    test('should disconnect runtime', () => {
      const collector = createRuntimeCollector();
      const nodeMap = new Map([[1, { id: 1, type: 'View', props: {}, children: [] }]]);

      bridge.connectRuntime(collector, nodeMap, [1]);
      bridge.enable();
      bridge.disconnectRuntime();

      expect(bridge.getHostTree()).toHaveLength(0);
    });
  });

  describe('handleGuestMessage', () => {
    beforeEach(() => {
      bridge.enable();
    });

    test('should handle READY message', () => {
      expect(bridge.isGuestReady()).toBe(false);

      bridge.handleGuestMessage({ type: '__DEVTOOLS_READY__' });

      expect(bridge.isGuestReady()).toBe(true);
    });

    test('should handle CONSOLE message', () => {
      const entry = { level: 'log' as const, args: ['hello'], timestamp: Date.now() };
      bridge.handleGuestMessage({ type: '__DEVTOOLS_CONSOLE__', entry });

      const logs = bridge.getConsoleLogs();
      expect(logs).toHaveLength(1);
      expect(logs[0].args).toEqual(['hello']);
    });

    test('should limit console logs to 100', () => {
      for (let i = 0; i < 110; i++) {
        bridge.handleGuestMessage({
          type: '__DEVTOOLS_CONSOLE__',
          entry: { level: 'log', args: [i], timestamp: Date.now() },
        });
      }

      expect(bridge.getConsoleLogs()).toHaveLength(100);
    });

    test('should handle ERROR message', () => {
      const error = { message: 'Test error', timestamp: Date.now(), fatal: true };
      bridge.handleGuestMessage({ type: '__DEVTOOLS_ERROR__', error });

      const errors = bridge.getErrors();
      expect(errors).toHaveLength(1);
      expect(errors[0].message).toBe('Test error');
    });

    test('should handle RENDER message', () => {
      const handler = mock(() => {});
      bridge.subscribe('render', handler);

      bridge.handleGuestMessage({
        type: '__DEVTOOLS_RENDER__',
        timings: [{ nodeId: 1, type: 'View', phase: 'mount', duration: 5, timestamp: Date.now() }],
        commitDuration: 10,
        timestamp: Date.now(),
      });

      expect(handler).toHaveBeenCalled();
    });

    test('should not handle messages when disabled', () => {
      bridge.disable();
      bridge.handleGuestMessage({ type: '__DEVTOOLS_READY__' });

      expect(bridge.isGuestReady()).toBe(false);
    });
  });

  describe('subscribe', () => {
    test('should emit console events', () => {
      bridge.enable();
      const handler = mock(() => {});
      bridge.subscribe('console', handler);

      bridge.handleGuestMessage({
        type: '__DEVTOOLS_CONSOLE__',
        entry: { level: 'log', args: ['test'], timestamp: Date.now() },
      });

      expect(handler).toHaveBeenCalledTimes(1);
    });

    test('should emit error events', () => {
      bridge.enable();
      const handler = mock(() => {});
      bridge.subscribe('error', handler);

      bridge.handleGuestMessage({
        type: '__DEVTOOLS_ERROR__',
        error: { message: 'error', timestamp: Date.now(), fatal: false },
      });

      expect(handler).toHaveBeenCalledTimes(1);
    });

    test('should unsubscribe', () => {
      bridge.enable();
      const handler = mock(() => {});
      const unsubscribe = bridge.subscribe('console', handler);

      unsubscribe();

      bridge.handleGuestMessage({
        type: '__DEVTOOLS_CONSOLE__',
        entry: { level: 'log', args: ['test'], timestamp: Date.now() },
      });

      expect(handler).not.toHaveBeenCalled();
    });

    test('should emit operation events', () => {
      bridge.enable();
      const handler = mock(() => {});
      bridge.subscribe('operation', handler);

      bridge.recordOperationBatch({
        batchId: 1,
        timestamp: Date.now(),
        operations: [{ op: 'CREATE', id: 1, type: 'View' }],
      });

      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  describe('profiling', () => {
    beforeEach(() => {
      bridge.enable();
    });

    test('should start and stop profiling', () => {
      expect(bridge.isProfiling()).toBe(false);

      bridge.startProfiling();
      expect(bridge.isProfiling()).toBe(true);

      const report = bridge.stopProfiling();
      expect(bridge.isProfiling()).toBe(false);
      expect(report).not.toBeNull();
    });

    test('should collect operations during profiling', () => {
      bridge.startProfiling();

      bridge.recordOperationBatch({
        batchId: 1,
        timestamp: Date.now(),
        operations: [{ op: 'CREATE' }, { op: 'APPEND' }],
        duration: 5,
      });

      const report = bridge.stopProfiling();
      expect(report?.summary.totalOperations).toBe(2);
    });

    test('should collect render timings during profiling', () => {
      bridge.startProfiling();

      bridge.handleGuestMessage({
        type: '__DEVTOOLS_RENDER__',
        timings: [
          { nodeId: 1, type: 'View', phase: 'mount', duration: 10, timestamp: Date.now() },
          { nodeId: 2, type: 'Text', phase: 'mount', duration: 5, timestamp: Date.now() },
        ],
        commitDuration: 15,
        timestamp: Date.now(),
      });

      const report = bridge.stopProfiling();
      expect(report?.summary.totalRenders).toBe(2);
      expect(report?.summary.avgRenderTime).toBe(7.5);
    });

    test('should collect errors during profiling', () => {
      bridge.startProfiling();

      bridge.handleGuestMessage({
        type: '__DEVTOOLS_ERROR__',
        error: { message: 'error', timestamp: Date.now(), fatal: false },
      });

      const report = bridge.stopProfiling();
      expect(report?.summary.errorCount).toBe(1);
    });

    test('should compute slowest nodes', () => {
      bridge.startProfiling();

      bridge.handleGuestMessage({
        type: '__DEVTOOLS_RENDER__',
        timings: [
          { nodeId: 1, type: 'SlowView', phase: 'mount', duration: 20, timestamp: Date.now() },
          { nodeId: 1, type: 'SlowView', phase: 'update', duration: 30, timestamp: Date.now() },
          { nodeId: 2, type: 'FastView', phase: 'mount', duration: 2, timestamp: Date.now() },
        ],
        commitDuration: 52,
        timestamp: Date.now(),
      });

      const report = bridge.stopProfiling();
      expect(report?.summary.slowestNodes[0].nodeId).toBe(1);
      expect(report?.summary.slowestNodes[0].avgTime).toBe(25);
    });

    test('should return null if not profiling', () => {
      expect(bridge.stopProfiling()).toBeNull();
    });
  });

  describe('data access', () => {
    test('should return empty arrays when not connected', () => {
      expect(bridge.getHostTree()).toEqual([]);
      expect(bridge.getHostMetrics()).toBeNull();
      expect(bridge.getSandboxStatus()).toBeNull();
      expect(bridge.getOperationLogs()).toEqual([]);
    });

    test('should return metrics when connected', () => {
      const collector = createRuntimeCollector();
      collector.enable();
      collector.logBatch({ batchId: 1, operations: [] }, 5);

      const nodeMap = new Map([[1, { id: 1, type: 'View', props: {}, children: [] }]]);
      bridge.connectRuntime(collector, nodeMap, [1]);

      const metrics = bridge.getHostMetrics();
      expect(metrics).not.toBeNull();
      expect(metrics?.nodeCount).toBe(1);
    });
  });

  describe('export', () => {
    test('should export all data as JSON', () => {
      bridge.enable();
      bridge.handleGuestMessage({ type: '__DEVTOOLS_READY__' });
      bridge.handleGuestMessage({
        type: '__DEVTOOLS_CONSOLE__',
        entry: { level: 'log', args: ['test'], timestamp: 1000 },
      });

      const exported = bridge.export();
      const data = JSON.parse(exported);

      expect(data.guest.ready).toBe(true);
      expect(data.guest.consoleLogs).toHaveLength(1);
    });
  });

  describe('clear/reset', () => {
    test('should clear data', () => {
      bridge.enable();
      bridge.handleGuestMessage({
        type: '__DEVTOOLS_CONSOLE__',
        entry: { level: 'log', args: ['test'], timestamp: Date.now() },
      });
      bridge.handleGuestMessage({
        type: '__DEVTOOLS_ERROR__',
        error: { message: 'error', timestamp: Date.now(), fatal: false },
      });

      bridge.clear();

      expect(bridge.getConsoleLogs()).toHaveLength(0);
      expect(bridge.getErrors()).toHaveLength(0);
    });

    test('should reset including guest ready state', () => {
      bridge.enable();
      bridge.handleGuestMessage({ type: '__DEVTOOLS_READY__' });

      bridge.reset();

      expect(bridge.isGuestReady()).toBe(false);
    });
  });
});
