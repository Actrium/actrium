import { describe, expect, it } from 'bun:test';
import { Engine } from './engine';
import { NoSandboxProvider } from './NoSandboxProvider';

describe('NoSandboxProvider', () => {
  it('should execute code directly in the host global scope', async () => {
    // Set a global variable on the host (in the test environment)
    (globalThis as Record<string, unknown>).HOST_TEST_VARIABLE = 'initial_value';

    const provider = new NoSandboxProvider();
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // Direct context.eval test
    context.eval('globalThis.HOST_TEST_VARIABLE = "modified_by_guest";');

    // Check if the global variable on the host has been modified
    expect((globalThis as Record<string, unknown>).HOST_TEST_VARIABLE).toBe('modified_by_guest');

    // Clean up the global scope
    delete (globalThis as Record<string, unknown>).HOST_TEST_VARIABLE;
    context.dispose();
  });

  it('should be selected via the sandbox option', async () => {
    // This test confirms the DefaultJSEngineProvider correctly selects NoSandboxProvider
    (globalThis as Record<string, unknown>).ANOTHER_HOST_VAR = 'unmodified';

    // Let the engine select the provider automatically based on the option
    const engine = new Engine({
      sandbox: 'none',
      debug: false,
    });

    await engine.loadBundle(`globalThis.ANOTHER_HOST_VAR = 'modified_again'`);

    expect((globalThis as Record<string, unknown>).ANOTHER_HOST_VAR).toBe('modified_again');

    delete (globalThis as Record<string, unknown>).ANOTHER_HOST_VAR;
    engine.destroy();
  });

  it('should support cooperative timeout via __checkBudget', () => {
    const provider = new NoSandboxProvider({ timeout: 100 });
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // First eval injects budget checker
    context.eval('1 + 1');

    // Verify globals are injected
    expect(globalThis.__rill_budget_start).toBeDefined();
    expect(globalThis.__rill_budget_timeout).toBe(100);
    expect(globalThis.__rill_budget_ops).toBe(0);
    expect(typeof globalThis.__checkBudget).toBe('function');

    // Call checkBudget multiple times (1000+ times to trigger timeout check)
    for (let i = 0; i < 1005; i++) {
      globalThis.__checkBudget();
    }

    // Should not throw if within timeout
    expect(globalThis.__rill_budget_ops).toBeGreaterThan(1000);

    context.dispose();
  });

  it('should throw error when cooperative timeout exceeded', () => {
    const provider = new NoSandboxProvider({ timeout: 1 });
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // First eval injects budget checker
    context.eval('1 + 1');

    // Wait to exceed timeout
    const start = Date.now();
    while (Date.now() - start < 10) {
      // busy wait
    }

    // Reset budget start to simulate timeout
    globalThis.__rill_budget_start = Date.now() - 100;

    // Call checkBudget 1000+ times to trigger check
    expect(() => {
      for (let i = 0; i < 1005; i++) {
        globalThis.__checkBudget();
      }
    }).toThrow('Cooperative timeout exceeded');

    context.dispose();
  });

  it('should support getGlobal', () => {
    const provider = new NoSandboxProvider();
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    context.setGlobal('testVar', 'testValue');
    const value = context.getGlobal?.('testVar');

    expect(value).toBe('testValue');

    delete globalThis.testVar;
    context.dispose();
  });

  it('should support interrupt handler', () => {
    const provider = new NoSandboxProvider();
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    let interrupted = false;
    context.setInterruptHandler?.(() => {
      interrupted = true;
      return true;
    });

    expect(() => context.eval('1 + 1')).toThrow('interrupted by handler');
    expect(interrupted).toBe(true);

    context.dispose();
  });

  it('should support clearInterruptHandler', () => {
    const provider = new NoSandboxProvider();
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    context.setInterruptHandler?.(() => true);
    context.clearInterruptHandler?.();

    // Should not throw after clearing handler
    expect(() => context.eval('1 + 1')).not.toThrow();

    context.dispose();
  });

  it('should clean up globals on dispose', () => {
    const provider = new NoSandboxProvider({ timeout: 100 });
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // Trigger budget injection
    context.eval('1');

    // Verify globals exist
    expect(globalThis.__rill_budget_start).toBeDefined();

    context.dispose();

    // Verify cleanup
    expect(globalThis.__rill_budget_start).toBeUndefined();
    expect(globalThis.__rill_budget_timeout).toBeUndefined();
    expect(globalThis.__rill_budget_ops).toBeUndefined();
    expect(globalThis.__checkBudget).toBeUndefined();
  });

  it('should support runtime dispose', () => {
    const provider = new NoSandboxProvider({ timeout: 100 });
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    context.eval('1');

    runtime.dispose?.();

    // Should clean up
    expect(globalThis.__rill_budget_start).toBeUndefined();
  });
});
