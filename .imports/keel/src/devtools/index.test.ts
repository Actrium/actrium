/**
 * DevTools Tests
 *
 * Tests for DevTools integration and functionality
 */

import { beforeEach, describe, expect, test } from 'bun:test';
import { createDevTools, DevTools, getDevTools, resetDevTools } from './index';

describe('DevTools', () => {
  beforeEach(() => {
    resetDevTools();
  });

  describe('Lifecycle', () => {
    test('should create DevTools instance', () => {
      const devtools = createDevTools();

      expect(devtools).toBeDefined();
      expect(devtools).toBeInstanceOf(DevTools);
    });

    test('should enable DevTools', () => {
      const devtools = createDevTools();

      expect(devtools.isEnabled()).toBe(false);

      devtools.enable();

      expect(devtools.isEnabled()).toBe(true);
    });

    test('should disable DevTools', () => {
      const devtools = createDevTools();
      devtools.enable();

      expect(devtools.isEnabled()).toBe(true);

      devtools.disable();

      expect(devtools.isEnabled()).toBe(false);
    });

    test('should accept configuration', () => {
      const devtools = createDevTools({
        runtime: {
          maxLogs: 200,
          maxTimelineEvents: 500,
        },
      });

      expect(devtools).toBeDefined();
    });
  });

  describe('Engine Connection', () => {
    test('should connect to engine', () => {
      const devtools = createDevTools();
      const mockEngine = {
        getNodeMap: () => new Map(),
        getRootChildren: () => [],
      };

      expect(() => {
        devtools.connectEngine(mockEngine);
      }).not.toThrow();
    });

    test('should disconnect from engine', () => {
      const devtools = createDevTools();
      const mockEngine = {
        getNodeMap: () => new Map(),
        getRootChildren: () => [],
      };

      devtools.connectEngine(mockEngine);

      expect(() => {
        devtools.disconnectEngine();
      }).not.toThrow();
    });

    test('should handle engine with event support', () => {
      const devtools = createDevTools();
      devtools.enable(); // Must enable to record logs
      // biome-ignore lint/complexity/noBannedTypes: Test mock needs generic function type
      let batchHandler: Function | null = null;

      const mockEngine = {
        getNodeMap: () => new Map(),
        getRootChildren: () => [],
        // biome-ignore lint/complexity/noBannedTypes: Test mock needs generic function type
        on: (event: string, handler: Function) => {
          if (event === 'batch') {
            batchHandler = handler;
          }
        },
      };

      devtools.connectEngine(mockEngine);

      expect(batchHandler).not.toBeNull();

      // Trigger batch event
      if (batchHandler) {
        batchHandler({ batchId: 1, operations: [] }, 10);
      }

      const logs = devtools.getOperationLogs();
      expect(logs.length).toBeGreaterThan(0);
    });
  });

  describe('Data Access', () => {
    test('should get host tree', () => {
      const devtools = createDevTools();
      const tree = devtools.getHostTree();

      expect(Array.isArray(tree)).toBe(true);
    });

    test('should get operation logs', () => {
      const devtools = createDevTools();
      const logs = devtools.getOperationLogs();

      expect(Array.isArray(logs)).toBe(true);
    });

    test('should get console logs', () => {
      const devtools = createDevTools();
      const logs = devtools.getConsoleLogs();

      expect(Array.isArray(logs)).toBe(true);
    });
  });

  describe('Profiling', () => {
    test('should start profiling', () => {
      const devtools = createDevTools();

      devtools.startProfiling();

      expect(devtools.isProfiling()).toBe(true);
    });

    test('should stop profiling', () => {
      const devtools = createDevTools();

      devtools.startProfiling();
      const _report = devtools.stopProfiling();

      expect(devtools.isProfiling()).toBe(false);
    });
  });

  describe('Export', () => {
    test('should export DevTools data', () => {
      const devtools = createDevTools();
      const data = devtools.export();

      expect(typeof data).toBe('string');
    });

    test('should export host tree as text', () => {
      const devtools = createDevTools();
      const text = devtools.getHostTreeText();

      expect(typeof text).toBe('string');
    });
  });

  describe('Global Instance', () => {
    test('should get global DevTools instance', () => {
      const devtools1 = getDevTools();
      const devtools2 = getDevTools();

      expect(devtools1).toBe(devtools2);
    });

    test('should reset global instance', () => {
      const devtools1 = getDevTools();

      resetDevTools();

      const devtools2 = getDevTools();

      expect(devtools1).not.toBe(devtools2);
    });
  });
});
