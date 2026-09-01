/**
 * Generic worker pool for pure, synchronous native-guest compute exports.
 * Independent wasm instances + transferable buffers: no SharedArrayBuffer,
 * atomics, COOP/COEP, shared memory or extra guest imports.
 */
import type { ComputePoolToWorker, ComputeWorkerToPool } from './native-compute-protocol';

const DEFAULT_MAX_BYTES = 64 * 1024 * 1024;
const DEFAULT_MAX_ARGS = 8;
const DEFAULT_MAX_QUEUE = 64;
const DEFAULT_TIMEOUT_MS = 5_000;
const MAX_WORKERS = 16;
const MAX_TIMER_MS = 0x7fff_ffff;
const EXPORT_RE = /^keel_compute_[A-Za-z0-9_]+$/;

export interface NativeComputeRequest {
  /** A guest export named `keel_compute_<name>`. */
  exportName: string;
  input: Uint8Array;
  /** i32 arguments passed after the four standard ptr/length parameters. */
  args?: number[];
  /** Maximum bytes the export may return; defaults to input length. */
  outputCapacity?: number;
}

export interface NativeGuestComputePoolOptions {
  /** Host-managed worker count. Defaults to min(hardwareConcurrency, 8). */
  workers?: number;
  /** Per-input and per-output cap. Default 64 MiB. */
  maxBytes?: number;
  /** Numeric argument count cap. Default 8. */
  maxArgs?: number;
  /** Waiting + running jobs cap. Default 64. */
  maxQueue?: number;
  /** Wall timeout for worker initialization and each dispatched job. */
  timeoutMs?: number;
  createWorker?: () => Worker;
}

interface NormalizedComputeRequest {
  exportName: string;
  input: ArrayBuffer;
  args: number[];
  outputCapacity: number;
}

interface Job {
  id: number;
  request: NormalizedComputeRequest;
  resolve: (output: Uint8Array) => void;
  reject: (error: Error) => void;
}

interface Slot {
  worker: Worker;
  ready: boolean;
  job: Job | null;
  initTimer: ReturnType<typeof setTimeout> | null;
  timer: ReturnType<typeof setTimeout> | null;
}

const defaultWorkerCount = (): number => {
  const hint = typeof navigator === 'object' ? Number(navigator.hardwareConcurrency) : 4;
  return Math.min(Number.isInteger(hint) && hint > 0 ? hint : 4, 8);
};

export class NativeGuestComputePool {
  readonly #wasm: ArrayBuffer;
  readonly #workerCount: number;
  readonly #maxBytes: number;
  readonly #maxArgs: number;
  readonly #maxQueue: number;
  readonly #timeoutMs: number;
  readonly #createWorker: () => Worker;
  #slots: Slot[] = [];
  #queue: Job[] = [];
  #seq = 0;
  #started = false;
  #destroyed = false;
  #fatal: Error | null = null;

  constructor(wasmBytes: Uint8Array, options: NativeGuestComputePoolOptions = {}) {
    this.#wasm = wasmBytes.slice().buffer;
    const requestedWorkers = options.workers ?? defaultWorkerCount();
    if (
      !Number.isInteger(requestedWorkers) ||
      requestedWorkers < 1 ||
      requestedWorkers > MAX_WORKERS
    ) {
      throw new Error(`native-compute: workers must be an integer in 1..${MAX_WORKERS}`);
    }
    this.#workerCount = requestedWorkers;
    this.#maxBytes = options.maxBytes ?? DEFAULT_MAX_BYTES;
    this.#maxArgs = options.maxArgs ?? DEFAULT_MAX_ARGS;
    this.#maxQueue = options.maxQueue ?? DEFAULT_MAX_QUEUE;
    this.#timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    if (!Number.isSafeInteger(this.#maxBytes) || this.#maxBytes < 0) {
      throw new Error('native-compute: maxBytes must be a non-negative safe integer');
    }
    if (!Number.isSafeInteger(this.#maxArgs) || this.#maxArgs < 0) {
      throw new Error('native-compute: maxArgs must be a non-negative safe integer');
    }
    if (!Number.isSafeInteger(this.#maxQueue) || this.#maxQueue < 1) {
      throw new Error('native-compute: maxQueue must be a positive safe integer');
    }
    if (
      !Number.isSafeInteger(this.#timeoutMs) ||
      this.#timeoutMs < 1 ||
      this.#timeoutMs > MAX_TIMER_MS
    ) {
      throw new Error(`native-compute: timeoutMs must be an integer in 1..${MAX_TIMER_MS}`);
    }
    this.#createWorker =
      options.createWorker ??
      (() =>
        new Worker(new URL('./native-compute-worker.ts', import.meta.url).href, {
          type: 'module',
        }));
  }

  run(request: NativeComputeRequest): Promise<Uint8Array> {
    if (this.#fatal) return Promise.reject(this.#fatal);
    const outstanding = this.#queue.length + this.#slots.filter((slot) => slot.job).length;
    if (outstanding >= this.#maxQueue) {
      return Promise.reject(new Error(`native-compute: queue cap ${this.#maxQueue} exceeded`));
    }
    let normalized: NormalizedComputeRequest;
    try {
      normalized = this.#normalizeRequest(request);
    } catch (error) {
      return Promise.reject(error);
    }
    return new Promise<Uint8Array>((resolve, reject) => {
      this.#queue.push({ id: ++this.#seq, request: normalized, resolve, reject });
      try {
        this.#ensureStarted();
        this.#schedule();
      } catch (error) {
        this.#failFatal(
          error instanceof Error ? error : new Error(`native-compute: ${String(error)}`)
        );
      }
    });
  }

  destroy(): void {
    if (this.#destroyed) return;
    this.#shutdown(new Error('native-compute: pool destroyed'));
  }

  #shutdown(error: Error): void {
    this.#destroyed = true;
    for (const job of this.#queue.splice(0)) job.reject(error);
    for (const slot of this.#slots) {
      if (slot.initTimer) clearTimeout(slot.initTimer);
      if (slot.timer) clearTimeout(slot.timer);
      slot.job?.reject(error);
      try {
        slot.worker.postMessage({ kind: 'destroy' } satisfies ComputePoolToWorker);
      } catch {
        // best effort
      }
      slot.worker.terminate();
    }
    this.#slots = [];
  }

  #normalizeRequest(request: NativeComputeRequest): NormalizedComputeRequest {
    if (this.#destroyed) throw new Error('native-compute: used after destroy()');
    if (typeof request.exportName !== 'string' || !EXPORT_RE.test(request.exportName)) {
      throw new Error('native-compute: exportName must match keel_compute_[A-Za-z0-9_]+');
    }
    if (!(request.input instanceof Uint8Array))
      throw new Error('native-compute: input must be Uint8Array');
    if (request.input.byteLength > this.#maxBytes) {
      throw new Error(`native-compute: input exceeds byte cap ${this.#maxBytes}`);
    }
    const outputCapacity = request.outputCapacity ?? request.input.byteLength;
    if (
      !Number.isInteger(outputCapacity) ||
      outputCapacity < 0 ||
      outputCapacity > this.#maxBytes
    ) {
      throw new Error(`native-compute: outputCapacity exceeds byte cap ${this.#maxBytes}`);
    }
    const args = request.args ?? [];
    if (!Array.isArray(args)) throw new Error('native-compute: args must be an array');
    if (args.length > this.#maxArgs)
      throw new Error(`native-compute: argument cap ${this.#maxArgs} exceeded`);
    if (args.some((arg) => !Number.isInteger(arg) || arg < -0x8000_0000 || arg > 0xffff_ffff)) {
      throw new Error('native-compute: arguments must be i32/u32 integers');
    }
    return {
      exportName: request.exportName,
      input: request.input.slice().buffer as ArrayBuffer,
      args: [...args],
      outputCapacity,
    };
  }

  #ensureStarted(): void {
    if (this.#started) return;
    this.#started = true;
    for (let index = 0; index < this.#workerCount; index++) this.#slots.push(this.#makeSlot());
  }

  #makeSlot(): Slot {
    const worker = this.#createWorker();
    const slot: Slot = { worker, ready: false, job: null, initTimer: null, timer: null };
    worker.onmessage = (event: MessageEvent<ComputeWorkerToPool>) =>
      this.#onMessage(slot, event.data);
    worker.onerror = (event: ErrorEvent) =>
      this.#failSlot(slot, new Error(`native-compute worker error: ${event.message}`));
    slot.initTimer = setTimeout(() => {
      if (slot.ready || this.#destroyed || !this.#slots.includes(slot)) return;
      this.#failFatal(
        new Error(`native-compute worker initialization timed out after ${this.#timeoutMs}ms`)
      );
    }, this.#timeoutMs);
    try {
      worker.postMessage({
        kind: 'init',
        wasm: this.#wasm,
        maxBytes: this.#maxBytes,
        maxArgs: this.#maxArgs,
      } satisfies ComputePoolToWorker);
    } catch (error) {
      clearTimeout(slot.initTimer);
      slot.initTimer = null;
      worker.terminate();
      throw error;
    }
    return slot;
  }

  #onMessage(slot: Slot, msg: ComputeWorkerToPool): void {
    if (this.#destroyed || !this.#slots.includes(slot)) return;
    if (msg.kind === 'ready') {
      if (slot.initTimer) clearTimeout(slot.initTimer);
      slot.initTimer = null;
      slot.ready = true;
      this.#schedule();
      return;
    }
    if (msg.kind === 'error' && msg.id === undefined) {
      this.#failFatal(new Error(`native-compute worker init failed: ${msg.message}`));
      return;
    }
    const job = slot.job;
    if (!job || msg.id !== job.id) return;
    if (slot.timer) clearTimeout(slot.timer);
    slot.timer = null;
    slot.job = null;
    if (msg.kind === 'result') job.resolve(new Uint8Array(msg.output));
    else job.reject(new Error(msg.message));
    this.#schedule();
  }

  #schedule(): void {
    if (this.#destroyed || this.#fatal) return;
    for (const slot of this.#slots) {
      if (!slot.ready || slot.job || !this.#queue.length) continue;
      const job = this.#queue.shift()!;
      slot.job = job;
      const inputBuffer = job.request.input;
      slot.timer = setTimeout(() => {
        if (slot.job !== job) return;
        this.#failSlot(
          slot,
          new Error(
            `native-compute: ${job.request.exportName} timed out after ${this.#timeoutMs}ms`
          )
        );
      }, this.#timeoutMs);
      try {
        slot.worker.postMessage(
          {
            kind: 'run',
            id: job.id,
            exportName: job.request.exportName,
            input: inputBuffer,
            outputCapacity: job.request.outputCapacity,
            args: job.request.args,
          } satisfies ComputePoolToWorker,
          [inputBuffer]
        );
      } catch (error) {
        this.#failSlot(
          slot,
          error instanceof Error ? error : new Error(`native-compute: ${String(error)}`)
        );
      }
    }
  }

  #failSlot(slot: Slot, error: Error): void {
    if (this.#destroyed || !this.#slots.includes(slot)) return;
    if (slot.initTimer) clearTimeout(slot.initTimer);
    slot.initTimer = null;
    if (slot.timer) clearTimeout(slot.timer);
    slot.timer = null;
    slot.job?.reject(error);
    slot.job = null;
    slot.worker.terminate();
    const index = this.#slots.indexOf(slot);
    if (index >= 0 && !this.#destroyed) {
      try {
        this.#slots[index] = this.#makeSlot();
      } catch (replacementError) {
        this.#failFatal(
          replacementError instanceof Error
            ? replacementError
            : new Error(`native-compute: ${String(replacementError)}`)
        );
        return;
      }
    }
    this.#schedule();
  }

  #failFatal(error: Error): void {
    if (this.#destroyed) return;
    this.#fatal = error;
    this.#shutdown(error);
  }
}
