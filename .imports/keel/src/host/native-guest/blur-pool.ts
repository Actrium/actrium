/**
 * blur-pool.ts — approach B (#57): a Web Worker pool that box-blurs a large
 * RGBA8 framebuffer in parallel across N INDEPENDENT native-wasm guest
 * instances, one per worker, with pixel tiles moved as **transferable**
 * ArrayBuffers (zero-copy across the postMessage boundary).
 *
 * NO SharedArrayBuffer, no atomics, no COOP/COEP, no shared wasm memory — each
 * worker owns a private linear memory (approach A's cost is deferred, see
 * docs/native-guest-threads.zh.md). The blur guest and its host seal are
 * untouched; this is pure host-side orchestration.
 *
 * Correctness (the gate in __tests__/blur-worker-pool.test.ts): the tiled result
 * is BYTE-FOR-BYTE identical to a single-instance whole-image blur. The trick is
 * the halo — the image is split into full-width row strips, and each worker is
 * fed its interior rows PLUS `radius` rows of overlap above and below (clamped
 * to the true image bounds). Interior output rows therefore see the exact same
 * window as the whole-image blur (real neighbours across seams; true-edge
 * clamping only at the real top/bottom), and only the discarded halo rows differ.
 *
 * Concurrency is HOST-managed: the pool size is a caller parameter (a browser
 * host derives it from navigator.hardwareConcurrency); the single-threaded guest
 * neither sees nor needs a core count.
 */

import type { PoolToWorker, WorkerToPool } from './blur-pool-protocol';

const DEFAULT_WORKERS = 4;
// Fail-closed bomb-wall caps (host-side, checked BEFORE any tile is dispatched —
// mirrors host:asset's "dimension-capped before the raster"). These bound a
// hostile/buggy caller from asking the pool to allocate an absurd framebuffer or
// spin a pathological window. They are runtime tuning knobs, not a wire cap.
const DEFAULT_MAX_PIXELS = 64 * 1024 * 1024; // 64 Mpx
const DEFAULT_MAX_RADIUS = 256;

export interface BlurPoolOptions {
  /** Number of workers (pool size). Default 4. Caller derives from navigator.hardwareConcurrency. */
  workers?: number;
  /** Use the SIMD kernel (box_blur_simd) vs the scalar reference. Default true. */
  simd?: boolean;
  /** Injectable worker factory (tests override; default loads ./blur-pool-worker). */
  createWorker?: () => Worker;
  /** Fail-closed cap on width*height. Default 64 Mpx. */
  maxPixels?: number;
  /** Fail-closed cap on radius. Default 256. */
  maxRadius?: number;
}

interface Strip {
  y0: number; // interior first row (inclusive), image coords
  y1: number; // interior last row (exclusive)
  ry0: number; // padded first row (inclusive) = max(0, y0-r)
  ph: number; // padded height = ry1 - ry0
  buf: ArrayBuffer; // padded source rows, w*ph*4 bytes
}

interface Pending {
  resolve: (buf: ArrayBuffer) => void;
  reject: (err: Error) => void;
}

export class BlurPool {
  readonly #wasm: ArrayBuffer;
  readonly #simd: boolean;
  readonly #workerCount: number;
  readonly #maxPixels: number;
  readonly #maxRadius: number;
  readonly #createWorker: () => Worker;

  #workers: Worker[] = [];
  #pending = new Map<number, Pending>();
  #seq = 0;
  #ready: Promise<void> | null = null;
  #destroyed = false;

  constructor(wasmBytes: Uint8Array, options: BlurPoolOptions = {}) {
    // Copy into a standalone ArrayBuffer so each worker's init clone is clean.
    this.#wasm = wasmBytes.slice().buffer;
    this.#simd = options.simd ?? true;
    this.#workerCount = Math.max(1, options.workers ?? DEFAULT_WORKERS);
    this.#maxPixels = options.maxPixels ?? DEFAULT_MAX_PIXELS;
    this.#maxRadius = options.maxRadius ?? DEFAULT_MAX_RADIUS;
    this.#createWorker =
      options.createWorker ??
      (() =>
        new Worker(new URL('./blur-pool-worker.ts', import.meta.url).href, { type: 'module' }));
  }

  /** Blur a whole RGBA8 (straight-alpha, stride=w*4) image; returns a new buffer. */
  async blur(src: Uint8Array, width: number, height: number, radius: number): Promise<Uint8Array> {
    this.#assertRequest(src, width, height, radius);
    await this.#ensureReady();

    const output = new Uint8Array(width * height * 4);
    const stripCount = Math.min(this.#workerCount, height);
    const strips = this.#planStrips(src, width, height, radius, stripCount);

    // One strip per worker (stripCount <= workerCount); each worker blurs its
    // strip in parallel and we stitch its interior rows back.
    await Promise.all(
      strips.map((strip, i) => this.#blurStrip(this.#workers[i]!, strip, width, radius, output))
    );
    return output;
  }

  /** Terminate every worker. Idempotent. */
  destroy(): void {
    this.#destroyed = true;
    for (const worker of this.#workers) {
      try {
        worker.postMessage({ kind: 'destroy' } satisfies PoolToWorker);
      } catch {
        // best-effort
      }
      worker.terminate();
    }
    this.#workers = [];
    for (const p of this.#pending.values()) p.reject(new Error('blur-pool destroyed'));
    this.#pending.clear();
  }

  #assertRequest(src: Uint8Array, w: number, h: number, r: number): void {
    if (this.#destroyed) throw new Error('blur-pool: used after destroy()');
    if (!Number.isInteger(w) || !Number.isInteger(h) || w <= 0 || h <= 0) {
      throw new Error(`blur-pool: bad dimensions ${w}x${h}`);
    }
    if (!Number.isInteger(r) || r < 0 || r > this.#maxRadius) {
      throw new Error(`blur-pool: radius ${r} out of range (0..${this.#maxRadius})`);
    }
    if (w * h > this.#maxPixels) {
      throw new Error(`blur-pool: ${w}x${h} = ${w * h}px exceeds maxPixels cap ${this.#maxPixels}`);
    }
    if (src.length !== w * h * 4) {
      throw new Error(`blur-pool: src length ${src.length} != ${w * h * 4} (w*h*4)`);
    }
  }

  #planStrips(src: Uint8Array, w: number, h: number, r: number, stripCount: number): Strip[] {
    const rowBytes = w * 4;
    const base = Math.floor(h / stripCount);
    const extra = h % stripCount;
    const strips: Strip[] = [];
    let y = 0;
    for (let i = 0; i < stripCount; i++) {
      const rows = base + (i < extra ? 1 : 0);
      const y0 = y;
      const y1 = y + rows;
      y = y1;
      const ry0 = Math.max(0, y0 - r);
      const ry1 = Math.min(h, y1 + r);
      const ph = ry1 - ry0;
      const buf = new ArrayBuffer(w * ph * 4);
      // Copy the padded source rows [ry0, ry1) verbatim (full width).
      new Uint8Array(buf).set(src.subarray(ry0 * rowBytes, ry1 * rowBytes));
      strips.push({ y0, y1, ry0, ph, buf });
    }
    return strips;
  }

  #blurStrip(
    worker: Worker,
    strip: Strip,
    w: number,
    r: number,
    output: Uint8Array
  ): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const id = this.#seq++;
      this.#pending.set(id, {
        resolve: (buf) => {
          this.#stitch(strip, buf, w, output);
          resolve();
        },
        reject,
      });
      const msg: PoolToWorker = { kind: 'blur', id, buf: strip.buf, w, h: strip.ph, r };
      worker.postMessage(msg, [strip.buf]);
    });
  }

  #stitch(strip: Strip, resultBuf: ArrayBuffer, w: number, output: Uint8Array): void {
    const rowBytes = w * 4;
    const result = new Uint8Array(resultBuf);
    // Interior rows [y0, y1) live at strip-local rows [y0-ry0, y1-ry0).
    const localStart = (strip.y0 - strip.ry0) * rowBytes;
    const bytes = (strip.y1 - strip.y0) * rowBytes;
    output.set(result.subarray(localStart, localStart + bytes), strip.y0 * rowBytes);
  }

  #ensureReady(): Promise<void> {
    if (this.#ready) return this.#ready;
    this.#ready = new Promise<void>((resolveAll, rejectAll) => {
      let remaining = this.#workerCount;
      for (let i = 0; i < this.#workerCount; i++) {
        const worker = this.#createWorker();
        this.#workers.push(worker);
        worker.onmessage = (event: MessageEvent<WorkerToPool>) => {
          const msg = event.data;
          switch (msg.kind) {
            case 'ready':
              if (--remaining === 0) resolveAll();
              break;
            case 'result': {
              const p = this.#pending.get(msg.id);
              this.#pending.delete(msg.id);
              p?.resolve(msg.buf);
              break;
            }
            case 'error':
              if (msg.id !== undefined) {
                const p = this.#pending.get(msg.id);
                this.#pending.delete(msg.id);
                p?.reject(new Error(msg.message));
              } else {
                rejectAll(new Error(`worker init failed: ${msg.message}`));
              }
              break;
          }
        };
        worker.onerror = (event: ErrorEvent) =>
          rejectAll(new Error(`worker error: ${event.message}`));
        // wasm is CLONED (no transfer list) so every worker gets its own copy.
        worker.postMessage({
          kind: 'init',
          wasm: this.#wasm,
          simd: this.#simd,
        } satisfies PoolToWorker);
      }
    });
    return this.#ready;
  }
}
