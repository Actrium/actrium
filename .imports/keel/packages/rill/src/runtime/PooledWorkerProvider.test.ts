import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { PooledWorkerProvider } from './PooledWorkerProvider';
import { WorkerPool } from './WorkerPool';

// Mock Worker class
class MockWorker {
  onmessage: ((ev: MessageEvent<any>) => void) | null = null;
  onerror: ((ev: ErrorEvent) => void) | null = null;
  private listeners: Map<string, Set<(ev: any) => void>> = new Map();
  private pendingGlobals: Map<string, unknown> = new Map();

  postMessage(msg: any) {
    queueMicrotask(() => {
      if (msg.type === 'init') {
        this.dispatchMessage({ type: 'result', id: msg.id, result: 'ready' });
      } else if (msg.type === 'eval') {
        // WorkerPool spreads payload: { type, id, code, globals, engineId }
        const code = msg.code ?? '';
        this.dispatchMessage({
          type: 'result',
          id: msg.id,
          result: { ok: true, data: `evaluated: ${code}` },
        });
      } else if (msg.type === 'setGlobal') {
        this.pendingGlobals.set(msg.name, msg.value);
        this.dispatchMessage({ type: 'result', id: msg.id, result: null });
      } else {
        this.dispatchMessage({ type: 'result', id: msg.id, result: null });
      }
    });
  }

  private dispatchMessage(data: any) {
    const event = { data } as MessageEvent;
    this.onmessage?.(event);
    this.listeners.get('message')?.forEach(handler => handler(event));
  }

  addEventListener(event: string, handler: (ev: any) => void) {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(handler);
  }

  removeEventListener(event: string, handler: (ev: any) => void) {
    this.listeners.get(event)?.delete(handler);
  }

  terminate() {
    this.listeners.clear();
    this.onmessage = null;
    this.onerror = null;
  }
}

describe('PooledWorkerProvider', () => {
  let pool: WorkerPool;
  let provider: PooledWorkerProvider;

  beforeEach(() => {
    pool = new WorkerPool({
      createWorker: () => new MockWorker() as unknown as Worker,
      maxWorkers: 2,
    });
  });

  afterEach(() => {
    pool?.dispose();
  });

  describe('Creation', () => {
    it('should create provider with shared pool', () => {
      provider = new PooledWorkerProvider({ pool });
      expect(provider).toBeDefined();
      expect(provider.getPool()).toBe(pool);
    });

    it('should create provider with createWorker factory', () => {
      provider = new PooledWorkerProvider({
        createWorker: () => new MockWorker() as unknown as Worker,
      });
      expect(provider).toBeDefined();
      expect(provider.getPool()).not.toBe(pool);
    });

    it('should throw if neither pool nor createWorker provided', () => {
      expect(() => new PooledWorkerProvider({})).toThrow('Either pool or createWorker must be provided');
    });
  });

  describe('Runtime Creation', () => {
    it('should create runtime successfully', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();

      expect(runtime).toBeDefined();
      expect(typeof runtime.createContext).toBe('function');
      expect(typeof runtime.dispose).toBe('function');
    });

    it('should create context from runtime', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      expect(context).toBeDefined();
      expect(typeof context.eval).toBe('function');
      expect(typeof context.evalAsync).toBe('function');
      expect(typeof context.setGlobal).toBe('function');
      expect(typeof context.getGlobal).toBe('function');
      expect(typeof context.dispose).toBe('function');
    });
  });

  describe('Code Evaluation', () => {
    it('should evaluate code via evalAsync', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      const result = await context.evalAsync!('1 + 1');
      expect(result).toBe('evaluated: 1 + 1');
    });

    it('should throw on sync eval', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      expect(() => context.eval('1 + 1')).toThrow('Use evalAsync instead');
    });

    it('should throw after dispose', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      context.dispose();

      await expect(context.evalAsync!('1 + 1')).rejects.toThrow('disposed');
    });
  });

  describe('Globals Management', () => {
    it('should set and get globals', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      context.setGlobal('testVar', 42);
      const value = await context.getGlobal('testVar');

      expect(value).toBe(42);
    });

    it('should skip function globals', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      context.setGlobal('fn', () => {});
      const value = await context.getGlobal('fn');

      expect(value).toBeUndefined();
    });

    it('should skip objects with function properties', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      context.setGlobal('obj', { fn: () => {} });
      const value = await context.getGlobal('obj');

      expect(value).toBeUndefined();
    });
  });

  describe('Interrupt Handler', () => {
    it('should support setInterruptHandler', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      expect(context.setInterruptHandler).toBeDefined();
      expect(context.clearInterruptHandler).toBeDefined();

      // Set handler that returns true (interrupt)
      context.setInterruptHandler!(() => true);

      await expect(context.evalAsync!('test')).rejects.toThrow('interrupted');
    });

    it('should clear interrupt handler', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();
      const context = runtime.createContext();

      context.setInterruptHandler!(() => true);
      context.clearInterruptHandler!();

      // Should not throw after clearing
      const result = await context.evalAsync!('test');
      expect(result).toBeDefined();
    });
  });

  describe('Pool Ownership', () => {
    it('should not dispose shared pool on runtime dispose', async () => {
      provider = new PooledWorkerProvider({ pool });
      const runtime = await provider.createRuntime();

      runtime.dispose();

      // Pool should still be usable
      const health = pool.getHealth();
      expect(health).toBeDefined();
    });

    it('should dispose owned pool on runtime dispose', async () => {
      const ownedPoolProvider = new PooledWorkerProvider({
        createWorker: () => new MockWorker() as unknown as Worker,
      });
      const ownedPool = ownedPoolProvider.getPool();
      const runtime = await ownedPoolProvider.createRuntime();

      runtime.dispose();

      // Owned pool should be disposed
      await expect(ownedPool.execute('init', {})).rejects.toThrow('disposed');
    });
  });

  describe('Multiple Providers Sharing Pool', () => {
    it('should allow multiple providers to share same pool', async () => {
      const provider1 = new PooledWorkerProvider({ pool });
      const provider2 = new PooledWorkerProvider({ pool });

      const runtime1 = await provider1.createRuntime();
      const runtime2 = await provider2.createRuntime();

      const context1 = runtime1.createContext();
      const context2 = runtime2.createContext();

      // Both should work
      const [result1, result2] = await Promise.all([
        context1.evalAsync!('code1'),
        context2.evalAsync!('code2'),
      ]);

      expect(result1).toBe('evaluated: code1');
      expect(result2).toBe('evaluated: code2');
    });
  });
});
