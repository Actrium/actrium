// e2e/web-worker.e2e.ts - Comprehensive tests for WorkerProvider
import { expect, test } from '@playwright/test';

declare global {
  interface Window {
    WorkerProvider: any;
  }
}

test.beforeEach(async ({ page }) => {
  const consolePromise = page.waitForEvent('console', (msg: any) =>
    msg.text().includes('WorkerProvider attached to window.')
  );
  await page.goto('/');
  await consolePromise;
});

// =============================================================================
// Primitive Data Types
// =============================================================================
test.describe('Primitive Types', () => {
  test('number - integer', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return 42;');
    });
    expect(result).toBe(42);
  });

  test('number - float', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return 3.14;');
    });
    expect(result).toBeCloseTo(3.14);
  });

  test('number - negative', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return -999;');
    });
    expect(result).toBe(-999);
  });

  test('number - zero', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return 0;');
    });
    expect(result).toBe(0);
  });

  test('number - Infinity', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return Infinity;');
    });
    expect(result).toBe(Infinity);
  });

  test('number - NaN', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return NaN;');
    });
    expect(result).toBeNaN();
  });

  test('string - simple', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return "hello world";');
    });
    expect(result).toBe('hello world');
  });

  test('string - empty', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return "";');
    });
    expect(result).toBe('');
  });

  test('string - unicode', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return "你好世界🌍";');
    });
    expect(result).toBe('你好世界🌍');
  });

  test('string - special characters', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return "line1\\nline2\\ttab";');
    });
    expect(result).toBe('line1\nline2\ttab');
  });

  test('boolean - true', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return true;');
    });
    expect(result).toBe(true);
  });

  test('boolean - false', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return false;');
    });
    expect(result).toBe(false);
  });

  test('null', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return null;');
    });
    expect(result).toBeNull();
  });

  test('undefined', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return undefined;');
    });
    expect(result).toBeUndefined();
  });
});

// =============================================================================
// Complex Data Types
// =============================================================================
test.describe('Complex Types', () => {
  test('object - simple', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return { name: "test", value: 123 };');
    });
    expect(result).toEqual({ name: 'test', value: 123 });
  });

  test('object - nested', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        return {
          level1: {
            level2: {
              level3: { value: "deep" }
            }
          }
        };
      `);
    });
    expect(result).toEqual({
      level1: { level2: { level3: { value: 'deep' } } },
    });
  });

  test('object - empty', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return {};');
    });
    expect(result).toEqual({});
  });

  test('array - simple', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return [1, 2, 3, 4, 5];');
    });
    expect(result).toEqual([1, 2, 3, 4, 5]);
  });

  test('array - mixed types', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return [1, "two", true, null, { key: "value" }];');
    });
    expect(result).toEqual([1, 'two', true, null, { key: 'value' }]);
  });

  test('array - nested', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return [[1, 2], [3, [4, 5]]];');
    });
    expect(result).toEqual([
      [1, 2],
      [3, [4, 5]],
    ]);
  });

  test('array - empty', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return [];');
    });
    expect(result).toEqual([]);
  });

  test('array - sparse', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        const arr = [];
        arr[0] = 1;
        arr[5] = 5;
        return arr;
      `);
    });
    expect(result[0]).toBe(1);
    expect(result[5]).toBe(5);
    expect(result.length).toBe(6);
  });
});

// =============================================================================
// Async Operations
// =============================================================================
test.describe('Async Operations', () => {
  test('Promise.resolve', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return Promise.resolve(42);');
    });
    expect(result).toBe(42);
  });

  test('setTimeout', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        return new Promise(resolve => {
          setTimeout(() => resolve("delayed"), 50);
        });
      `);
    });
    expect(result).toBe('delayed');
  });

  test('async/await', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        const delay = ms => new Promise(r => setTimeout(r, ms));
        await delay(10);
        return "completed";
      `);
    });
    expect(result).toBe('completed');
  });

  test('Promise chain', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        return Promise.resolve(1)
          .then(x => x + 1)
          .then(x => x * 2)
          .then(x => x + 10);
      `);
    });
    expect(result).toBe(14); // (1+1)*2+10 = 14
  });

  test('Promise.all', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        return Promise.all([
          Promise.resolve(1),
          Promise.resolve(2),
          Promise.resolve(3)
        ]);
      `);
    });
    expect(result).toEqual([1, 2, 3]);
  });

  test('concurrent evaluations', async ({ page }) => {
    const results = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();

      const p1 = ctx.evalAsync('return 1;');
      const p2 = ctx.evalAsync('return "two";');
      const p3 = ctx.evalAsync('return new Promise(r => setTimeout(() => r(3), 10));');
      return Promise.all([p1, p2, p3]);
    });
    expect(results).toEqual([1, 'two', 3]);
  });
});

// =============================================================================
// Error Handling
// =============================================================================
test.describe('Error Handling', () => {
  test('throw Error', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        await ctx.evalAsync('throw new Error("test error");');
      } catch (e: any) {
        return { name: e.name, message: e.message };
      }
    });
    expect(result).toEqual({ name: 'Error', message: 'test error' });
  });

  test('throw TypeError', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        await ctx.evalAsync('null.foo();');
      } catch (e: any) {
        return { name: e.name };
      }
    });
    expect(result.name).toBe('TypeError');
  });

  test('throw ReferenceError', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        await ctx.evalAsync('return undefinedVariable;');
      } catch (e: any) {
        return { name: e.name };
      }
    });
    expect(result.name).toBe('ReferenceError');
  });

  test('throw SyntaxError', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        await ctx.evalAsync('return {{{;');
      } catch (e: any) {
        return { name: e.name };
      }
    });
    expect(result.name).toBe('SyntaxError');
  });

  test('Promise rejection', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        await ctx.evalAsync('return Promise.reject(new Error("rejected"));');
      } catch (e: any) {
        return { name: e.name, message: e.message };
      }
    });
    expect(result).toEqual({ name: 'Error', message: 'rejected' });
  });

  test('async throw', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        await ctx.evalAsync(`
          await new Promise(r => setTimeout(r, 10));
          throw new Error("async error");
        `);
      } catch (e: any) {
        return { name: e.name, message: e.message };
      }
    });
    expect(result).toEqual({ name: 'Error', message: 'async error' });
  });

  test('throw non-Error value', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        await ctx.evalAsync('throw "string error";');
      } catch (e: any) {
        return { caught: true, hasMessage: !!e.message };
      }
    });
    expect(result.caught).toBe(true);
  });
});

// =============================================================================
// Isolation Tests
// =============================================================================
test.describe('Isolation', () => {
  test('global variables persist within same context', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();

      await ctx.evalAsync('globalThis.testVar = 123;');
      return ctx.evalAsync('return globalThis.testVar;');
    });
    expect(result).toBe(123);
  });

  test('separate runtimes are isolated', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();

      const runtime1 = await provider.createRuntime();
      const ctx1 = await runtime1.createContext();

      const runtime2 = await provider.createRuntime();
      const ctx2 = await runtime2.createContext();

      await ctx1.evalAsync('globalThis.isolated = "runtime1";');
      const val2 = await ctx2.evalAsync('return globalThis.isolated;');

      return { val2 };
    });
    expect(result.val2).toBeUndefined();
  });

  test('cannot access main thread globals', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();

      const hasWindow = await ctx.evalAsync('return typeof window !== "undefined";');
      const hasDocument = await ctx.evalAsync('return typeof document !== "undefined";');

      return { hasWindow, hasDocument };
    });
    expect(result.hasWindow).toBe(false);
    expect(result.hasDocument).toBe(false);
  });

  test('has Web Worker globals', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();

      const hasSelf = await ctx.evalAsync('return typeof self !== "undefined";');
      const hasPostMessage = await ctx.evalAsync('return typeof postMessage === "function";');

      return { hasSelf, hasPostMessage };
    });
    expect(result.hasSelf).toBe(true);
    expect(result.hasPostMessage).toBe(true);
  });
});

// =============================================================================
// Resource Management
// =============================================================================
test.describe('Resource Management', () => {
  test('destroy terminates worker and rejects pending', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();

      // First call should work
      const before = await ctx.evalAsync('return "alive";');

      // Start a long-running operation, then destroy immediately
      const pendingPromise = ctx
        .evalAsync('return new Promise(r => setTimeout(() => r("should not resolve"), 5000));')
        .then(() => ({ rejected: false }))
        .catch((e: any) => ({ rejected: true, message: e.message }));

      // Destroy while the promise is pending
      runtime.dispose();

      const pendingResult = await pendingPromise;
      return { before, pendingResult };
    });
    expect(result.before).toBe('alive');
    expect(result.pendingResult.rejected).toBe(true);
  });

  test('multiple providers are independent', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider1 = new window.WorkerProvider();
      const provider2 = new window.WorkerProvider();

      const runtime1 = await provider1.createRuntime();
      const runtime2 = await provider2.createRuntime();

      const ctx1 = await runtime1.createContext();
      const ctx2 = await runtime2.createContext();

      const r1 = await ctx1.evalAsync('return "from provider 1";');
      const r2 = await ctx2.evalAsync('return "from provider 2";');

      return { r1, r2 };
    });
    expect(result.r1).toBe('from provider 1');
    expect(result.r2).toBe('from provider 2');
  });
});

// =============================================================================
// Edge Cases
// =============================================================================
test.describe('Edge Cases', () => {
  test('empty script returns undefined', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('');
    });
    expect(result).toBeUndefined();
  });

  test('script with only comments', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      // Comments followed by return undefined
      return ctx.evalAsync('// this is a comment\nreturn undefined;');
    });
    expect(result).toBeUndefined();
  });

  test('large string', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return "x".repeat(100000);');
    });
    expect(result.length).toBe(100000);
  });

  test('large array', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync('return Array.from({length: 10000}, (_, i) => i);');
    });
    expect(result.length).toBe(10000);
    expect(result[0]).toBe(0);
    expect(result[9999]).toBe(9999);
  });

  test('deeply nested object', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        let obj = { value: "deep" };
        for (let i = 0; i < 50; i++) {
          obj = { nested: obj };
        }
        return obj;
      `);
    });
    // Traverse to verify depth
    let current = result;
    let depth = 0;
    while (current.nested) {
      current = current.nested;
      depth++;
    }
    expect(depth).toBe(50);
    expect(current.value).toBe('deep');
  });

  test('rapid sequential calls', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();

      const results = [];
      for (let i = 0; i < 100; i++) {
        results.push(await ctx.evalAsync(`return ${i};`));
      }
      return results;
    });
    expect(result.length).toBe(100);
    expect(result[0]).toBe(0);
    expect(result[99]).toBe(99);
  });

  test('multiline script', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        const a = 1;
        const b = 2;
        const c = 3;

        function sum(x, y, z) {
          return x + y + z;
        }

        return sum(a, b, c);
      `);
    });
    expect(result).toBe(6);
  });

  test('script with regex', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        const str = "hello123world456";
        const matches = str.match(/\\d+/g);
        return matches;
      `);
    });
    expect(result).toEqual(['123', '456']);
  });

  test('Date operations', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        const date = new Date('2024-01-15T12:00:00Z');
        return {
          year: date.getUTCFullYear(),
          month: date.getUTCMonth(),
          day: date.getUTCDate()
        };
      `);
    });
    expect(result).toEqual({ year: 2024, month: 0, day: 15 });
  });

  test('JSON operations', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        const obj = { name: "test", values: [1, 2, 3] };
        const json = JSON.stringify(obj);
        const parsed = JSON.parse(json);
        return { json, parsed };
      `);
    });
    expect(result.json).toBe('{"name":"test","values":[1,2,3]}');
    expect(result.parsed).toEqual({ name: 'test', values: [1, 2, 3] });
  });

  test('Math operations', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      return ctx.evalAsync(`
        return {
          pi: Math.PI,
          sqrt: Math.sqrt(16),
          pow: Math.pow(2, 10),
          abs: Math.abs(-42),
          floor: Math.floor(3.7),
          ceil: Math.ceil(3.2)
        };
      `);
    });
    expect(result.pi).toBeCloseTo(Math.PI);
    expect(result.sqrt).toBe(4);
    expect(result.pow).toBe(1024);
    expect(result.abs).toBe(42);
    expect(result.floor).toBe(3);
    expect(result.ceil).toBe(4);
  });
});

// =============================================================================
// Synchronous Evaluation (should fail)
// =============================================================================
test.describe('Sync Evaluation Not Supported', () => {
  test('evaluate() throws error', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const provider = new window.WorkerProvider();
      const runtime = await provider.createRuntime();
      const ctx = await runtime.createContext();
      try {
        ctx.eval('return 1;');
        return { threw: false };
      } catch (e: any) {
        return { threw: true, message: e.message };
      }
    });
    expect(result.threw).toBe(true);
    expect(result.message).toContain('evalAsync');
  });
});
