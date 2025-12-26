/**
 * Engine Performance Benchmarks
 *
 * Tests core Engine operations:
 * - Bundle loading time
 * - Message passing latency (host→sandbox, sandbox→host)
 * - Event handling
 * - Configuration updates
 */

import { afterEach, beforeEach, describe, it } from 'bun:test';
import { Engine } from '../runtime/engine';
import type { BenchmarkResult } from './utils/benchmark';
import { benchmark } from './utils/benchmark';
import { formatMemoryMeasurement, measureMemory } from './utils/memory';

// Mock Provider
function createMockProvider() {
  return {
    createRuntime() {
      const globals = new Map<string, unknown>();
      return {
        createContext() {
          return {
            eval(code: string): unknown {
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              const fn = new Function(...globalNames, `"use strict"; ${code}`);
              return fn(...globalValues);
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

// Simple bundle code for testing
const SIMPLE_BUNDLE = `
  __sendToHost({
    version: 1,
    batchId: 1,
    operations: [
      { op: 'CREATE', id: 1, type: 'View', props: { style: { flex: 1 } } },
      { op: 'APPEND', id: 1, parentId: 0, childId: 1 }
    ]
  });
`;

// Medium complexity bundle
const MEDIUM_BUNDLE = `
  const operations = [];
  for (let i = 1; i <= 10; i++) {
    operations.push({ op: 'CREATE', id: i, type: 'View', props: {} });
    if (i > 1) {
      operations.push({ op: 'APPEND', id: i, parentId: i - 1, childId: i });
    }
  }
  operations.push({ op: 'APPEND', id: 1, parentId: 0, childId: 1 });
  __sendToHost({ version: 1, batchId: 1, operations });
`;

// Large bundle with many operations
const LARGE_BUNDLE = `
  const operations = [];
  for (let i = 1; i <= 100; i++) {
    operations.push({ op: 'CREATE', id: i, type: 'View', props: { index: i } });
  }
  for (let i = 2; i <= 100; i++) {
    operations.push({ op: 'APPEND', id: i, parentId: 1, childId: i });
  }
  operations.push({ op: 'APPEND', id: 1, parentId: 0, childId: 1 });
  __sendToHost({ version: 1, batchId: 1, operations });
`;

describe('Engine Benchmarks', () => {
  const results: BenchmarkResult[] = [];
  let engine: Engine;

  beforeEach(() => {
    engine = new Engine({
      provider: createMockProvider(),
      debug: false,
    });
  });

  afterEach(() => {
    engine.destroy();

    // Print results after all tests
    if (results.length > 0) {
      console.log(`\n${'='.repeat(80)}`);
      console.log('ENGINE BENCHMARK RESULTS');
      console.log('='.repeat(80));
      results.forEach((result) => {
        console.log(`\n${result.name}:`);
        console.log(`  Mean: ${result.mean.toFixed(3)}ms`);
        console.log(`  Median: ${result.median.toFixed(3)}ms`);
        console.log(`  Min: ${result.min.toFixed(3)}ms`);
        console.log(`  Max: ${result.max.toFixed(3)}ms`);
        console.log(`  Ops/sec: ${result.ops.toFixed(0)}`);
      });
      console.log(`\n${'='.repeat(80)}\n`);
    }
  });

  it('benchmark: Engine initialization', async () => {
    const result = await benchmark(
      'Engine.new',
      () => {
        const e = new Engine({
          provider: createMockProvider(),
          debug: false,
        });
        e.destroy();
      },
      { iterations: 100, warmup: 5 }
    );
    results.push(result);
  });

  it('benchmark: Simple bundle loading', async () => {
    const result = await benchmark(
      'Engine.loadBundle (simple)',
      async () => {
        const e = new Engine({ provider: createMockProvider(), debug: false });
        await e.loadBundle(SIMPLE_BUNDLE);
        e.destroy();
      },
      { iterations: 50, warmup: 5 }
    );
    results.push(result);
  });

  it('benchmark: Medium bundle loading', async () => {
    const result = await benchmark(
      'Engine.loadBundle (medium)',
      async () => {
        const e = new Engine({ provider: createMockProvider(), debug: false });
        await e.loadBundle(MEDIUM_BUNDLE);
        e.destroy();
      },
      { iterations: 50, warmup: 5 }
    );
    results.push(result);
  });

  it('benchmark: Large bundle loading', async () => {
    const result = await benchmark(
      'Engine.loadBundle (large)',
      async () => {
        const e = new Engine({ provider: createMockProvider(), debug: false });
        await e.loadBundle(LARGE_BUNDLE);
        e.destroy();
      },
      { iterations: 30, warmup: 3 }
    );
    results.push(result);
  });

  it('benchmark: Send event to sandbox', async () => {
    await engine.loadBundle(SIMPLE_BUNDLE);

    const result = await benchmark(
      'Engine.sendEvent',
      () => {
        engine.sendEvent('TEST_EVENT', { data: 'test' });
      },
      { iterations: 1000, warmup: 10 }
    );
    results.push(result);
  });

  it('benchmark: Update config', async () => {
    await engine.loadBundle(SIMPLE_BUNDLE);

    const result = await benchmark(
      'Engine.updateConfig',
      () => {
        engine.updateConfig({ theme: 'dark' });
      },
      { iterations: 1000, warmup: 10 }
    );
    results.push(result);
  });

  it('benchmark: Create receiver', async () => {
    const result = await benchmark(
      'Engine.createReceiver',
      () => {
        const receiver = engine.createReceiver(() => {});
        receiver.clear();
      },
      { iterations: 100, warmup: 5 }
    );
    results.push(result);
  });

  it('memory: Engine initialization', async () => {
    const measurement = await measureMemory(() => {
      const e = new Engine({
        provider: createMockProvider(),
        debug: false,
      });
      e.destroy();
    });

    console.log(`\n${formatMemoryMeasurement(measurement)}`);
  });

  it('memory: Bundle loading', async () => {
    const measurement = await measureMemory(async () => {
      await engine.loadBundle(LARGE_BUNDLE);
    });

    console.log(`\n${formatMemoryMeasurement(measurement)}`);
  });
});
