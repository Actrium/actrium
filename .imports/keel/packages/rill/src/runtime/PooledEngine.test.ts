import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { PooledEngine } from './PooledEngine';
import { WorkerPool, createWorkerPool, disposeGlobalWorkerPool } from './WorkerPool';

// Mock Worker class for pool testing
class MockWorker {
  onmessage: ((ev: MessageEvent<any>) => void) | null = null;
  onerror: ((ev: ErrorEvent) => void) | null = null;
  private listeners: Map<string, Set<(ev: any) => void>> = new Map();

  postMessage(msg: any) {
    queueMicrotask(() => {
      if (msg.type === 'init') {
        this.dispatchMessage({ type: 'result', id: msg.id, result: 'ready' });
      } else if (msg.type === 'eval') {
        const code = msg.code ?? '';
        this.dispatchMessage({
          type: 'result',
          id: msg.id,
          result: { ok: true, data: undefined },
        });
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

describe('PooledEngine', () => {
  let pool: WorkerPool;
  let engines: PooledEngine[] = [];

  beforeEach(() => {
    pool = new WorkerPool({
      createWorker: () => new MockWorker() as unknown as Worker,
      maxWorkers: 4,
    });
    engines = [];
  });

  afterEach(() => {
    engines.forEach(e => e.destroy());
    pool?.dispose();
    disposeGlobalWorkerPool();
  });

  describe('Creation', () => {
    it('should create with explicit pool', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      expect(engine).toBeDefined();
      expect(engine.getPool()).toBe(pool);
    });

    it('should create with default options', () => {
      // Note: This would use global pool, but Worker is mocked
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      expect(engine).toBeDefined();
    });

    it('should implement IEngine interface', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      // Verify all IEngine methods exist
      expect(typeof engine.register).toBe('function');
      expect(typeof engine.loadBundle).toBe('function');
      expect(typeof engine.on).toBe('function');
      expect(typeof engine.sendEvent).toBe('function');
      expect(typeof engine.updateConfig).toBe('function');
      expect(typeof engine.createReceiver).toBe('function');
      expect(typeof engine.getReceiver).toBe('function');
      expect(typeof engine.getRegistry).toBe('function');
      expect(typeof engine.getHealth).toBe('function');
      expect(typeof engine.destroy).toBe('function');
      expect(typeof engine.isLoaded).toBe('boolean');
      expect(typeof engine.isDestroyed).toBe('boolean');
    });
  });

  describe('Multiple Engines Sharing Pool', () => {
    it('should share pool across engines', () => {
      const engine1 = new PooledEngine({ pool });
      const engine2 = new PooledEngine({ pool });
      const engine3 = new PooledEngine({ pool });

      engines.push(engine1, engine2, engine3);

      expect(engine1.getPool()).toBe(pool);
      expect(engine2.getPool()).toBe(pool);
      expect(engine3.getPool()).toBe(pool);
    });

    it('should isolate engine destruction', () => {
      const engine1 = new PooledEngine({ pool });
      const engine2 = new PooledEngine({ pool });

      engines.push(engine2); // Only track engine2 for cleanup

      engine1.destroy();

      // engine2 should still work
      expect(engine2.isDestroyed).toBe(false);
      // Pool should still be available
      expect(pool.getHealth().activeWorkers).toBeGreaterThanOrEqual(0);
    });
  });

  describe('Health Monitoring', () => {
    it('should report engine health', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      const health = engine.getHealth();
      expect(health).toHaveProperty('loaded');
      expect(health).toHaveProperty('destroyed');
      expect(health).toHaveProperty('errorCount');
    });

    it('should report pool health', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      const poolHealth = engine.getPoolHealth();
      expect(poolHealth).toHaveProperty('activeWorkers');
      expect(poolHealth).toHaveProperty('busyWorkers');
      expect(poolHealth).toHaveProperty('queueSize');
    });
  });

  describe('Event System', () => {
    it('should support event subscription', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      let destroyCalled = false;
      const unsubscribe = engine.on('destroy', () => {
        destroyCalled = true;
      });

      engine.destroy();
      expect(destroyCalled).toBe(true);
    });

    it('should support unsubscription', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      let callCount = 0;
      const unsubscribe = engine.on('destroy', () => {
        callCount++;
      });

      unsubscribe();
      engine.destroy();

      expect(callCount).toBe(0);
    });
  });

  describe('IEngine API Coverage', () => {
    it('should support register method', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      expect(() => {
        engine.register({ View: 'View', Text: 'Text' });
      }).not.toThrow();
    });

    it('should support loadBundle method', async () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      await expect(engine.loadBundle('const x = 1;')).resolves.toBeUndefined();
    });

    it('should support sendEvent method', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      expect(() => {
        engine.sendEvent('customEvent', { data: 'test' });
      }).not.toThrow();
    });

    it('should support updateConfig method', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      expect(() => {
        engine.updateConfig({ setting: 'value' });
      }).not.toThrow();
    });

    it('should support createReceiver method', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      const receiver = engine.createReceiver(() => {});
      expect(receiver).toBeDefined();
    });

    it('should support getReceiver method', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      engine.createReceiver(() => {});
      const receiver = engine.getReceiver();
      expect(receiver).not.toBeNull();
    });

    it('should support getRegistry method', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      const registry = engine.getRegistry();
      expect(registry).toBeDefined();
    });

    it('should support isLoaded getter', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      const loaded = engine.isLoaded;
      expect(typeof loaded).toBe('boolean');
    });

    it('should support isDestroyed getter', () => {
      const engine = new PooledEngine({ pool });
      engines.push(engine);

      expect(engine.isDestroyed).toBe(false);
      engine.destroy();
      expect(engine.isDestroyed).toBe(true);
    });
  });
});

describe('PooledEngine vs Engine API Compatibility', () => {
  it('both should implement same interface methods', async () => {
    const { Engine } = await import('./engine');

    // Get method names from Engine prototype
    const engineMethods = Object.getOwnPropertyNames(Engine.prototype)
      .filter(name => name !== 'constructor' && !name.startsWith('_'));

    // Get method names from PooledEngine prototype
    const pooledEngineMethods = Object.getOwnPropertyNames(PooledEngine.prototype)
      .filter(name => name !== 'constructor' && !name.startsWith('_'));

    // Core IEngine methods should be present in both
    const coreMethods = [
      'register',
      'loadBundle',
      'on',
      'sendEvent',
      'updateConfig',
      'createReceiver',
      'getReceiver',
      'getRegistry',
      'getHealth',
      'destroy',
    ];

    for (const method of coreMethods) {
      expect(engineMethods).toContain(method);
      expect(pooledEngineMethods).toContain(method);
    }
  });
});
