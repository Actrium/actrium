import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { BlurPool } from '../blur-pool';
import { NativeGuestHost } from '../native-guest-host';

// Approach-B correctness gate (#151): an N-worker tiled box blur must be
// BYTE-FOR-BYTE identical to a single-instance whole-image blur of the same
// image with the same kernel. Equality holds because each worker's strip carries
// a `radius`-row halo, so interior output rows see the exact same window (real
// neighbours across seams; true-edge clamping only at the real top/bottom).
//
// Uses the shipped fixture; rebuild via examples/keel/guests/rust/build.sh after changing the guest.
const BLUR_GUEST = readFileSync(join(import.meta.dir, 'fixtures/blur-demo-guest.wasm'));

type BlurFn = (src: number, dst: number, w: number, h: number, r: number) => void;

function fillSource(buf: Uint8Array, seed: number): void {
  let s = seed >>> 0 || 1;
  for (let i = 0; i < buf.length; i++) {
    s = (Math.imul(s, 1103515245) + 12345) & 0x7fffffff;
    buf[i] = (s >>> 16) & 0xff;
  }
}

// Reference: blur the whole image in one NativeGuestHost (same SIMD kernel the pool
// uses), on the main thread — the oracle the pooled result must match.
async function wholeImageBlur(
  src: Uint8Array,
  w: number,
  h: number,
  r: number
): Promise<Uint8Array> {
  const host = new NativeGuestHost({ dispatch: {} });
  await host.load(BLUR_GUEST);
  const n = w * h * 4;
  const blurAlloc = host.exports.blur_alloc as (len: number) => number;
  const boxBlurSimd = host.exports.box_blur_simd as BlurFn;
  const srcPtr = blurAlloc(n);
  const dstPtr = blurAlloc(n);
  host.writeBytes(srcPtr, src);
  boxBlurSimd(srcPtr, dstPtr, w, h, r);
  return host.readBytes(dstPtr, n);
}

describe('blur worker pool — N-worker tiled blur equals the whole-image blur (byte-for-byte)', () => {
  // width, height, radius, workers. Covers uneven splits, height < workers, a
  // radius wider than a strip, a thin strip, and the radius-0 identity.
  const CASES: Array<[number, number, number, number]> = [
    [64, 48, 2, 2],
    [100, 80, 3, 4],
    [50, 37, 4, 3],
    [40, 3, 2, 4],
    [24, 24, 5, 2],
    [16, 16, 0, 4],
    [128, 96, 3, 4],
  ];

  for (const [w, h, r, workers] of CASES) {
    it(`${w}x${h} r=${r} workers=${workers}: pooled === whole-image`, async () => {
      const src = new Uint8Array(w * h * 4);
      fillSource(src, 0xc0ffee ^ (w << 17) ^ (h << 8) ^ (r << 3) ^ workers);

      const reference = await wholeImageBlur(src, w, h, r);

      const pool = new BlurPool(BLUR_GUEST, { workers, simd: true });
      try {
        const pooled = await pool.blur(src, w, h, r);
        expect(pooled.length).toBe(w * h * 4);
        expect(Array.from(pooled)).toEqual(Array.from(reference));
      } finally {
        pool.destroy();
      }
    });
  }

  it('fails closed on a radius over the cap (before dispatching any tile)', async () => {
    const pool = new BlurPool(BLUR_GUEST, { workers: 2, maxRadius: 8 });
    try {
      const src = new Uint8Array(8 * 8 * 4);
      await expect(pool.blur(src, 8, 8, 9)).rejects.toThrow(/radius 9 out of range/);
    } finally {
      pool.destroy();
    }
  });

  it('fails closed on a pixel count over the cap', async () => {
    const pool = new BlurPool(BLUR_GUEST, { workers: 2, maxPixels: 1024 });
    try {
      const src = new Uint8Array(64 * 64 * 4);
      await expect(pool.blur(src, 64, 64, 1)).rejects.toThrow(/exceeds maxPixels cap/);
    } finally {
      pool.destroy();
    }
  });
});
