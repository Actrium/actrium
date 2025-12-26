/**
 * Engine setGlobal mechanism tests
 *
 * Verifies that Engine uses setGlobal to pass complex objects directly
 * without JSON serialization, enabling support for:
 * - Circular references (e.g., RN event objects)
 * - Functions
 * - Complex nested objects
 *
 * Note: These tests focus on the Engine's use of setGlobal mechanism.
 * Full end-to-end testing with actual providers (JSC, QuickJS, VM, WASM)
 * should be done via integration tests.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';
import { createMockJSEngineProvider } from '../test-utils';

describe('Engine setGlobal Direct Object Passing', () => {
  let engine: Engine;

  beforeEach(() => {
    engine = new Engine({ quickjs: createMockJSEngineProvider() });
  });

  afterEach(() => {
    engine.destroy();
  });

  it('should use setGlobal to pass objects without JSON serialization', async () => {
    await engine.loadBundle('console.log("test")');

    // Create an object that would fail JSON.stringify
    const circularObj: Record<string, unknown> = { a: 1 };
    circularObj.self = circularObj;

    // Verify JSON.stringify would fail
    expect(() => JSON.stringify(circularObj)).toThrow();

    // But setGlobal should work
    engine.context?.setGlobal('testCircular', circularObj);
    const retrieved = engine.context?.getGlobal('testCircular') as Record<string, unknown>;

    expect(retrieved).toBeDefined();
    expect(retrieved.a).toBe(1);
    expect(retrieved.self).toBe(retrieved); // Circular reference preserved
  });

  it('should not fail on circular references (unlike JSON.stringify)', async () => {
    await engine.loadBundle('console.log("test")');

    // Create RN-style event object with circular references
    const targetView = { id: 'button1' };
    const rnEvent = {
      nativeEvent: {
        timestamp: Date.now(),
        pageX: 100,
        pageY: 200,
        target: targetView,
      },
      currentTarget: targetView,
      _dispatchInstances: null as unknown,
    };
    rnEvent._dispatchInstances = rnEvent; // Circular reference

    // JSON.stringify would throw
    expect(() => JSON.stringify(rnEvent)).toThrow();

    // But setGlobal should work fine
    engine.context?.setGlobal('rnEvent', rnEvent);
    const retrieved = engine.context?.getGlobal('rnEvent') as typeof rnEvent;

    expect(retrieved).toBeDefined();
    expect(retrieved.nativeEvent.pageX).toBe(100);
    expect(retrieved._dispatchInstances).toBe(retrieved); // Circular ref preserved
  });
});

describe('Engine setGlobal Cleanup', () => {
  let engine: Engine;

  beforeEach(() => {
    engine = new Engine({ quickjs: createMockJSEngineProvider() });
  });

  afterEach(() => {
    engine.destroy();
  });

  it('should set and clear global variables', async () => {
    await engine.loadBundle('console.log("test")');

    // Set a global
    engine.context?.setGlobal('testVar', 'test value');
    let result = engine.context?.getGlobal('testVar');
    expect(result).toBe('test value');

    // Clear the global (setting to undefined should remove it)
    engine.context?.setGlobal('testVar', undefined);
    result = engine.context?.getGlobal('testVar');
    expect(result).toBeUndefined();
  });
});

// Note: sanitizeArgsForSetGlobal tests removed - functionality moved to Bridge
// See: src/runtime/bridge/Bridge.edge-cases.test.ts (Circular References tests)
