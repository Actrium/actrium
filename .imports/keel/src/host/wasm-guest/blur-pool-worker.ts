/**
 * blur-pool-worker.ts — runs INSIDE a Web Worker (approach B, #57).
 *
 * Hosts ONE independent {@link WasmGuestHost} + blur-demo-guest instance (its
 * own private wasm linear memory — no shared memory) and blurs pixel tiles the
 * pool hands it as transferable `ArrayBuffer`s: writeBytes the tile into linear
 * memory, run box_blur_(simd|scalar) on the wasm export, readBytes the result,
 * and transfer it back. This is deliberately thinner than the QuickJS
 * `worker-host` — no host:* dispatch, no receiver, no debugger — because the
 * blur guest is driven directly through its raw wasm exports.
 */

import type { PoolToWorker, WorkerToPool } from './blur-pool-protocol';
import { WasmGuestHost } from './wasm-guest-host';

// The worker global, typed to what we use. `self as unknown as` mirrors
// worker-host.ts; the DOM lib types `self` too broadly for a module worker.
interface WorkerScope {
  onmessage: ((event: { data: PoolToWorker }) => void) | null;
  postMessage(message: WorkerToPool, transfer?: Transferable[]): void;
}
const scope = self as unknown as WorkerScope;

type BlurFn = (src: number, dst: number, w: number, h: number, r: number) => void;

let host: WasmGuestHost | null = null;
let blurFn: BlurFn | null = null;
let srcPtr = 0;
let dstPtr = 0;
let capacity = 0; // bytes currently allocated for src and for dst

async function init(msg: Extract<PoolToWorker, { kind: 'init' }>): Promise<void> {
  const h = new WasmGuestHost({ dispatch: {} });
  await h.load(new Uint8Array(msg.wasm));
  blurFn = (msg.simd ? h.exports.box_blur_simd : h.exports.box_blur_scalar) as BlurFn;
  host = h;
}

// Grow the src/dst scratch buffers to hold `bytes` if needed. blur_alloc leaks
// (by design in the guest), so we allocate lazily and only when a bigger tile
// arrives — the leak is bounded by the largest tile ever seen.
function ensureCapacity(h: WasmGuestHost, bytes: number): void {
  if (bytes <= capacity) return;
  const alloc = h.exports.blur_alloc as (len: number) => number;
  srcPtr = alloc(bytes);
  dstPtr = alloc(bytes);
  if (srcPtr === 0 || dstPtr === 0) throw new Error('blur_alloc returned null (out of memory)');
  capacity = bytes;
}

function blurTile(msg: Extract<PoolToWorker, { kind: 'blur' }>): void {
  if (!host || !blurFn) throw new Error('blur before ready');
  const n = msg.w * msg.h * 4;
  ensureCapacity(host, n);
  host.writeBytes(srcPtr, new Uint8Array(msg.buf, 0, n));
  blurFn(srcPtr, dstPtr, msg.w, msg.h, msg.r);
  // readBytes returns a fresh slice-copy Uint8Array over a standalone
  // ArrayBuffer of exactly n bytes — transfer it back zero-copy.
  const out = host.readBytes(dstPtr, n);
  const outBuf = out.buffer as ArrayBuffer;
  scope.postMessage({ kind: 'result', id: msg.id, buf: outBuf }, [outBuf]);
}

async function handleInit(msg: Extract<PoolToWorker, { kind: 'init' }>): Promise<void> {
  try {
    await init(msg);
    scope.postMessage({ kind: 'ready' });
  } catch (err) {
    scope.postMessage({ kind: 'error', message: err instanceof Error ? err.message : String(err) });
  }
}

scope.onmessage = (event) => {
  const msg = event.data;
  switch (msg.kind) {
    case 'init':
      void handleInit(msg);
      break;
    case 'blur':
      try {
        blurTile(msg);
      } catch (err) {
        scope.postMessage({
          kind: 'error',
          message: err instanceof Error ? err.message : String(err),
          id: msg.id,
        });
      }
      break;
    case 'destroy':
      host = null;
      blurFn = null;
      break;
  }
};
