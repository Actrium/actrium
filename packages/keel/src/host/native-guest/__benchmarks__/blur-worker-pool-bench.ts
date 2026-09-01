/**
 * blur-worker-pool-bench.ts
 *
 * ⚠️ GATED, NON-PRODUCTION PROFILING HARNESS — not collected by the unit-test
 * runner (not a *.test.ts). Run explicitly:
 *
 *     bun src/host/native-guest/__benchmarks__/blur-worker-pool-bench.ts
 *
 * PURPOSE
 * -------
 * Quantify the approach-B (#57) parallel speedup: box-blur one large image across
 * 1 / 2 / 4 / 8 workers (each an independent native-wasm guest instance, tiles
 * moved as transferable ArrayBuffers) and report wall time vs the 1-worker
 * baseline. This is the perf evidence; correctness (pooled == whole-image,
 * byte-for-byte) is gated separately in __tests__/blur-worker-pool.test.ts.
 *
 * ENGINE / MACHINE SPECIFIC: speedup is bounded by physical cores and the wasm
 * engine's SIMD codegen (V8 ~2x SIMD vs JSC ~1.1x — see blur-bench.ts). Under
 * bun (JSC) here the per-worker kernel is slower, so absolute numbers differ from
 * a browser; the SHAPE (near-linear up to core count, then flattening) is the
 * signal. Treat as order-of-magnitude, not a spec.
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { BlurPool } from '../blur-pool';

const BLUR_GUEST = readFileSync(join(import.meta.dir, '..', '__tests__', 'fixtures', 'blur-demo-guest.wasm'));

interface Timing {
  medianMs: number;
  minMs: number;
}

async function bench(fn: () => Promise<number>, warmup: number, rounds: number): Promise<Timing> {
  let sink = 0;
  for (let i = 0; i < warmup; i++) sink += await fn();
  const perCall: number[] = [];
  for (let r = 0; r < rounds; r++) {
    const t0 = performance.now();
    sink += await fn();
    perCall.push(performance.now() - t0);
  }
  if (sink === Number.POSITIVE_INFINITY) console.log('');
  perCall.sort((a, b) => a - b);
  return { medianMs: perCall[Math.floor(perCall.length / 2)]!, minMs: perCall[0]! };
}

async function main(): Promise<void> {
  const W = 1024;
  const H = 1024;
  const R = 4;
  const n = W * H * 4;

  const src = new Uint8Array(n);
  let s = 0x1234abcd;
  for (let i = 0; i < n; i++) {
    s = (Math.imul(s, 1103515245) + 12345) & 0x7fffffff;
    src[i] = (s >>> 16) & 0xff;
  }

  console.log(`blur worker pool — ${W}x${H} r=${R}, box_blur_simd (bun/JSC)\n`);
  console.log(`${'workers'.padEnd(10)}${'median ms'.padStart(12)}${'speedup'.padStart(10)}`);

  // Faithfulness: every pool size must produce the identical image before timing.
  let reference: Uint8Array | null = null;
  let baseline = 0;

  for (const workers of [1, 2, 4, 8]) {
    const pool = new BlurPool(BLUR_GUEST, { workers, simd: true });
    try {
      const out = await pool.blur(src, W, H, R);
      if (reference === null) {
        reference = out;
      } else if (out.length !== reference.length) {
        throw new Error('faithfulness: length mismatch');
      } else {
        for (let i = 0; i < out.length; i++) {
          if (out[i] !== reference[i]) throw new Error(`faithfulness: workers=${workers} differs at ${i}`);
        }
      }
      const t = await bench(() => pool.blur(src, W, H, R), 2, 5);
      if (workers === 1) baseline = t.medianMs;
      const speedup = baseline / t.medianMs;
      console.log(`${String(workers).padEnd(10)}${t.medianMs.toFixed(1).padStart(12)}${`${speedup.toFixed(2)}x`.padStart(10)}`);
    } finally {
      pool.destroy();
    }
  }
  console.log('\n(median of 5; bounded by physical cores; engine-specific — see header)');
}

if (import.meta.main || process.env.KEEL_BENCH_RUN) {
  await main();
}
