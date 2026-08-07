import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { NativeGuestComputePool } from '../native-compute-pool';
import { NativeGuestHost } from '../native-guest-host';
import type { ComputePoolToWorker, ComputeWorkerToPool } from '../native-compute-protocol';

const BLUR_GUEST = readFileSync(join(import.meta.dir, 'fixtures/blur-demo-guest.wasm'));

function source(width: number, height: number, seed: number): Uint8Array {
  const out = new Uint8Array(width * height * 4);
  let state = seed >>> 0;
  for (let i = 0; i < out.length; i++) {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    out[i] = state >>> 24;
  }
  return out;
}

async function directBlur(input: Uint8Array, width: number, height: number, radius: number) {
  const host = new NativeGuestHost({ dispatch: {} });
  await host.load(BLUR_GUEST);
  const alloc = host.exports.blur_alloc as (length: number) => number;
  const blur = host.exports.box_blur_simd as (
    src: number,
    dst: number,
    w: number,
    h: number,
    r: number
  ) => void;
  const src = alloc(input.length);
  const dst = alloc(input.length);
  host.writeBytes(src, input);
  blur(src, dst, width, height, radius);
  return host.readBytes(dst, input.length);
}

describe('generic native-guest compute pool', () => {
  it('runs a guest-selected compute export in worker-hosted instances byte-for-byte', async () => {
    const pool = new NativeGuestComputePool(BLUR_GUEST, { workers: 3 });
    try {
      const cases = [
        [32, 24, 0],
        [48, 31, 2],
        [64, 40, 4],
      ] as const;
      await Promise.all(
        cases.map(async ([width, height, radius], index) => {
          const input = source(width, height, 0xc0ffee + index);
          const before = input.slice();
          const [expected, actual] = await Promise.all([
            directBlur(input, width, height, radius),
            pool.run({
              exportName: 'keel_compute_box_blur',
              input,
              args: [width, height, radius],
            }),
          ]);
          expect(Array.from(actual)).toEqual(Array.from(expected));
          // The pool copies before transferring, so calling it never detaches or
          // mutates the guest/main-thread source buffer.
          expect(input).toEqual(before);
        })
      );
    } finally {
      pool.destroy();
    }
  });

  it('fails request caps before creating a worker', async () => {
    let created = 0;
    const pool = new NativeGuestComputePool(BLUR_GUEST, {
      workers: 1,
      maxBytes: 16,
      createWorker: () => (created++, {} as Worker),
    });
    await expect(
      pool.run({ exportName: 'box_blur_simd', input: new Uint8Array(1) })
    ).rejects.toThrow('exportName must match');
    await expect(
      pool.run({
        exportName: 'keel_compute_box_blur',
        input: new Uint8Array(17),
      })
    ).rejects.toThrow('input exceeds byte cap');
    expect(created).toBe(0);
  });

  it('returns an attributable error for a missing compute export', async () => {
    const pool = new NativeGuestComputePool(BLUR_GUEST, { workers: 1 });
    try {
      await expect(
        pool.run({ exportName: 'keel_compute_missing', input: new Uint8Array(4) })
      ).rejects.toThrow('compute export not found: keel_compute_missing');
    } finally {
      pool.destroy();
    }
  });

  it('terminates a worker that exceeds the wall timeout', async () => {
    let terminated = 0;
    class SilentWorker {
      onmessage: ((event: MessageEvent<ComputeWorkerToPool>) => void) | null = null;
      onerror: ((event: ErrorEvent) => void) | null = null;
      postMessage(message: ComputePoolToWorker) {
        if (message.kind === 'init') {
          queueMicrotask(() => this.onmessage?.({ data: { kind: 'ready' } } as MessageEvent));
        }
      }
      terminate() {
        terminated++;
      }
    }
    const pool = new NativeGuestComputePool(BLUR_GUEST, {
      workers: 1,
      timeoutMs: 10,
      createWorker: () => new SilentWorker() as unknown as Worker,
    });
    try {
      await expect(
        pool.run({ exportName: 'keel_compute_box_blur', input: new Uint8Array(4) })
      ).rejects.toThrow('timed out after 10ms');
      expect(terminated).toBeGreaterThanOrEqual(1);
    } finally {
      pool.destroy();
    }
  });

  it('bounds the waiting plus running queue', async () => {
    class NeverReadyWorker {
      onmessage = null;
      onerror = null;
      postMessage() {}
      terminate() {}
    }
    const pool = new NativeGuestComputePool(BLUR_GUEST, {
      workers: 1,
      maxQueue: 1,
      createWorker: () => new NeverReadyWorker() as unknown as Worker,
    });
    const first = pool.run({ exportName: 'keel_compute_box_blur', input: new Uint8Array(4) });
    await expect(
      pool.run({ exportName: 'keel_compute_box_blur', input: new Uint8Array(4) })
    ).rejects.toThrow('queue cap 1 exceeded');
    pool.destroy();
    await expect(first).rejects.toThrow('pool destroyed');
  });
});
