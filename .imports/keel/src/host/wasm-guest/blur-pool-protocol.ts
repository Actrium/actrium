/**
 * Message protocol between the main-thread {@link BlurPool} and each
 * `blur-pool-worker` (approach B, #57). Deliberately tiny — three main→worker
 * kinds and three worker→main kinds — and structured-clone-safe, with the
 * pixel buffers moved as **transferable** `ArrayBuffer`s (zero-copy) rather than
 * cloned. Each worker owns a private wasm linear memory; there is no shared
 * memory, no SharedArrayBuffer, no atomics.
 */

export type PoolToWorker =
  // Instantiate a WasmGuestHost + load the blur guest. `wasm` is CLONED (sent
  // with no transfer list) so every worker in the pool gets its own copy.
  | { kind: 'init'; wasm: ArrayBuffer; simd: boolean }
  // Blur one tile: `buf` (transferred in) holds w*h*4 straight-alpha RGBA8
  // bytes; the worker blurs it in place and transfers the result back.
  | { kind: 'blur'; id: number; buf: ArrayBuffer; w: number; h: number; r: number }
  | { kind: 'destroy' };

export type WorkerToPool =
  | { kind: 'ready' }
  // `id` present => the error is for a specific tile; absent => an init failure.
  | { kind: 'error'; message: string; id?: number }
  // The blurred tile, transferred back (a standalone ArrayBuffer of w*h*4 bytes).
  | { kind: 'result'; id: number; buf: ArrayBuffer };
