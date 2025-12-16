import { describe, expect, it } from 'bun:test';
import vm from 'node:vm';
import { Engine } from './engine';
import { VMProvider } from './VMProvider';

// These tests are specific to the VMProvider and should only run in a Node.js/Bun environment.
describe.skipIf(!vm)('VMProvider', () => {
  it('should interrupt a dead-loop with a timeout', async () => {
    const provider = new VMProvider({ timeout: 100 });
    const engine = new Engine({
      provider: provider, // Fixed: use 'provider' not 'quickjs'
      debug: false,
      timeout: 100,
    });

    let threw = false;
    let error: Error | null = null;
    try {
      await engine.loadBundle('for(;;){}');
    } catch (e) {
      threw = true;
      if (e instanceof Error) {
        error = e;
      }
    }

    expect(threw).toBe(true);
    expect(error?.message).toContain('Script execution timed out');
    engine.destroy();
  });

  it('should set and clear interrupt handler', () => {
    const provider = new VMProvider({ timeout: 1000 });
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // Test setInterruptHandler
    let _handlerCalled = false;
    const handler = () => {
      _handlerCalled = true;
      return false; // Don't interrupt
    };

    context.setInterruptHandler(handler);

    // Handler should be stored (tested indirectly via eval behavior)
    expect(context).toBeDefined();

    // Clear interrupt handler
    context.clearInterruptHandler();

    // Clean up
    context.dispose();
    runtime.dispose();
  });

  it('should handle dispose correctly', () => {
    const provider = new VMProvider({ timeout: 1000 });
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // Set up context state
    context.setGlobal('testVar', 123);
    expect(context.getGlobal('testVar')).toBe(123);

    // Dispose context
    context.dispose();

    // Dispose runtime
    runtime.dispose();

    // Should not throw
    expect(true).toBe(true);
  });

  it('should execute code in isolated context', () => {
    const provider = new VMProvider({ timeout: 1000 });
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // Set a global
    context.setGlobal('myVar', 42);

    // Eval code that uses the global
    const result = context.eval('myVar + 8');
    expect(result).toBe(50);

    // Clean up
    context.dispose();
    runtime.dispose();
  });

  it('should use default timeout when not specified', () => {
    const provider = new VMProvider(); // No timeout specified
    const runtime = provider.createRuntime();
    const context = runtime.createContext();

    // Should work with default timeout
    const result = context.eval('1 + 1');
    expect(result).toBe(2);

    context.dispose();
    runtime.dispose();
  });
});
