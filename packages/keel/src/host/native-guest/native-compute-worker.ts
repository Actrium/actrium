/**
 * One worker-hosted, private-memory instance of a native guest's pure compute
 * exports. keel_init is not invoked and no host capability dispatch exists.
 * WebAssembly start sections still execute during instantiation; the pool
 * bounds that initialization phase with the same wall timeout as a job.
 *
 * Compute ABI:
 *   keel_compute_<name>(src_ptr, src_len, dst_ptr, dst_cap, ...i32_args) -> out_len
 *
 * The host supplies both buffers. A compute export must be synchronous and must
 * not retain either pointer after it returns.
 */
import type { ComputePoolToWorker, ComputeWorkerToPool } from './native-compute-protocol';

interface WorkerScope {
  onmessage: ((event: { data: ComputePoolToWorker }) => void) | null;
  postMessage(message: ComputeWorkerToPool, transfer?: Transferable[]): void;
  close(): void;
}

const scope = self as unknown as WorkerScope;
const PAGE_BYTES = 64 * 1024;
const EXPORT_RE = /^keel_compute_[A-Za-z0-9_]+$/;

let instance: WebAssembly.Instance | null = null;
let memory: WebAssembly.Memory | null = null;
let scratchPtr = 0;
let scratchCapacity = 0;
let maxBytes = 0;
let maxArgs = 0;

async function init(msg: Extract<ComputePoolToWorker, { kind: 'init' }>): Promise<void> {
  const ignored = () => {};
  const imports: WebAssembly.Imports = {
    env: {
      keel_host_call: ignored,
      keel_log: ignored,
      keel_send_batch: ignored,
    },
  };
  const loaded = await WebAssembly.instantiate(msg.wasm, imports);
  const exportedMemory = loaded.instance.exports.memory;
  if (!(exportedMemory instanceof WebAssembly.Memory)) {
    throw new Error('compute guest exports no WebAssembly memory');
  }
  instance = loaded.instance;
  memory = exportedMemory;
  maxBytes = msg.maxBytes;
  maxArgs = msg.maxArgs;
}

const align16 = (value: number): number => (value + 15) & ~15;

/** Reserve host-owned pages beyond the module's current memory. We never call
 * keel_init, so the app's allocator does not own this region; a compute export
 * receives it only for the duration of one synchronous call. */
function ensureScratch(required: number): number {
  if (!memory) throw new Error('compute worker not initialized');
  if (required <= scratchCapacity) return scratchPtr;
  const next = Math.max(required, scratchCapacity ? scratchCapacity * 2 : PAGE_BYTES);
  const pages = Math.ceil(next / PAGE_BYTES);
  const previousPages = memory.grow(pages);
  scratchPtr = previousPages * PAGE_BYTES;
  scratchCapacity = pages * PAGE_BYTES;
  return scratchPtr;
}

function run(msg: Extract<ComputePoolToWorker, { kind: 'run' }>): void {
  if (!instance || !memory) throw new Error('compute before ready');
  if (!EXPORT_RE.test(msg.exportName)) throw new Error('invalid compute export name');
  if (msg.input.byteLength > maxBytes || msg.outputCapacity > maxBytes) {
    throw new Error('compute byte cap exceeded');
  }
  if (msg.args.length > maxArgs || msg.args.some((arg) => !Number.isInteger(arg))) {
    throw new Error('invalid compute arguments');
  }
  const compute = instance.exports[msg.exportName];
  if (typeof compute !== 'function') throw new Error(`compute export not found: ${msg.exportName}`);
  const inputLength = msg.input.byteLength;
  const outputOffset = align16(inputLength);
  const base = ensureScratch(outputOffset + msg.outputCapacity);
  if (inputLength) new Uint8Array(memory.buffer, base, inputLength).set(new Uint8Array(msg.input));
  const outputPtr = base + outputOffset;
  const outputLength = Number(
    (compute as (...args: number[]) => number)(
      base,
      inputLength,
      outputPtr,
      msg.outputCapacity,
      ...msg.args
    )
  );
  if (!Number.isInteger(outputLength) || outputLength < 0 || outputLength > msg.outputCapacity) {
    throw new Error(
      `compute export returned invalid output length ${outputLength} (capacity ${msg.outputCapacity})`
    );
  }
  // Never transfer WebAssembly.Memory.buffer (that would detach the instance).
  // Slice-copy exactly the output, then transfer that standalone buffer.
  const output = new Uint8Array(memory.buffer, outputPtr, outputLength).slice();
  const outputBuffer = output.buffer as ArrayBuffer;
  scope.postMessage({ kind: 'result', id: msg.id, output: outputBuffer }, [outputBuffer]);
}

scope.onmessage = (event) => {
  const msg = event.data;
  if (msg.kind === 'init') {
    void init(msg).then(
      () => scope.postMessage({ kind: 'ready' }),
      (error) =>
        scope.postMessage({
          kind: 'error',
          message: error instanceof Error ? error.message : String(error),
        })
    );
  } else if (msg.kind === 'run') {
    try {
      run(msg);
    } catch (error) {
      scope.postMessage({
        kind: 'error',
        id: msg.id,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  } else {
    instance = null;
    memory = null;
    scope.close();
  }
};
