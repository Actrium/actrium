/**
 * Memory measurement utilities
 */

export interface MemorySnapshot {
  heapUsed: number; // bytes
  heapTotal: number;
  external: number;
  arrayBuffers: number;
}

export interface MemoryMeasurement {
  before: MemorySnapshot;
  after: MemorySnapshot;
  delta: number; // bytes allocated
}

/**
 * Get current memory usage snapshot
 */
export function getMemorySnapshot(): MemorySnapshot {
  if (typeof process !== 'undefined' && process.memoryUsage) {
    const mem = process.memoryUsage();
    return {
      heapUsed: mem.heapUsed,
      heapTotal: mem.heapTotal,
      external: mem.external,
      arrayBuffers: mem.arrayBuffers,
    };
  }

  // Fallback for browser environment (if performance.memory is available)
  if (
    typeof performance !== 'undefined' &&
    'memory' in performance &&
    performance.memory
  ) {
    const mem = performance.memory as {
      usedJSHeapSize: number;
      totalJSHeapSize: number;
      jsHeapSizeLimit: number;
    };
    return {
      heapUsed: mem.usedJSHeapSize,
      heapTotal: mem.totalJSHeapSize,
      external: 0,
      arrayBuffers: 0,
    };
  }

  // No memory API available
  return {
    heapUsed: 0,
    heapTotal: 0,
    external: 0,
    arrayBuffers: 0,
  };
}

/**
 * Measure memory allocation during function execution
 */
export async function measureMemory(
  fn: () => void | Promise<void>
): Promise<MemoryMeasurement> {
  // Force GC if available (requires --expose-gc flag in Node.js)
  if (typeof global !== 'undefined' && 'gc' in global) {
    (global as { gc: () => void }).gc();
  }

  const before = getMemorySnapshot();
  await fn();
  const after = getMemorySnapshot();

  return {
    before,
    after,
    delta: after.heapUsed - before.heapUsed,
  };
}

/**
 * Format bytes to human-readable format
 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';

  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

/**
 * Format memory measurement for display
 */
export function formatMemoryMeasurement(measurement: MemoryMeasurement): string {
  return [
    'Memory Usage:',
    `  Before: ${formatBytes(measurement.before.heapUsed)}`,
    `  After: ${formatBytes(measurement.after.heapUsed)}`,
    `  Delta: ${formatBytes(measurement.delta)}`,
  ].join('\n');
}
