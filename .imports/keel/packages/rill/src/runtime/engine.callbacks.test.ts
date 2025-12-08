/**
 * Engine callback and message handling tests
 *
 * Covers:
 * - handleCallFunction (lines 470-480)
 * - handleHostEvent (lines 485-495)
 * - Event listener error handling (lines 520-521)
 * - sendToSandbox with metrics (line 504)
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { Engine } from './engine';

// Mock QuickJS Provider with callback tracking
function createMockQuickJSProvider() {
  const evalCalls: string[] = [];
  const globals = new Map<string, unknown>();

  return {
    evalCalls,
    globals,
    createRuntime() {
      return {
        createContext() {
          return {
            eval(code: string): unknown {
              evalCalls.push(code);
              // Execute callback registry setup and invocations
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              try {
                const fn = new Function(...globalNames, `"use strict"; ${code}`);
                return fn(...globalValues);
              } catch (e) {
                throw e;
              }
            },
            setGlobal(name: string, value: unknown): void {
              globals.set(name, value);
            },
            getGlobal(name: string): unknown {
              return globals.get(name);
            },
            dispose(): void {
              globals.clear();
            },
          };
        },
        dispose(): void {},
      };
    },
  };
}

describe('Engine Callback Handling', () => {
  let engine: Engine;
  let mockProvider: ReturnType<typeof createMockQuickJSProvider>;

  beforeEach(() => {
    mockProvider = createMockQuickJSProvider();
    engine = new Engine({ quickjs: mockProvider });
  });

  afterEach(() => {
    engine.destroy();
  });

  it('should handle CALL_FUNCTION message via sendToSandbox', async () => {
    await engine.loadBundle(`
      // Set up callback handler
      globalThis.__invokeCallback = function(fnId, args) {
        console.log('Callback invoked:', fnId, args);
      };
    `);

    // Send CALL_FUNCTION message
    engine.sendToSandbox({
      type: 'CALL_FUNCTION',
      fnId: 'fn_1_abc123',
      args: [1, 'test', { key: 'value' }],
    });

    // Verify eval was called with the callback invocation
    const callFunctionEval = mockProvider.evalCalls.find(
      (call) => call.includes('__handleHostMessage')
    );
    expect(callFunctionEval).toBeDefined();
  });

  it('should handle HOST_EVENT message via sendToSandbox', async () => {
    await engine.loadBundle(`
      globalThis.__handleHostEvent = function(eventName, payload) {
        console.log('Event received:', eventName, payload);
      };
    `);

    engine.sendToSandbox({
      type: 'HOST_EVENT',
      eventName: 'REFRESH',
      payload: { timestamp: 123456 },
    });

    const hostEventEval = mockProvider.evalCalls.find(
      (call) => call.includes('__handleHostMessage')
    );
    expect(hostEventEval).toBeDefined();
  });

  it('should handle CONFIG_UPDATE message', async () => {
    await engine.loadBundle('console.log("loaded")', { theme: 'light' });

    engine.sendToSandbox({
      type: 'CONFIG_UPDATE',
      config: { theme: 'dark', fontSize: 16 },
    });

    expect(engine.isLoaded).toBe(true);
  });

  it('should handle DESTROY message', async () => {
    await engine.loadBundle('console.log("loaded")');
    expect(engine.isDestroyed).toBe(false);

    engine.sendToSandbox({
      type: 'DESTROY',
    });

    expect(engine.isDestroyed).toBe(true);
  });

  it('should not send message when engine is destroyed', async () => {
    await engine.loadBundle('console.log("loaded")');
    const evalCountBefore = mockProvider.evalCalls.length;

    engine.destroy();

    engine.sendToSandbox({
      type: 'HOST_EVENT',
      eventName: 'TEST',
      payload: null,
    });

    // No new eval calls after destroy
    expect(mockProvider.evalCalls.length).toBe(evalCountBefore);
  });
});

describe('Engine Event Listener Error Handling', () => {
  let engine: Engine;
  let customLogger: { log: ReturnType<typeof vi.fn>; warn: ReturnType<typeof vi.fn>; error: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    customLogger = {
      log: vi.fn(),
      warn: vi.fn(),
      error: vi.fn(),
    };
    engine = new Engine({
      quickjs: createMockQuickJSProvider(),
      logger: customLogger,
      debug: true,
    });
  });

  afterEach(() => {
    engine.destroy();
  });

  it('should catch and log errors thrown by event listeners', async () => {
    // Register a listener that throws
    engine.on('load', () => {
      throw new Error('Listener error!');
    });

    // Also register a successful listener to verify it still runs
    const successHandler = vi.fn();
    engine.on('load', successHandler);

    // Load should not throw despite listener error
    await engine.loadBundle('console.log("test")');

    expect(customLogger.error).toHaveBeenCalledWith(
      '[rill] Event listener error:',
      expect.any(Error)
    );
    expect(successHandler).toHaveBeenCalled();
  });

  it('should continue with other listeners after one throws', async () => {
    const handlers = [vi.fn(), vi.fn(), vi.fn()];

    // First handler throws
    handlers[0].mockImplementation(() => {
      throw new Error('First handler error');
    });

    handlers.forEach((h) => engine.on('load', h));

    await engine.loadBundle('console.log("test")');

    // All handlers should be called despite first one throwing
    expect(handlers[0]).toHaveBeenCalled();
    expect(handlers[1]).toHaveBeenCalled();
    expect(handlers[2]).toHaveBeenCalled();
  });
});

describe('Engine Metrics', () => {
  let engine: Engine;
  let metricsCollector: Array<{ name: string; value: number; extra?: Record<string, unknown> }>;

  beforeEach(() => {
    metricsCollector = [];
    engine = new Engine({
      quickjs: createMockQuickJSProvider(),
      onMetric: (name, value, extra) => {
        metricsCollector.push({ name, value, extra });
      },
    });
  });

  afterEach(() => {
    engine.destroy();
  });

  it('should emit metrics for sendToSandbox', async () => {
    await engine.loadBundle('console.log("test")');

    engine.sendToSandbox({
      type: 'HOST_EVENT',
      eventName: 'TEST',
      payload: { data: 'value' },
    });

    const sendMetric = metricsCollector.find((m) => m.name === 'engine.sendToSandbox');
    expect(sendMetric).toBeDefined();
    expect(sendMetric?.extra).toHaveProperty('size');
  });

  it('should emit metrics for resolveSource with URL', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve('console.log("fetched")'),
    });
    global.fetch = mockFetch;

    await engine.loadBundle('https://example.com/bundle.js');

    const fetchMetric = metricsCollector.find((m) => m.name === 'engine.fetchBundle');
    expect(fetchMetric).toBeDefined();
    expect(fetchMetric?.extra).toHaveProperty('status', 200);
    expect(fetchMetric?.extra).toHaveProperty('size');
  });

  it('should emit metrics for executeBundle', async () => {
    await engine.loadBundle('console.log("test")');

    const execMetric = metricsCollector.find((m) => m.name === 'engine.executeBundle');
    expect(execMetric).toBeDefined();
    expect(execMetric?.extra).toHaveProperty('size');
  });

  it('should emit metrics for initializeRuntime', async () => {
    await engine.loadBundle('console.log("test")');

    const initMetric = metricsCollector.find((m) => m.name === 'engine.initializeRuntime');
    expect(initMetric).toBeDefined();
  });
});

describe('Engine RequireWhitelist', () => {
  it('should use default whitelist when not provided', async () => {
    const engine = new Engine({ quickjs: createMockQuickJSProvider() });

    // Default whitelist includes react, react-native, etc.
    await engine.loadBundle(`
      const React = require('react');
    `);

    expect(engine.isLoaded).toBe(true);
    engine.destroy();
  });

  it('should enforce custom whitelist', async () => {
    const customLogger = { log: vi.fn(), warn: vi.fn(), error: vi.fn() };
    const engine = new Engine({
      quickjs: createMockQuickJSProvider(),
      requireWhitelist: ['custom-module'],
      logger: customLogger,
    });

    // Attempting to require non-whitelisted module should throw
    await expect(
      engine.loadBundle(`const x = require('lodash');`)
    ).rejects.toThrow();

    engine.destroy();
  });

  it('should allow whitelisted modules', async () => {
    const engine = new Engine({
      quickjs: createMockQuickJSProvider(),
      requireWhitelist: ['react', 'my-custom-lib'],
    });

    await engine.loadBundle(`const React = require('react');`);
    expect(engine.isLoaded).toBe(true);

    engine.destroy();
  });
});

describe('Engine getHealth', () => {
  it('should return health snapshot', async () => {
    const engine = new Engine({ quickjs: createMockQuickJSProvider() });

    let health = engine.getHealth();
    expect(health.loaded).toBe(false);
    expect(health.destroyed).toBe(false);
    expect(health.errorCount).toBe(0);
    expect(health.lastErrorAt).toBeNull();
    expect(health.receiverNodes).toBe(0);

    await engine.loadBundle('console.log("test")');

    health = engine.getHealth();
    expect(health.loaded).toBe(true);
    expect(health.destroyed).toBe(false);

    engine.destroy();
  });

  it('should track error count', async () => {
    const engine = new Engine({ quickjs: createMockQuickJSProvider() });

    try {
      await engine.loadBundle('throw new Error("test error")');
    } catch {
      // Expected
    }

    const health = engine.getHealth();
    expect(health.errorCount).toBe(1);
    expect(health.lastErrorAt).not.toBeNull();

    engine.destroy();
  });

  it('should report receiver node count', async () => {
    const engine = new Engine({ quickjs: createMockQuickJSProvider() });
    engine.createReceiver(() => {});

    await engine.loadBundle(`
      __sendToHost({
        version: 1,
        batchId: 1,
        operations: [
          { op: 'CREATE', id: 1, type: 'View', props: {} },
          { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
        ]
      });
    `);

    const health = engine.getHealth();
    expect(health.receiverNodes).toBeGreaterThan(0);

    engine.destroy();
  });
});
