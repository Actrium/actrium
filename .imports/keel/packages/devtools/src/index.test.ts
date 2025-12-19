import { describe, test, expect, beforeEach, mock } from 'bun:test';
import { DevTools, createDevTools, getDevTools, resetDevTools } from './index';

describe('DevTools', () => {
  let devtools: DevTools;

  beforeEach(() => {
    resetDevTools();
    devtools = createDevTools();
  });

  describe('lifecycle', () => {
    test('should be disabled by default', () => {
      expect(devtools.isEnabled()).toBe(false);
    });

    test('should enable and disable', () => {
      devtools.enable();
      expect(devtools.isEnabled()).toBe(true);

      devtools.disable();
      expect(devtools.isEnabled()).toBe(false);
    });
  });

  describe('connectEngine', () => {
    test('should connect to engine with node map', () => {
      const nodeMap = new Map([
        [1, { id: 1, type: 'View', props: {}, children: [2] }],
        [2, { id: 2, type: 'Text', props: {}, children: [] }],
      ]);

      const engine = {
        getNodeMap: () => nodeMap,
        getRootChildren: () => [1],
      };

      devtools.connectEngine(engine);
      devtools.enable();

      const tree = devtools.getHostTree();
      expect(tree).toHaveLength(1);
      expect(tree[0].type).toBe('View');
      expect(tree[0].children).toHaveLength(1);
    });

    test('should hook into engine events', () => {
      const nodeMap = new Map();
      const handlers: Record<string, (...args: unknown[]) => void> = {};

      const engine = {
        getNodeMap: () => nodeMap,
        getRootChildren: () => [],
        on: (event: string, handler: (...args: unknown[]) => void) => {
          handlers[event] = handler;
        },
      };

      devtools.connectEngine(engine);
      devtools.enable();

      // Simulate batch event
      const batchHandler = mock(() => {});
      devtools.subscribe('operation', batchHandler);

      handlers['batch']?.({ batchId: 1, operations: [{ op: 'CREATE' }] }, 5);

      expect(batchHandler).toHaveBeenCalled();
    });

    test('should handle guest messages from engine', () => {
      const nodeMap = new Map();
      const handlers: Record<string, (...args: unknown[]) => void> = {};

      const engine = {
        getNodeMap: () => nodeMap,
        getRootChildren: () => [],
        on: (event: string, handler: (...args: unknown[]) => void) => {
          handlers[event] = handler;
        },
      };

      devtools.connectEngine(engine);
      devtools.enable();

      // Simulate guest message
      handlers['guestMessage']?.({ type: '__DEVTOOLS_READY__' });

      expect(devtools.isGuestReady()).toBe(true);
    });

    test('should disconnect from engine', () => {
      const nodeMap = new Map([[1, { id: 1, type: 'View', props: {}, children: [] }]]);

      const engine = {
        getNodeMap: () => nodeMap,
        getRootChildren: () => [1],
      };

      devtools.connectEngine(engine);
      devtools.enable();
      devtools.disconnectEngine();

      expect(devtools.getHostTree()).toHaveLength(0);
    });
  });

  describe('handleGuestMessage', () => {
    test('should handle guest messages directly', () => {
      devtools.enable();
      devtools.handleGuestMessage({ type: '__DEVTOOLS_READY__' });

      expect(devtools.isGuestReady()).toBe(true);
    });

    test('should collect console logs', () => {
      devtools.enable();
      devtools.handleGuestMessage({
        type: '__DEVTOOLS_CONSOLE__',
        entry: { level: 'log', args: ['hello'], timestamp: Date.now() },
      });

      expect(devtools.getConsoleLogs()).toHaveLength(1);
    });

    test('should collect errors', () => {
      devtools.enable();
      devtools.handleGuestMessage({
        type: '__DEVTOOLS_ERROR__',
        error: { message: 'test error', timestamp: Date.now(), fatal: false },
      });

      expect(devtools.getErrors()).toHaveLength(1);
    });
  });

  describe('subscription', () => {
    test('should subscribe to events', () => {
      devtools.enable();
      const handler = mock(() => {});
      devtools.subscribe('error', handler);

      devtools.handleGuestMessage({
        type: '__DEVTOOLS_ERROR__',
        error: { message: 'error', timestamp: Date.now(), fatal: false },
      });

      expect(handler).toHaveBeenCalled();
    });

    test('should unsubscribe', () => {
      devtools.enable();
      const handler = mock(() => {});
      const unsubscribe = devtools.subscribe('error', handler);
      unsubscribe();

      devtools.handleGuestMessage({
        type: '__DEVTOOLS_ERROR__',
        error: { message: 'error', timestamp: Date.now(), fatal: false },
      });

      expect(handler).not.toHaveBeenCalled();
    });
  });

  describe('profiling', () => {
    test('should start and stop profiling', () => {
      expect(devtools.isProfiling()).toBe(false);

      devtools.startProfiling();
      expect(devtools.isProfiling()).toBe(true);

      const report = devtools.stopProfiling();
      expect(devtools.isProfiling()).toBe(false);
      expect(report).not.toBeNull();
    });

    test('should generate profiling report', () => {
      devtools.enable();
      devtools.startProfiling();

      devtools.handleGuestMessage({
        type: '__DEVTOOLS_RENDER__',
        timings: [{ nodeId: 1, type: 'View', phase: 'mount', duration: 10, timestamp: Date.now() }],
        commitDuration: 10,
        timestamp: Date.now(),
      });

      const report = devtools.stopProfiling();
      expect(report?.summary.totalRenders).toBe(1);
      expect(report?.duration).toBeGreaterThanOrEqual(0);
    });
  });

  describe('data access', () => {
    test('should get sandbox status', () => {
      const nodeMap = new Map();
      const handlers: Record<string, (...args: unknown[]) => void> = {};

      devtools.connectEngine({
        getNodeMap: () => nodeMap,
        getRootChildren: () => [],
        on: (event, handler) => {
          handlers[event] = handler;
        },
      });
      devtools.enable();

      handlers['sandboxStateChange']?.('running');

      const status = devtools.getSandboxStatus();
      expect(status?.state).toBe('running');
    });

    test('should get operation logs via engine events', () => {
      const nodeMap = new Map();
      const handlers: Record<string, (...args: unknown[]) => void> = {};

      devtools.connectEngine({
        getNodeMap: () => nodeMap,
        getRootChildren: () => [],
        on: (event, handler) => {
          handlers[event] = handler;
        },
      });
      devtools.enable();

      handlers['batch']?.({ batchId: 1, operations: [{ op: 'CREATE' }] }, 5);

      const logs = devtools.getOperationLogs();
      expect(logs).toHaveLength(1);
    });
  });

  describe('export', () => {
    test('should export all data', () => {
      devtools.enable();
      devtools.handleGuestMessage({ type: '__DEVTOOLS_READY__' });

      const exported = devtools.export();
      const data = JSON.parse(exported);

      expect(data.guest.ready).toBe(true);
      expect(data.exportTime).toBeDefined();
    });
  });

  describe('getHostTreeText', () => {
    test('should format tree as text', () => {
      const nodeMap = new Map([
        [1, { id: 1, type: 'View', props: {}, children: [2] }],
        [2, { id: 2, type: 'Text', props: {}, children: [] }],
      ]);

      devtools.connectEngine({
        getNodeMap: () => nodeMap,
        getRootChildren: () => [1],
      });
      devtools.enable();

      const text = devtools.getHostTreeText();
      expect(text).toContain('View');
      expect(text).toContain('Text');
    });
  });

  describe('clear/reset', () => {
    test('should clear data', () => {
      devtools.enable();
      devtools.handleGuestMessage({
        type: '__DEVTOOLS_CONSOLE__',
        entry: { level: 'log', args: ['test'], timestamp: Date.now() },
      });

      devtools.clear();

      expect(devtools.getConsoleLogs()).toHaveLength(0);
    });

    test('should reset all state', () => {
      devtools.enable();
      devtools.handleGuestMessage({ type: '__DEVTOOLS_READY__' });

      devtools.reset();

      expect(devtools.isGuestReady()).toBe(false);
    });
  });
});

describe('global instance', () => {
  beforeEach(() => {
    resetDevTools();
  });

  test('should create global instance', () => {
    const d1 = getDevTools();
    const d2 = getDevTools();

    expect(d1).toBe(d2);
  });

  test('should reset global instance', () => {
    const d1 = getDevTools();
    d1.enable();
    d1.handleGuestMessage({ type: '__DEVTOOLS_READY__' });

    resetDevTools();

    const d2 = getDevTools();
    expect(d2).not.toBe(d1);
    expect(d2.isGuestReady()).toBe(false);
  });
});
