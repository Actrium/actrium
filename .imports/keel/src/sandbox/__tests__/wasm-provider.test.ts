/**
 * WASM Provider Tests
 *
 * Tests the QuickJSNativeWASMProvider to ensure it works correctly
 * with our C API bindings.
 */

import { afterAll, beforeAll, describe, expect, it } from 'bun:test';
import { QuickJSNativeWASMProvider } from '../providers/QuickJSNativeWASMProvider';
import type { JSEngineContext, JSEngineRuntime } from '../types/provider';

// Skip if WASM not available (e.g., in some CI environments)
const describeIfWASM = typeof WebAssembly !== 'undefined' ? describe : describe.skip;

describeIfWASM('QuickJSNativeWASMProvider', () => {
  let provider: QuickJSNativeWASMProvider;
  let runtime: JSEngineRuntime;
  let context: JSEngineContext;

  beforeAll(async () => {
    provider = new QuickJSNativeWASMProvider({ debug: false });
    runtime = await provider.createRuntime();
    context = runtime.createContext();
  });

  afterAll(() => {
    context?.dispose();
    runtime?.dispose();
  });

  describe('Basic Evaluation', () => {
    it('should evaluate simple expressions', () => {
      const result = context.eval('1 + 2');
      expect(result).toBe(3);
    });

    it('should handle string results', () => {
      const result = context.eval('"hello" + " " + "world"');
      expect(result).toBe('hello world');
    });

    it('should handle objects', () => {
      const result = context.eval('({ a: 1, b: 2 })');
      expect(result).toEqual({ a: 1, b: 2 });
    });

    it('should handle arrays', () => {
      const result = context.eval('[1, 2, 3]');
      expect(result).toEqual([1, 2, 3]);
    });

    it('should handle null and undefined', () => {
      expect(context.eval('null')).toBeNull();
      expect(context.eval('undefined')).toBeUndefined();
    });
  });

  describe('Global Variables', () => {
    it('should set and get global variables', () => {
      context.setGlobal('testVar', 42);
      const result = context.eval('testVar');
      expect(result).toBe(42);
    });

    it('should set objects as globals', () => {
      context.setGlobal('testObj', { x: 1, y: 2 });
      const result = context.eval('testObj.x + testObj.y');
      expect(result).toBe(3);
    });

    it('should set arrays as globals', () => {
      context.setGlobal('testArr', [1, 2, 3]);
      const result = context.eval('testArr.reduce((a, b) => a + b, 0)');
      expect(result).toBe(6);
    });
  });

  describe('Timer Functions', () => {
    it('should have setTimeout defined', () => {
      const result = context.eval('typeof setTimeout');
      expect(result).toBe('function');
    });

    it('should have clearTimeout defined', () => {
      const result = context.eval('typeof clearTimeout');
      expect(result).toBe('function');
    });

    it('should execute setTimeout callback', async () => {
      // Set up a flag that will be set by the timeout
      context.eval(`
        globalThis.timerFired = false;
        setTimeout(() => {
          globalThis.timerFired = true;
        }, 10);
      `);

      // Wait for timer to fire
      await new Promise((resolve) => setTimeout(resolve, 50));

      const result = context.eval('globalThis.timerFired');
      expect(result).toBe(true);
    });
  });

  describe('Error Handling', () => {
    it('should throw on syntax errors', () => {
      expect(() => {
        context.eval('function {');
      }).toThrow();
    });

    it('should throw on runtime errors', () => {
      expect(() => {
        context.eval('undefinedVariable.property');
      }).toThrow();
    });
  });

  describe('Console', () => {
    it('should have console defined', () => {
      const result = context.eval('typeof console');
      expect(result).toBe('object');
    });

    it('should have console.log defined', () => {
      const result = context.eval('typeof console.log');
      expect(result).toBe('function');
    });
  });
});

describeIfWASM('QuickJSNativeWASMProvider - Multi Context', () => {
  let provider: QuickJSNativeWASMProvider;
  let runtime: JSEngineRuntime;

  beforeAll(async () => {
    provider = new QuickJSNativeWASMProvider({ debug: false });
    runtime = await provider.createRuntime();
  });

  afterAll(() => {
    runtime?.dispose();
  });

  it('should support creating and disposing multiple contexts', async () => {
    const ctx1 = runtime.createContext();
    ctx1.setGlobal('value', 1);
    expect(ctx1.eval('value')).toBe(1);
    ctx1.dispose();

    const ctx2 = runtime.createContext();
    ctx2.setGlobal('value', 2);
    expect(ctx2.eval('value')).toBe(2);
    ctx2.dispose();
  });
});
