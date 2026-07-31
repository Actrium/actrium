/**
 * blur-bench.ts
 *
 * ⚠️ GATED, NON-PRODUCTION PROFILING HARNESS — changes NO defaults, is NOT
 * wired into any live path, and is NOT collected by the unit-test runner (not a
 * *.test.ts). Run it explicitly:
 *
 *     bun src/host/native-guest/__benchmarks__/blur-bench.ts
 *
 * PURPOSE
 * -------
 * Quantify the wasm SIMD128 speedup for the baseline box-blur filter shipped by
 * crates/blur-demo-guest: the hand-vectorized `box_blur_simd` (core::arch::wasm32
 * v128) vs the `box_blur_scalar` reference, on the same source pixels, across a
 * couple of representative image sizes / radii. This is the perf evidence behind
 * #57; it is NOT the correctness gate (that lives in the sibling
 * __tests__/blur-demo-guest.test.ts and asserts byte-for-byte equality).
 *
 * FAITHFULNESS: before any timing, this harness asserts the two paths are
 * byte-for-byte identical on the bench image, so the reported speedup is for
 * work that produces the SAME output — not a faster-but-different kernel.
 *
 * ENGINE MATTERS A LOT. This harness runs under bun (JSC). Measured on the SAME
 * fixture/kernel, the SIMD win is strongly engine-dependent:
 *
 *     engine        512x512 r=4 speedup
 *     V8 (node)     ~2.07x   <- the browser / web-host engine that #57 targets
 *     JSC (bun)     ~1.11x   <- weaker wasm-SIMD codegen; this harness's runtime
 *
 * So the ~1.1x you see here UNDERSTATES the real web-host win (~2x on V8, which
 * matches/beats the issue's 1.7x for naive i32x4 SIMD). Treat these as an
 * order-of-magnitude, engine-specific signal, not a spec. Correctness (byte-for-
 * byte scalar==simd) is engine-independent and gated in the sibling test.
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { NativeGuestHost } from '../native-guest-host';

const BLUR_GUEST = readFileSync(join(import.meta.dir, '..', '__tests__', 'fixtures', 'blur-demo-guest.wasm'));

type BlurFn = (src: number, dst: number, w: number, h: number, r: number) => void;

function fillSource(buf: Uint8Array, seed: number): void {
  let s = seed >>> 0 || 1;
  for (let i = 0; i < buf.length; i++) {
    s = (Math.imul(s, 1103515245) + 12345) & 0x7fffffff;
    buf[i] = (s >>> 16) & 0xff;
  }
}

interface Timing {
  medianMs: number;
  minMs: number;
}

// Warm up, then time `rounds` single calls; report median (resists GC/scheduler
// outliers) and min (cleanest achievable). `sink` defeats dead-code elimination.
function bench(fn: () => number, opts: { warmup: number; rounds: number }): Timing {
  let sink = 0;
  for (let i = 0; i < opts.warmup; i++) sink += fn();
  const perCall: number[] = [];
  for (let r = 0; r < opts.rounds; r++) {
    const t0 = performance.now();
    sink += fn();
    perCall.push(performance.now() - t0);
  }
  if (sink === Number.POSITIVE_INFINITY) console.log(''); // keep `sink` live
  perCall.sort((a, b) => a - b);
  return { medianMs: perCall[Math.floor(perCall.length / 2)]!, minMs: perCall[0]! };
}

async function main(): Promise<void> {
  const host = new NativeGuestHost({ dispatch: {} });
  await host.load(BLUR_GUEST);
  // Cast per-function (avoids a bare `unknown` cast, which the check:unknown gate bans).
  const blurAlloc = host.exports.blur_alloc as (len: number) => number;
  const boxBlurScalar = host.exports.box_blur_scalar as BlurFn;
  const boxBlurSimd = host.exports.box_blur_simd as BlurFn;

  const CASES: Array<[number, number, number]> = [
    [256, 256, 4],
    [512, 512, 4],
    [512, 512, 8],
  ];

  console.log('blur-demo-guest — scalar vs wasm SIMD128 box blur (bun/JSC)\n');
  console.log(`${'image'.padEnd(14)}${'scalar ms'.padStart(12)}${'simd ms'.padStart(12)}${'speedup'.padStart(10)}`);

  for (const [w, h, r] of CASES) {
    const n = w * h * 4;
    const srcPtr = blurAlloc(n);
    const dstScalar = blurAlloc(n);
    const dstSimd = blurAlloc(n);

    const src = new Uint8Array(n);
    fillSource(src, 0x51ed ^ (w << 8) ^ r);
    host.writeBytes(srcPtr, src);

    // Faithfulness gate: identical output before timing anything.
    boxBlurScalar(srcPtr, dstScalar, w, h, r);
    boxBlurSimd(srcPtr, dstSimd, w, h, r);
    const a = host.readBytes(dstScalar, n);
    const b = host.readBytes(dstSimd, n);
    for (let i = 0; i < n; i++) {
      if (a[i] !== b[i]) throw new Error(`faithfulness: scalar!=simd at ${i} for ${w}x${h} r=${r}`);
    }

    const readByte0 = (dst: number) => host.readBytes(dst, 1)[0]!;
    const scalar = bench(() => {
      boxBlurScalar(srcPtr, dstScalar, w, h, r);
      return readByte0(dstScalar);
    }, { warmup: 2, rounds: 7 });
    const simd = bench(() => {
      boxBlurSimd(srcPtr, dstSimd, w, h, r);
      return readByte0(dstSimd);
    }, { warmup: 2, rounds: 7 });

    const speedup = scalar.medianMs / simd.medianMs;
    const label = `${w}x${h} r=${r}`;
    console.log(
      `${label.padEnd(14)}${scalar.medianMs.toFixed(2).padStart(12)}${simd.medianMs.toFixed(2).padStart(12)}${`${speedup.toFixed(2)}x`.padStart(10)}`
    );
  }
  console.log('\n(median of 7; engine-specific; correctness proven separately in __tests__/blur-demo-guest.test.ts)');
}

// `import.meta.main` fires under `bun <file>`; RILL_BENCH_RUN lets a wrapper opt in.
if (import.meta.main || process.env.RILL_BENCH_RUN) {
  await main();
}
