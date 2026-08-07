/**
 * NativeGuestView — adapts a native (non-JS) WASM guest to the `EngineViewEngine`
 * surface that `useEngineView` drives (see src/host/use-engine-view.ts). It lets
 * the platform mount a native guest through the SAME rendering pipeline as the
 * JS `Engine` — only the constructed "engine" object differs; `useEngineView`,
 * the receiver, keyboard forwarding, etc. are unchanged.
 *
 * Construction mirrors `Engine`: same `contract` + `hostModules` (the dispatch
 * is built from them, so host:* is written once and serves both guest kinds) and
 * the same `components`. It owns a `NativeGuestHost` + a `Receiver`; the guest's
 * render batches flow NativeGuestHost.onRenderBatch -> receiver.applyBatch, and
 * `sendEvent` forwards to the native event channel (keel_on_event).
 */
import type { HostModuleImplementationMap, KeelContractShape } from '../../contract';
import { createHostModuleDispatch } from '../../contract';
import { Receiver } from '../receiver';
import type { ComponentType } from '../registry';
import { ComponentRegistry } from '../registry';
import type { EngineViewEngine } from '../use-engine-view';
import { NativeGuestHost } from './native-guest-host';

export interface NativeGuestViewOptions {
  /** The decoded native guest `.wasm` (the caller owns any base64/gzip decode). */
  wasmBytes: BufferSource;
  /** Host-capability contract — same one an `Engine` would take. */
  contract: KeelContractShape;
  /** Host-capability implementations — same shape as `Engine`'s `hostModules`. */
  hostModules: HostModuleImplementationMap;
  /** Sealed component materializers — same map as `Engine.register`. */
  components: Record<string, ComponentType>;
  /** Sink for guest `keel_log`. */
  onLog?: (message: string) => void;
}

/**
 * The complete surface the platform mounts a native guest against: the shared
 * {@link EngineViewEngine} (so `useEngineView` drives it exactly like the JS
 * `Engine`) PLUS the wasm-specific members a native guest exposes. This was
 * previously only an implicit duck type; making it explicit ends the guessing.
 */
export interface NativeGuestEngine extends EngineViewEngine {
  /**
   * Bounds-checked slice-copy reader over the guest's linear memory. An
   * in-thread guest returns directly; a worker-isolated guest resolves after a
   * memory RPC. Host capability implementations should `await` the result.
   */
  readonly readGuestMemory: (ptr: number, len: number) => Uint8Array | Promise<Uint8Array>;
  /** Worker-aware counterpart of readGuestMemory. */
  readonly writeGuestMemory: (ptr: number, bytes: Uint8Array) => void | Promise<void>;
  /** Guest-declared ABI version; null when the guest predates the export. */
  readonly guestAbiVersion: number | null;
  /**
   * Raw guest wasm exports for diagnostics (e.g. kv-guest's `last_ok`);
   * undefined before `loadBundle` resolves. Diagnostics-only — calling
   * state-mutating exports (keel_alloc, ...) through this can corrupt the guest.
   */
  readonly exports: WebAssembly.Exports | undefined;
  /** Named host->guest event channel (keel_on_event). */
  // Reason: an event payload is any JSON-serializable value, forwarded as-is.
  sendEvent(name: string, payload?: unknown): void;
  destroy(): void;
}

type Listener = () => void;
type ErrorListener = (error: Error) => void;

export class NativeGuestView implements NativeGuestEngine {
  private readonly host: NativeGuestHost;
  private readonly wasmBytes: BufferSource;
  private readonly registry: ComponentRegistry;
  private receiver: Receiver | null = null;
  private loaded = false;
  private destroyed = false;
  private readonly updateListeners = new Set<Listener>();
  private readonly errorListeners = new Set<ErrorListener>();
  private readonly destroyListeners = new Set<Listener>();

  /**
   * Bounds-checked SLICE-COPY of the native guest's linear memory, exposed so the
   * platform can late-bind it onto host:canvas handles for the stage-② `present`
   * framebuffer path (canvasRegistry.bindGuestMemory). Delegates to
   * NativeGuestHost.readBytes — a copy, NEVER a live view over `memory.buffer`
   * (which would detach on `memory.grow`). Only a native guest exposes this; a JS
   * (QuickJS) guest has no such reader, so `present` stays fail-closed for it.
   *
   * Arrow field (bound to `this`) so it survives being passed by reference, i.e.
   * `canvasRegistry.bindGuestMemory(engine.readGuestMemory)`. A hostile ptr/len
   * throws inside readBytes (assertInBounds) — the caller turns that into a
   * fail-closed result; it never reads out of bounds.
   */
  readonly readGuestMemory = (ptr: number, len: number): Uint8Array =>
    this.host.readBytes(ptr, len);

  /**
   * Bounds-checked WRITE into the native guest's linear memory — the counterpart
   * of readGuestMemory, exposed so the platform can late-bind it onto host:asset
   * for the ④ `blit` path (the host decodes an asset to RGBA and writes it into a
   * buffer the guest allocated). Delegates to NativeGuestHost.writeBytes, which
   * assertInBounds BEFORE writing: a hostile dstPtr/dstCap throws (caught by the
   * host:asset impl -> fail-closed result) and never writes past guest memory.
   * Only a native guest exposes this; a JS (QuickJS) guest has no writer, so
   * host:asset.blit stays unavailable to it (mirrors host:canvas.present).
   *
   * Arrow field (bound to `this`) so it survives being passed by reference.
   */
  readonly writeGuestMemory = (ptr: number, bytes: Uint8Array): void =>
    this.host.writeBytes(ptr, bytes);

  /** Guest-declared ABI version; null before `loadBundle` or when the guest predates the export. */
  get guestAbiVersion(): number | null {
    return this.host.guestAbiVersion;
  }

  /**
   * Raw guest exports passthrough (diagnostics only — e.g. kv-guest's
   * `last_ok`). Undefined before `loadBundle` resolves, mirroring
   * guestAbiVersion's null-before-load contract; NativeGuestHost.exports itself
   * throws pre-instantiation, so the loaded-guard keeps this undefined-safe.
   */
  get exports(): WebAssembly.Exports | undefined {
    return this.loaded ? this.host.exports : undefined;
  }

  constructor(options: NativeGuestViewOptions) {
    this.wasmBytes = options.wasmBytes;
    this.registry = new ComponentRegistry();
    for (const [name, component] of Object.entries(options.components)) {
      this.registry.register(name, component);
    }
    this.host = new NativeGuestHost({
      dispatch: createHostModuleDispatch(options.contract, options.hostModules),
      onLog: options.onLog,
      onRenderBatch: (batch) => this.receiver?.applyBatch(batch),
    });
  }

  get isLoaded(): boolean {
    return this.loaded;
  }

  get isDestroyed(): boolean {
    return this.destroyed;
  }

  createReceiver(): Receiver {
    if (this.receiver) return this.receiver;
    this.receiver = new Receiver(
      this.registry,
      // Native guests don't support host->guest ref method calls yet (REF_CALL);
      // this is where that transport would go.
      () => {},
      () => {
        for (const listener of this.updateListeners) listener();
      }
    );
    return this.receiver;
  }

  getReceiver(): Receiver | null {
    return this.receiver;
  }

  // Signature matches EngineViewEngine.loadBundle. The wasm bytes are provided at
  // construction, so `_source` (the JS-guest bundle URL/code) and `_options` (JS
  // bytecode asset path) are deliberately unused — a native guest's bytes are
  // already fixed. `initialProps` IS used: it reaches the guest over the named
  // event channel (see below), matching the JS Engine's props delivery.
  async loadBundle(
    _source?: string,
    initialProps?: Record<string, unknown>,
    _options?: { bytecodeAssetPath?: string }
  ): Promise<void> {
    try {
      await this.host.load(this.wasmBytes); // keel_init registers event handlers
      if (initialProps !== undefined) {
        // JS-parity: a native guest takes initial props over the NAMED event
        // channel (events::on('props')). Delivered before drain so a guest that
        // renders from props sees them in its first settled state.
        this.host.emitEvent('props', initialProps);
      }
      await this.host.drain();
      this.loaded = true;
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      for (const listener of this.errorListeners) listener(err);
      throw err;
    }
  }

  /**
   * Forward an input / lifecycle event to the native guest (keel_on_event).
   *
   * The payload may carry `Uint8Array` values: they ride as raw KBS1 segments
   * rather than being stringified, which is how a host pushes an UNSOLICITED
   * byte stream (a forwarded socket, a serial port, a subprocess's stdout) to a
   * native guest — the guest cannot know when to pull for those. A byte-free
   * payload is encoded exactly as before.
   */
  // Reason: an event payload is any JSON-serializable value plus Uint8Array leaves, forwarded as-is.
  sendEvent(name: string, payload?: unknown): void {
    this.host.emitEvent(name, payload);
  }

  on(event: 'update', listener: Listener): () => void;
  on(event: 'error', listener: ErrorListener): () => void;
  on(event: 'destroy', listener: Listener): () => void;
  on(event: 'update' | 'error' | 'destroy', listener: Listener | ErrorListener): () => void {
    const set =
      event === 'update'
        ? this.updateListeners
        : event === 'error'
          ? this.errorListeners
          : this.destroyListeners;
    set.add(listener as Listener & ErrorListener);
    return () => set.delete(listener as Listener & ErrorListener);
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    for (const listener of this.destroyListeners) listener();
  }
}

/**
 * Construct a native WASM guest engine (a `NativeGuestEngine`, i.e.
 * EngineViewEngine + the wasm-specific surface).
 */
export function createNativeGuestEngine(options: NativeGuestViewOptions): NativeGuestView {
  return new NativeGuestView(options);
}
