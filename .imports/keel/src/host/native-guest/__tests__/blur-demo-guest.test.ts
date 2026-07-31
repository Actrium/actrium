import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { NativeGuestHost } from '../native-guest-host';

// blur-demo-guest ships one baseline pixel filter (a straight-alpha RGBA8 box
// blur) in two forms — a scalar reference and a hand-vectorized wasm SIMD128
// version (core::arch::wasm32 v128). The correctness contract of #57 is that the
// SIMD output is byte-for-byte identical to the scalar reference. This suite is
// that gate: for a spread of sizes and radii it drives both exports over the
// same source pixels and asserts the two framebuffers are equal, byte for byte.
//
// Rebuild the fixture with crates/build.sh after changing crates/blur-demo-guest.
const BLUR_GUEST = readFileSync(join(import.meta.dir, 'fixtures/blur-demo-guest.wasm'));

// Deterministic, non-uniform source so a real divergence would surface (a
// uniform image blurs to itself and would hide bugs). Seeded LCG, no deps.
function fillSource(buf: Uint8Array, seed: number): void {
  let s = seed >>> 0 || 1;
  for (let i = 0; i < buf.length; i++) {
    s = (Math.imul(s, 1103515245) + 12345) & 0x7fffffff;
    buf[i] = (s >>> 16) & 0xff;
  }
}

// Guest exports, cast per-function (the repo idiom; avoids a bare `unknown` cast
// of the whole exports object, which the check:unknown preflight gate bans).
type BlurFn = (src: number, dst: number, w: number, h: number, r: number) => void;
interface Blur {
  host: NativeGuestHost;
  blurAlloc: (len: number) => number;
  boxBlurScalar: BlurFn;
  boxBlurSimd: BlurFn;
}

async function loadGuest(): Promise<Blur> {
  const host = new NativeGuestHost({ dispatch: {} });
  await host.load(BLUR_GUEST);
  return {
    host,
    blurAlloc: host.exports.blur_alloc as (len: number) => number,
    boxBlurScalar: host.exports.box_blur_scalar as BlurFn,
    boxBlurSimd: host.exports.box_blur_simd as BlurFn,
  };
}

describe('blur-demo-guest — wasm SIMD128 box blur equals the scalar reference (byte-for-byte)', () => {
  // width, height, radius. Covers odd/even dims, a thin strip, and a radius
  // wider than the image (exercises edge clamping to the extreme).
  const CASES: Array<[number, number, number]> = [
    [17, 13, 1],
    [17, 13, 2],
    [64, 64, 3],
    [100, 50, 4],
    [128, 128, 5],
    [7, 7, 8],
    [200, 3, 2],
    [1, 1, 2],
  ];

  for (const [w, h, r] of CASES) {
    it(`${w}x${h} r=${r}: SIMD === scalar`, async () => {
      const { host, blurAlloc, boxBlurScalar, boxBlurSimd } = await loadGuest();
      const n = w * h * 4;

      // Allocate all buffers first; blur_alloc may grow linear memory, and the
      // host re-derives its view per call, so write/read happen after growth.
      const srcPtr = blurAlloc(n);
      const dstScalar = blurAlloc(n);
      const dstSimd = blurAlloc(n);

      const src = new Uint8Array(n);
      fillSource(src, 0x1234 ^ (w << 16) ^ (h << 8) ^ r);
      host.writeBytes(srcPtr, src);

      boxBlurScalar(srcPtr, dstScalar, w, h, r);
      boxBlurSimd(srcPtr, dstSimd, w, h, r);

      const scalar = host.readBytes(dstScalar, n);
      const simd = host.readBytes(dstSimd, n);

      expect(simd.length).toBe(n);
      // THE gate: bit-identical output.
      expect(Array.from(simd)).toEqual(Array.from(scalar));

      // Sanity: the blur is not a no-op on a non-uniform image (except the
      // degenerate 1x1 case, where any blur returns the single pixel).
      if (w * h > 1) {
        expect(Array.from(scalar)).not.toEqual(Array.from(src));
      }
    });
  }

  it('the fixture actually contains wasm SIMD (v128) opcodes — we are testing the real SIMD path', () => {
    // Canary against the SIMD export silently regressing to scalar codegen (e.g.
    // if #[target_feature]/intrinsics were dropped): every v128 instruction is
    // prefixed with 0xFD. We emit ~12 of them; require a clear floor so stray
    // 0xFD LEB bytes can't satisfy it. Not a disassembler — a regression tripwire.
    let fd = 0;
    for (const b of BLUR_GUEST) if (b === 0xfd) fd++;
    expect(fd).toBeGreaterThanOrEqual(4);
  });
});
