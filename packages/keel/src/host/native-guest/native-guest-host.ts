/**
 * NativeGuestHost — the host side of keel's native (non-JS) WASM guest boundary.
 *
 * A native guest is the app compiled straight to `.wasm` (Rust / C / Zig / …),
 * not JS running inside QuickJS. This host instantiates that module, injects the
 * host:* capabilities as WASM imports, and drives the linear-memory ABI:
 *
 *   guest --(keel_host_call)-->  host  --(dispatch)-->  host:* impl
 *   guest <--(keel_resolve)----  host  <--(result)----
 *
 * It **reuses the existing capability dispatch** (`createHostModuleDispatch`), so
 * a host:* implemented once serves both JS (QuickJS) and native guests — this is
 * purely additive to the QuickJS path (see docs/native-guest.zh.md). The async
 * model mirrors the QuickJS bridge's callback-resolve: a call returns at once and
 * the result comes back later via keel_resolve(cb_id, …).
 *
 * Seal: the guest can only reach what this importObject provides. No fetch /
 * socket / RTC (WASM has no ambient network; the host doesn't import them), and
 * an undeclared host module fails closed. The WASM import model *is* the sandbox.
 *
 * ABI v0 (headless / foundation):
 *   host  -> guest imports:  env.keel_host_call(mod_ptr,mod_len, method_ptr,method_len, in_ptr,in_len, cb_id)
 *                            env.keel_log(ptr,len)
 *                            env.keel_now_ms() -> f64 (monotonic, 1 ms granularity)
 *   guest -> host  exports:  memory, keel_alloc(size)->ptr, keel_resolve(cb,ok,ptr,len), keel_init()
 *   guest -> host  exports (optional): keel_abi_version() -> u32,
 *                            keel_wire_caps(flags) (WIP binary op-batch gate),
 *                            keel_on_event(name_ptr,name_len, payload_ptr,payload_len)
 *   wire: request/response/event bytes live in the guest's linear memory,
 *         addressed by (ptr,len); the host reads guest memory bounds-checked
 *         (results are copied out). The bytes are UTF-8 JSON by default and an
 *         KBS1 envelope (contracts/sandbox/store-net-bytes.json) when the value carries
 *         a Uint8Array — one self-describing fork, same on all three paths.
 */
import type { HostModuleDispatchTable } from '../../contract';
import type { OperationBatch, SerializedOperation } from '../../shared/types';
import {
  decodeRequest as decodeKbs1Request,
  encodeResult as encodeKbs1Result,
  isKbs1,
} from '../wire/store-net-envelope';
import { decodeBatchStreaming } from '../wire/wire-decoder';

/**
 * `keel_wire_caps` flag bits (host→guest capability push for WIP binary
 * wires). Bit0 = binary op-batch render batches.
 */
const WIRE_CAP_OP_BATCH_BINARY = 0x1;

/** Cap on render-channel drop warnings per host, so hostile input cannot flood the log. */
const MAX_BATCH_DROP_WARNINGS = 10;

/** Same cap for host->guest event drops (a per-frame event source could flood too). */
const MAX_EVENT_DROP_WARNINGS = 10;

/** Native guests get useful profiling resolution without a high-resolution side-channel. */
const CLOCK_GRANULARITY_MS = 1;

/** Default clock is relative and monotonic; the Date fallback never exposes wall time. */
function defaultRawNowMs(): () => number {
  if (typeof globalThis.performance?.now === 'function') {
    return () => globalThis.performance.now();
  }
  const origin = Date.now();
  return () => Date.now() - origin;
}

/**
 * Guest ABI versions this host understands. A guest may declare its version via
 * the optional `keel_abi_version()` export; the host rejects any version outside
 * this set at load (fail-closed) and tolerates a guest that omits the export
 * (pre-versioning — the ABI v0/v1 wire is identical).
 */
export const SUPPORTED_GUEST_ABI_VERSIONS: ReadonlySet<number> = new Set([1]);

export interface NativeGuestHostOptions {
  /** Capability dispatch table from `createHostModuleDispatch(contract, impl)`. */
  dispatch: HostModuleDispatchTable;
  /** Sink for guest `keel_log` calls. */
  onLog?: (message: string) => void;
  /**
   * Sink for guest render batches (`keel_send_batch`). The batch wire is UTF-8
   * JSON in guest memory (or the binary op-batch wire when `binaryOpBatch` is
   * on); the host decodes it to an `OperationBatch` and hands it off —
   * typically to `receiver.applyBatch`. Keeping this a callback leaves
   * `NativeGuestHost` decoupled from the receiver.
   */
  onRenderBatch?: (batch: OperationBatch) => void;
  /**
   * Enable the binary op-batch render wire (default TRUE — promoted, issue #45).
   * When true, the host pushes `keel_wire_caps(bit0)` to a guest that exports it
   * (before `keel_init`, so the first render is already negotiated) and accepts
   * KEEL-magic binary frames on the render channel. A guest without the export
   * (a pre-promotion, JSON-only build) stays on JSON — the caps push is skipped.
   * Set to false to force JSON on the render channel; a binary frame arriving
   * while this is off is dropped with a warning (fail-closed).
   */
  binaryOpBatch?: boolean;
  /**
   * Reports the intervals in which host JavaScript is entering guest
   * WebAssembly. A worker proxy uses this to charge guest CPU time without
   * charging time spent awaiting a host capability.
   */
  onGuestRunStateChange?: (running: boolean) => void;
  /**
   * Relative monotonic clock source for `env.keel_now_ms` (primarily for
   * deterministic tests and platform ports). The host clamps it to 1 ms and
   * never lets the guest observe a backwards or non-finite sample.
   */
  nowMs?: () => number;
}

/** Runtime-owned metadata passed after the schema-checked guest input. */
export interface NativeHostCallContext {
  /** Aborted by env.keel_host_cancel(callbackId). */
  signal: AbortSignal;
}

/**
 * Preserve the public v0.1 HostError shape across the native ABI without
 * trusting an arbitrary thrown object. A module may only report an error for
 * the method currently being dispatched, and error codes stay deliberately
 * small and machine-readable. Everything else uses the legacy opaque fallback.
 */
// Reason: a guest-initiated host module may throw any JavaScript value at this ABI boundary.
function structuredHostError(error: unknown, expectedMethod: string): object | null {
  if (typeof error !== 'object' || error === null) return null;
  try {
    // Reason: each field of an arbitrary thrown value remains untrusted until checked below.
    const candidate = error as { method?: unknown; code?: unknown; retryable?: unknown };
    const method = candidate.method;
    const code = candidate.code;
    const retryable = candidate.retryable;
    if (
      method !== expectedMethod ||
      typeof code !== 'string' ||
      !/^[a-z][a-z0-9-]{0,62}$/.test(code) ||
      typeof retryable !== 'boolean'
    ) {
      return null;
    }
    return { method, code, retryable };
  } catch {
    // A thrown object is untrusted and may expose throwing accessors.
    return null;
  }
}

// Reason: legacy host modules may throw arbitrary JavaScript values rather than Error objects.
function legacyHostError(error: unknown): { error: string } {
  try {
    return { error: error instanceof Error ? error.message : String(error) };
  } catch {
    return { error: 'host call failed' };
  }
}

// Reason: this guard classifies an arbitrary thrown value before any field is read.
function resemblesStructuredHostError(error: unknown): boolean {
  if (typeof error !== 'object' || error === null) return false;
  try {
    return 'method' in error || 'code' in error || 'retryable' in error;
  } catch {
    return true;
  }
}

export class NativeGuestHost {
  private instance!: WebAssembly.Instance;
  private memory!: WebAssembly.Memory;
  private readonly dispatch: HostModuleDispatchTable;
  private readonly onLog?: (message: string) => void;
  private readonly onRenderBatch?: (batch: OperationBatch) => void;
  private readonly binaryOpBatch: boolean;
  private readonly onGuestRunStateChange?: (running: boolean) => void;
  private readonly rawNowMs: () => number;
  private readonly inflight = new Set<Promise<void>>();
  private guestRunDepth = 0;
  private readonly callControllers = new Map<number, AbortController>();
  private abiVersion: number | null = null;
  private batchDropWarnings = 0;
  private eventDropWarnings = 0;
  private lastNowMs = 0;

  constructor(options: NativeGuestHostOptions) {
    this.dispatch = options.dispatch;
    this.onLog = options.onLog;
    this.onRenderBatch = options.onRenderBatch;
    this.binaryOpBatch = options.binaryOpBatch ?? true;
    this.onGuestRunStateChange = options.onGuestRunStateChange;
    this.rawNowMs = options.nowMs ?? defaultRawNowMs();
  }

  /**
   * The ABI version the loaded guest declared via `keel_abi_version()`, or
   * `null` when the guest predates the export (tolerated). Undefined until
   * `load()` runs.
   */
  get guestAbiVersion(): number | null {
    return this.abiVersion;
  }

  /** Instantiate the guest `.wasm`, wire host imports, and run its entry. */
  async load(wasmSource: BufferSource | WebAssembly.Module): Promise<void> {
    const cancelHostCall = (cb: number) => this.onCancelHostCall(cb >>> 0);
    const importObject: WebAssembly.Imports = {
      env: {
        keel_host_call: (
          mp: number,
          ml: number,
          xp: number,
          xl: number,
          ip: number,
          il: number,
          cb: number
        ) => this.onHostCall(mp >>> 0, ml >>> 0, xp >>> 0, xl >>> 0, ip >>> 0, il >>> 0, cb >>> 0),
        keel_log: (ptr: number, len: number) => this.onLog?.(this.readString(ptr >>> 0, len >>> 0)),
        keel_now_ms: () => this.nowMs(),
        keel_send_batch: (ptr: number, len: number) => this.onSendBatch(ptr >>> 0, len >>> 0),
        keel_host_cancel: cancelHostCall,
      },
    };
    const loaded = await this.runGuestAsync<
      WebAssembly.Instance | WebAssembly.WebAssemblyInstantiatedSource
    >(() =>
      wasmSource instanceof WebAssembly.Module
        ? WebAssembly.instantiate(wasmSource, importObject)
        : WebAssembly.instantiate(wasmSource, importObject)
    );
    // The bytes overload returns { module, instance }; the already-compiled
    // module overload returns the instance directly.
    const instance = loaded instanceof WebAssembly.Instance ? loaded : loaded.instance;
    this.instance = instance;
    this.memory = instance.exports.memory as WebAssembly.Memory;

    // ABI version gate BEFORE keel_init: a guest that declares an unsupported
    // version must not run its entry (fail-closed). A guest that omits the
    // export is a pre-versioning guest and is tolerated.
    this.runGuest(() => {
      const versionExport = instance.exports.keel_abi_version;
      if (typeof versionExport === 'function') {
        // May trap (guest code): let it propagate — a guest whose version probe
        // traps must not run (same fail-closed posture as a failed instantiate).
        const version = (versionExport as () => number)() >>> 0;
        if (!SUPPORTED_GUEST_ABI_VERSIONS.has(version)) {
          throw new Error(
            `unsupported guest ABI version: ${version} (host supports: ${[...SUPPORTED_GUEST_ABI_VERSIONS].join(', ')})`
          );
        }
        this.abiVersion = version;
      } else {
        this.abiVersion = null; // pre-versioning guest (ABI v0/v1 wire is identical): tolerated
      }

      // WIP binary op-batch: push the runtime half of the gate BEFORE keel_init,
      // so the guest's very first render already goes out on the negotiated
      // wire. Optional in both directions — a guest without the export (JSON-only
      // build) is skipped, and with the option off the guest is never told, so
      // old/new host x guest combinations all degrade to JSON.
      if (this.binaryOpBatch) {
        const wireCaps = instance.exports.keel_wire_caps;
        if (typeof wireCaps === 'function') {
          (wireCaps as (flags: number) => void)(WIRE_CAP_OP_BATCH_BINARY);
        }
      }

      (instance.exports.keel_init as (() => void) | undefined)?.();
    });
  }

  /** Return a finite, non-decreasing relative time at the ABI's fixed 1 ms granularity. */
  private nowMs(): number {
    const raw = this.rawNowMs();
    if (!Number.isFinite(raw)) return this.lastNowMs;
    const quantized = Math.floor(raw / CLOCK_GRANULARITY_MS) * CLOCK_GRANULARITY_MS;
    this.lastNowMs = Math.max(0, this.lastNowMs, quantized);
    return this.lastNowMs;
  }

  /** Resolve once every in-flight host call has run its keel_resolve. */
  async drain(): Promise<void> {
    while (this.inflight.size > 0) {
      await Promise.all([...this.inflight]);
    }
  }

  get exports(): WebAssembly.Exports {
    return this.instance.exports;
  }

  /**
   * Deliver an event (name + payload) to the guest's `keel_on_event` export.
   * No-op if the guest doesn't export it. One-way, like the render channel —
   * this is how input / lifecycle events, and pushed byte streams, reach a
   * native guest.
   *
   * The payload takes the SAME framing fork as `resolve()`'s return path: a
   * payload carrying at least one `Uint8Array` is encoded as a KBS1 envelope
   * (bytes ride as raw segments); a byte-free payload emits the IDENTICAL
   * `JSON.stringify` bytes as before — no behaviour change for any existing
   * event. The ABI is unchanged: `keel_on_event` always took a (ptr,len) byte
   * range, and the guest SDK hands its handlers the raw `&[u8]`, so a guest
   * that wants bytes peeks the magic and unframes; one that doesn't sees the
   * same JSON it always did.
   */
  // Reason: an event payload is any JSON-serializable value, plus Uint8Array leaves the KBS1 fork hoists.
  emitEvent(name: string, payload?: unknown): void {
    const onEvent = this.instance.exports.keel_on_event as
      | ((np: number, nl: number, pp: number, pl: number) => void)
      | undefined;
    if (typeof onEvent !== 'function') return;

    // Encode OUTSIDE the delivery try/catch: an encode failure is a host-side
    // data problem (a byte segment over cap, a circular payload), not the
    // guest's doing, so it gets a named warning instead of the silent drop that
    // guest-mediated failures take below. Silence here would turn "this event
    // never arrived" into an unexplainable gap on the guest side.
    let nameBytes: Uint8Array;
    let payloadBytes: Uint8Array;
    try {
      nameBytes = new TextEncoder().encode(name);
      payloadBytes =
        encodeKbs1Result(payload ?? null) ??
        new TextEncoder().encode(JSON.stringify(payload ?? null));
    } catch (err) {
      this.warnEventDropped(name, err instanceof Error ? err.message : String(err));
      return;
    }

    try {
      this.runGuest(() => {
        const np = this.allocWrite(nameBytes);
        const pp = this.allocWrite(payloadBytes);
        onEvent(np, nameBytes.length, pp, payloadBytes.length);
      });
    } catch {
      // Delivery is guest-mediated (keel_alloc can hand back a bad pointer,
      // keel_on_event can trap). A hostile/broken guest must not make event
      // delivery throw into the host — drop the event. Same fail-closed
      // contract as onHostCall / onSendBatch.
    }
  }

  private warnEventDropped(name: string, reason: string): void {
    if (this.eventDropWarnings >= MAX_EVENT_DROP_WARNINGS) return;
    this.eventDropWarnings++;
    const suffix =
      this.eventDropWarnings === MAX_EVENT_DROP_WARNINGS
        ? ' (further drop warnings suppressed)'
        : '';
    console.warn(`[keel:native-guest] event "${name}" dropped before delivery: ${reason}${suffix}`);
  }

  /** Allocate guest memory via keel_alloc and copy `bytes` in; returns the ptr. */
  private allocWrite(bytes: Uint8Array): number {
    const ptr = (this.instance.exports.keel_alloc as (n: number) => number)(bytes.length) >>> 0;
    // keel_alloc returning 0 (NULL) is the allocator's failure signal (e.g. the
    // SDK's bump heap is exhausted). Address 0 is IN bounds, so without this
    // check the write below would silently corrupt whatever the guest keeps at
    // the bottom of its linear memory. Fail closed instead.
    if (ptr === 0 && bytes.length > 0) {
      throw new Error(`guest keel_alloc failed (returned 0) for ${bytes.length} bytes`);
    }
    // keel_alloc is guest code: a broken/exhausted allocator can hand back a
    // pointer that doesn't fit. Validate before writing (re-reads the buffer,
    // which may have grown during alloc). Fails closed instead of writing OOB.
    this.assertInBounds(ptr, bytes.length);
    new Uint8Array(this.memory.buffer, ptr, bytes.length).set(bytes);
    return ptr;
  }

  /** Copy `len` bytes at `ptr` out of the guest's linear memory (bounds-checked). */
  readBytes(ptr: number, len: number): Uint8Array {
    this.assertInBounds(ptr, len);
    return new Uint8Array(this.memory.buffer.slice(ptr, ptr + len));
  }

  /**
   * Copy `bytes` INTO the guest's linear memory at `ptr` (bounds-checked, the
   * WRITE counterpart of readBytes — the host:asset `blit` path). `ptr`/len are
   * guest-supplied and UNTRUSTED: assertInBounds fails closed (throws, caught by
   * the caller) so a hostile ptr/cap can NEVER write past guest memory. The write
   * targets a LIVE view over `memory.buffer` (unlike readBytes' slice-copy) — that
   * is the point, we are handing decoded bytes to the guest — but the bounds check
   * re-reads `memory.buffer.byteLength` first, so a memory.grow that happened
   * during the async decode is accounted for before the write lands.
   */
  writeBytes(ptr: number, bytes: Uint8Array): void {
    this.assertInBounds(ptr, bytes.length);
    new Uint8Array(this.memory.buffer, ptr, bytes.length).set(bytes);
  }

  private readString(ptr: number, len: number): string {
    this.assertInBounds(ptr, len);
    return new TextDecoder().decode(new Uint8Array(this.memory.buffer, ptr, len));
  }

  /**
   * Reject guest-supplied (ptr, len) that fall outside linear memory. The guest
   * is untrusted, so a bad pointer must fail here (caught by the call site and
   * turned into a fail-closed result), not read OOB or crash the host.
   */
  private assertInBounds(ptr: number, len: number): void {
    if (
      !Number.isInteger(ptr) ||
      !Number.isInteger(len) ||
      ptr < 0 ||
      len < 0 ||
      ptr + len > this.memory.buffer.byteLength
    ) {
      throw new Error(`guest pointer out of bounds: ptr=${ptr} len=${len}`);
    }
  }

  // --- render channel: guest keel_send_batch -> host onRenderBatch (-> receiver) ---
  private onSendBatch(ptr: number, len: number): void {
    if (!this.onRenderBatch) return;
    try {
      const raw = this.readBytes(ptr, len);
      // Render-channel framing fork (same full-u32 magic policy as onHostCall,
      // never byte 0 alone): 'KEEL' (4B 45 45 4C) -> binary op-batch wire;
      // anything else -> the legacy JSON path, byte-for-byte unchanged.
      const isBinaryOpBatch =
        raw.length >= 4 && raw[0] === 0x4b && raw[1] === 0x45 && raw[2] === 0x45 && raw[3] === 0x4c;
      if (isBinaryOpBatch) {
        if (!this.binaryOpBatch) {
          // A guest can only emit this frame if something told it to (its
          // keel_wire_caps was called) — a frame arriving with the gate off is
          // a wiring bug, not hostile noise; fail closed but attributably.
          this.warnBatchDropped(
            'binary op-batch frame received but the binaryOpBatch option is off'
          );
          return;
        }
        // Collect at the call site — the decoder stays streaming per its
        // header contract. readBytes returned an exact-size copy, so
        // raw.buffer IS the frame (offset 0, no detach hazard).
        const operations: SerializedOperation[] = [];
        const header = decodeBatchStreaming(raw.buffer as ArrayBuffer, (op) => operations.push(op));
        this.onRenderBatch({
          version: header.version,
          batchId: header.batchId,
          operations,
        } as unknown as OperationBatch);
        return;
      }
      const batch = JSON.parse(new TextDecoder().decode(raw)) as OperationBatch;
      this.onRenderBatch(batch);
    } catch (err) {
      // A hostile/malformed batch (bad pointer, non-JSON, WireDecodeError,
      // wrong shape) must not crash the host — drop it, but never silently:
      // the drop reason is the difference between a five-minute fix and a
      // debugging session. Warning volume is capped against hostile flooding.
      this.warnBatchDropped(err instanceof Error ? err.message : String(err));
    }
  }

  private warnBatchDropped(reason: string): void {
    if (this.batchDropWarnings >= MAX_BATCH_DROP_WARNINGS) return;
    this.batchDropWarnings++;
    const suffix =
      this.batchDropWarnings === MAX_BATCH_DROP_WARNINGS
        ? ' (further drop warnings suppressed)'
        : '';
    console.warn(`[keel:native-guest] render batch dropped: ${reason}${suffix}`);
  }

  // --- ABI bridge: guest keel_host_call -> host dispatch -> guest keel_resolve ---
  private onCancelHostCall(cb: number): void {
    // Unknown/already-settled ids are intentionally idempotent. A guest may race
    // cancellation with completion, and cancellation must never affect another
    // callback merely because its id is absent.
    this.callControllers.get(cb)?.abort();
  }

  private onHostCall(
    mp: number,
    ml: number,
    xp: number,
    xl: number,
    ip: number,
    il: number,
    cb: number
  ): void {
    // A callback id names exactly one live call. Reuse would make a later cancel
    // ambiguous and could resolve the same guest future twice, so reject it before
    // dispatch. Resolution remains deferred by the task's first await below.
    const duplicate = this.callControllers.has(cb);
    const controller = new AbortController();
    if (!duplicate) this.callControllers.set(cb, controller);
    // Everything guest-controlled (pointers, bytes, JSON) is read inside the try
    // so a hostile or malformed call fails closed (resolve ok=0) instead of
    // crashing the host.
    const task = (async () => {
      let canonicalMethod = '';
      try {
        // MEMORY-SAFETY CRITICAL (cross-crate invariant): the host must never
        // resolve synchronously inside a guest-initiated call. keel_resolve
        // re-enters the guest's single-task executor, and re-polling a future
        // that is still mid-poll is not merely a stack overflow — it aliases
        // `&mut` on the guest's `static mut TASK`, i.e. undefined behavior in the
        // guest. Yielding here defers all reads + resolve to a microtask after
        // the guest has parked (it is suspended until then, so its buffer is
        // stable and no future is mid-poll). Do not remove this await.
        await Promise.resolve();
        if (duplicate) throw new Error(`duplicate host callback id: ${cb}`);
        const moduleId = this.readString(mp, ml);
        const method = this.readString(xp, xl);
        canonicalMethod = `${moduleId.startsWith('host:') ? moduleId.slice(5) : moduleId}.${method}`;
        // Payload framing fork (canvas-wire.DESIGN.md §1.4 + store-net-bytes
        // §B.2). Each binary wire has a DISTINCT 4-byte magic, all sharing byte 0
        // (0x4B 'K'), so the fork compares the FULL u32 — never byte 0 alone:
        //   - 'KCNV' (4B 43 4E 56) -> hand RAW BYTES to a binary-aware capability
        //     (host:canvas.draw) to decode itself.
        //   - 'KBS1' (4B 42 53 31) -> decode the KBS1 envelope here and revive
        //     each {"$b":N} sentinel into a Uint8Array, then hand the
        //     contract-agnostic args to dispatch (the codec is self-describing —
        //     no per-module knowledge). A malformed frame throws and fails closed.
        //   - anything else -> the legacy JSON.parse path, byte-for-byte
        //     unchanged (a JSON body '{…}'/'[…]' matches no magic). Genuinely
        //     malformed input still throws here and fails closed (ok=0).
        // Reason: the guest-supplied call input is untrusted bytes until a magic fork decodes it.
        let input: unknown;
        if (il > 0) {
          const raw = this.readBytes(ip, il);
          const isCanvasBinary =
            raw.length >= 4 &&
            raw[0] === 0x4b &&
            raw[1] === 0x43 &&
            raw[2] === 0x4e &&
            raw[3] === 0x56;
          if (isCanvasBinary) {
            input = raw;
          } else if (isKbs1(raw)) {
            input = decodeKbs1Request(raw);
          } else {
            input = JSON.parse(new TextDecoder().decode(raw));
          }
        } else {
          input = undefined;
        }
        const handler = this.dispatch[moduleId]?.[method];
        if (typeof handler !== 'function') {
          throw new Error(`host module not registered: ${moduleId}.${method}`);
        }
        const result = await handler(input, {
          signal: controller.signal,
        } satisfies NativeHostCallContext);
        this.resolve(cb, 1, result === undefined ? null : result);
      } catch (err) {
        this.resolve(
          cb,
          0,
          structuredHostError(err, canonicalMethod) ??
            (resemblesStructuredHostError(err)
              ? { error: 'host call failed' }
              : legacyHostError(err))
        );
      }
    })();
    this.inflight.add(task);
    void task.finally(() => {
      this.inflight.delete(task);
      if (!duplicate && this.callControllers.get(cb) === controller) {
        this.callControllers.delete(cb);
      }
    });
  }

  /** Write a JSON result into guest memory (via keel_alloc) and hand it back. */
  // Reason: a host module result is any JSON-serializable value, only stringified here.
  private resolve(cb: number, ok: number, result: unknown): void {
    // Writing the result back is guest-mediated (keel_alloc can return a bad
    // pointer, the keel_resolve export can trap). Absorb any failure here so it
    // never escapes the async task — otherwise the catch in onHostCall would
    // call resolve() again, throw again, and reject drain(). On failure the cb
    // is simply abandoned (the guest never sees a resolution), staying fail-closed.
    try {
      // Return framing fork (store-net-bytes §B.3), symmetric with the receive
      // fork. When the handler result carries at least one Uint8Array, encode an
      // KBS1 envelope (hoisting each byte stream to a segment + a {"$b":N}
      // sentinel); when it carries none, `encodeKbs1Result` returns null and the
      // reply is the IDENTICAL JSON.stringify bytes as today — no behaviour
      // change for any existing (non-binary) capability. Contract-agnostic: the
      // hoist is driven by the runtime type (Uint8Array), not per-module config.
      this.runGuest(() => {
        const envelope = encodeKbs1Result(result);
        const bytes = envelope ?? new TextEncoder().encode(JSON.stringify(result));
        const ptr = this.allocWrite(bytes);
        (
          this.instance.exports.keel_resolve as (
            cb: number,
            ok: number,
            ptr: number,
            len: number
          ) => void
        )(cb, ok, ptr, bytes.length);
      });
    } catch {
      // guest-mediated write failed; abandon this cb.
    }
  }

  /** Run one synchronous guest entry while reporting a balanced run interval. */
  private runGuest<T>(run: () => T): T {
    this.setGuestRunning(true);
    try {
      return run();
    } finally {
      this.setGuestRunning(false);
    }
  }

  /** Account for instantiation too: a module start section can run arbitrary guest code. */
  private async runGuestAsync<T>(run: () => Promise<T>): Promise<T> {
    this.setGuestRunning(true);
    try {
      return await run();
    } finally {
      this.setGuestRunning(false);
    }
  }

  private setGuestRunning(running: boolean): void {
    if (running) {
      this.guestRunDepth++;
      if (this.guestRunDepth !== 1) return;
    } else {
      if (this.guestRunDepth === 0) return;
      this.guestRunDepth--;
      if (this.guestRunDepth !== 0) return;
    }
    try {
      this.onGuestRunStateChange?.(running);
    } catch {
      // Observability must never become a guest capability or affect execution.
    }
  }
}
