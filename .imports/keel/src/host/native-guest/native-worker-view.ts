import type { HostModuleImplementationMap, KeelContractShape } from '../../contract';
import { createHostModuleDispatch } from '../../contract';
import { Receiver } from '../receiver';
import type { ComponentType } from '../registry';
import { ComponentRegistry } from '../registry';
import type { NativeGuestEngine } from './native-guest-view';
import type {
  MainToNativeWorkerMessage,
  NativeWorkerToMainMessage,
} from './native-worker-protocol';
import { deserializeNativeWorkerError, serializeNativeWorkerError } from './native-worker-protocol';

interface NativeWorkerLike {
  onmessage: ((event: MessageEvent<NativeWorkerToMainMessage>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
  postMessage(message: MainToNativeWorkerMessage, transfer?: Transferable[]): void;
  terminate(): void;
}

export interface NativeGuestWorkerKillInfo {
  turnId: number;
  budgetMs: number;
  usedMs: number;
}

export interface NativeGuestWorkerOptions {
  /** Decoded bytes for legacy loaders. Exactly one wasm source is required. */
  wasmBytes?: BufferSource;
  /** A browser-compiled module, which can be structured-cloned into the worker. */
  wasmModule?: WebAssembly.Module;
  contract: KeelContractShape;
  hostModules: HostModuleImplementationMap;
  components: Record<string, ComponentType>;
  onLog?: (message: string) => void;
  /** Cumulative guest CPU budget for one load/event turn. Host wait time is excluded. */
  watchdogTimeout?: number;
  onWatchdogKill?: (info: NativeGuestWorkerKillInfo) => void;
  /** Test/bundler hook. Production defaults to keel's native worker entry. */
  createWorker?: () => NativeWorkerLike;
}

type Listener = () => void;
type ErrorListener = (error: Error) => void;

interface TurnState {
  kind: 'load' | 'event';
  usedMs: number;
  runningSince: number | null;
  timer: ReturnType<typeof setTimeout> | null;
  resolve?: () => void;
  reject?: (error: Error) => void;
}

interface MemoryRequest {
  kind: 'read' | 'write';
  resolve(value?: Uint8Array): void;
  reject(error: Error): void;
}

const copyBufferSource = (source: BufferSource): ArrayBuffer => {
  const view =
    source instanceof ArrayBuffer
      ? new Uint8Array(source)
      : new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
  return view.slice().buffer;
};

const now = (): number => (typeof performance === 'undefined' ? Date.now() : performance.now());

/**
 * Main-thread proxy for a native guest that lives in a dedicated Worker.
 * Capability implementations remain on the main thread; pointer-backed calls
 * use bounded, on-demand memory RPCs rather than mirroring the whole Wasm heap.
 */
export class NativeGuestWorkerView implements NativeGuestEngine {
  private readonly worker: NativeWorkerLike;
  private readonly dispatch;
  private readonly registry = new ComponentRegistry();
  private readonly onLog?: (message: string) => void;
  private readonly watchdogTimeout: number;
  private readonly onWatchdogKill?: (info: NativeGuestWorkerKillInfo) => void;
  private receiver: Receiver | null = null;
  private loaded = false;
  private destroyed = false;
  private abiVersion: number | null = null;
  private turnSeq = 0;
  private memorySeq = 0;
  private activeHostCalls = 0;
  private readonly turns = new Map<number, TurnState>();
  private readonly memoryRequests = new Map<number, MemoryRequest>();
  private readonly updateListeners = new Set<Listener>();
  private readonly errorListeners = new Set<ErrorListener>();
  private readonly destroyListeners = new Set<Listener>();
  private readonly readyPromise: Promise<void>;
  private resolveReady!: () => void;
  private rejectReady!: (error: Error) => void;

  constructor(options: NativeGuestWorkerOptions) {
    const hasWasmBytes = options.wasmBytes !== undefined;
    const hasWasmModule = options.wasmModule !== undefined;
    if (hasWasmBytes === hasWasmModule) {
      throw new Error('native guest requires exactly one wasm source');
    }
    this.dispatch = createHostModuleDispatch(options.contract, options.hostModules);
    this.onLog = options.onLog;
    this.watchdogTimeout = Math.max(1, options.watchdogTimeout ?? 5000);
    this.onWatchdogKill = options.onWatchdogKill;
    for (const [name, component] of Object.entries(options.components)) {
      this.registry.register(name, component);
    }

    this.readyPromise = new Promise((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    this.worker = options.createWorker
      ? options.createWorker()
      : (new Worker(new URL('./native-worker-host.js', import.meta.url), {
          type: 'module',
        }) as unknown as NativeWorkerLike);
    this.worker.onmessage = (event) => this.handleWorkerMessage(event.data);
    this.worker.onerror = (event) =>
      this.failFatal(new Error(`[keel] native worker error: ${event.message || 'unknown'}`));

    if (options.wasmModule !== undefined) {
      // WebAssembly.Module is structured-cloneable. Keeping it compiled avoids
      // buffering and recompiling a module that the browser received as a stream.
      this.worker.postMessage({ type: 'init', wasmModule: options.wasmModule });
    } else {
      const wasmBytes = copyBufferSource(options.wasmBytes!);
      this.worker.postMessage({ type: 'init', wasmBytes }, [wasmBytes]);
    }
  }

  get isLoaded(): boolean {
    return this.loaded;
  }

  get isDestroyed(): boolean {
    return this.destroyed;
  }

  get guestAbiVersion(): number | null {
    return this.abiVersion;
  }

  /** Worker-owned exports cannot cross structured clone. */
  get exports(): WebAssembly.Exports | undefined {
    return undefined;
  }

  readonly readGuestMemory = (ptr: number, len: number): Promise<Uint8Array> => {
    if (this.activeHostCalls < 1) {
      return Promise.reject(new Error('guest memory is only available during a host call'));
    }
    const requestId = ++this.memorySeq;
    return new Promise<Uint8Array>((resolve, reject) => {
      this.memoryRequests.set(requestId, {
        kind: 'read',
        resolve: (value) => resolve(value ?? new Uint8Array()),
        reject,
      });
      this.post({ type: 'memoryRead', requestId, ptr, len });
    });
  };

  readonly writeGuestMemory = (ptr: number, bytes: Uint8Array): Promise<void> => {
    if (this.activeHostCalls < 1) {
      return Promise.reject(new Error('guest memory is only available during a host call'));
    }
    const requestId = ++this.memorySeq;
    const copy = bytes.slice().buffer;
    return new Promise<void>((resolve, reject) => {
      this.memoryRequests.set(requestId, {
        kind: 'write',
        resolve: () => resolve(),
        reject,
      });
      this.post({ type: 'memoryWrite', requestId, ptr, bytes: copy }, [copy]);
    });
  };

  createReceiver(): Receiver {
    if (this.receiver) return this.receiver;
    this.receiver = new Receiver(
      this.registry,
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

  async loadBundle(
    _source?: string,
    initialProps?: Record<string, unknown>,
    _options?: { bytecodeAssetPath?: string }
  ): Promise<void> {
    await this.readyPromise;
    if (this.destroyed) throw new Error('native worker is destroyed');
    const turnId = ++this.turnSeq;
    const done = new Promise<void>((resolve, reject) => {
      this.turns.set(turnId, {
        kind: 'load',
        usedMs: 0,
        runningSince: null,
        timer: null,
        resolve,
        reject,
      });
    });
    this.post({ type: 'load', turnId, initialProps });
    return done;
  }

  // Reason: named native events carry arbitrary contract-defined payloads.
  sendEvent(name: string, payload?: unknown): void {
    // Keep the shared EngineViewEngine fire-and-forget surface. Errors are
    // already emitted through the engine's error listeners; consume the
    // acknowledgement rejection so legacy callers do not create an unhandled
    // promise.
    void this.sendEventAndWait(name, payload).catch(() => {});
  }

  /**
   * Queue a named event and resolve only after the worker has run its guest
   * handler and drained any work it woke. This acknowledgement is what lets a
   * bounded byte transport return receive credit without merely moving an
   * unbounded queue from the socket into Worker.postMessage.
   */
  // Reason: named native events carry arbitrary contract-defined payloads.
  sendEventAndWait(name: string, payload?: unknown): Promise<void> {
    if (!this.loaded || this.destroyed) {
      return Promise.reject(new Error('native guest is not available for event delivery'));
    }
    const turnId = ++this.turnSeq;
    let rejectDone!: (error: Error) => void;
    const done = new Promise<void>((resolve, reject) => {
      rejectDone = reject;
      this.turns.set(turnId, {
        kind: 'event',
        usedMs: 0,
        runningSince: null,
        timer: null,
        resolve,
        reject,
      });
    });
    try {
      this.post({ type: 'event', turnId, name, payload });
    } catch (error) {
      this.turns.delete(turnId);
      const err = error instanceof Error ? error : new Error(String(error));
      this.emitError(err);
      rejectDone(err);
    }
    return done;
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
    try {
      this.worker.postMessage({ type: 'destroy' });
    } catch {
      // The worker may already have exited after a fatal error.
    }
    this.worker.terminate();
    const error = new Error('native worker destroyed');
    this.rejectReady(error);
    this.rejectOutstanding(error);
    for (const listener of this.destroyListeners) listener();
  }

  private post(message: MainToNativeWorkerMessage, transfer?: Transferable[]): void {
    if (this.destroyed) throw new Error('native worker is destroyed');
    this.worker.postMessage(message, transfer);
  }

  private handleWorkerMessage(message: NativeWorkerToMainMessage): void {
    if (this.destroyed) return;
    switch (message.type) {
      case 'ready':
        this.resolveReady();
        return;
      case 'loaded':
        this.loaded = true;
        this.abiVersion = message.abiVersion;
        this.finishTurn(message.turnId);
        return;
      case 'turnDone':
        this.finishTurn(message.turnId);
        return;
      case 'turnError': {
        const error = deserializeNativeWorkerError(message.error);
        this.finishTurn(message.turnId, error);
        return;
      }
      case 'guestState':
        this.updateGuestState(message.turnId, message.running);
        return;
      case 'hostCall':
        void this.handleHostCall(message);
        return;
      case 'memoryReadResult':
      case 'memoryWriteResult':
        this.finishMemoryRequest(message);
        return;
      case 'batch':
        try {
          this.receiver?.applyBatch(message.batch);
        } catch (error) {
          this.emitError(error instanceof Error ? error : new Error(String(error)));
        }
        return;
      case 'log':
        this.onLog?.(message.message);
        return;
    }
  }

  private async handleHostCall(
    message: Extract<NativeWorkerToMainMessage, { type: 'hostCall' }>
  ): Promise<void> {
    this.activeHostCalls++;
    try {
      const handler = this.dispatch[message.moduleId]?.[message.method];
      if (typeof handler !== 'function') {
        throw new Error(`host module not registered: ${message.moduleId}.${message.method}`);
      }
      const result = await handler(message.input);
      this.post({ type: 'hostResult', callId: message.callId, ok: true, result });
    } catch (error) {
      try {
        this.post({
          type: 'hostResult',
          callId: message.callId,
          ok: false,
          error: serializeNativeWorkerError(error),
        });
      } catch (postError) {
        this.failFatal(postError instanceof Error ? postError : new Error(String(postError)));
      }
    } finally {
      this.activeHostCalls--;
    }
  }

  private finishMemoryRequest(
    message: Extract<NativeWorkerToMainMessage, { type: 'memoryReadResult' | 'memoryWriteResult' }>
  ): void {
    const pending = this.memoryRequests.get(message.requestId);
    if (!pending) return;
    this.memoryRequests.delete(message.requestId);
    if (!message.ok) {
      pending.reject(
        message.error ? deserializeNativeWorkerError(message.error) : new Error('memory RPC failed')
      );
      return;
    }
    if (message.type === 'memoryReadResult') {
      pending.resolve(new Uint8Array(message.bytes ?? new ArrayBuffer(0)));
    } else {
      pending.resolve();
    }
  }

  private updateGuestState(turnId: number, running: boolean): void {
    const turn = this.turns.get(turnId);
    if (!turn) return;
    if (running) {
      if (turn.runningSince !== null) return;
      turn.runningSince = now();
      this.armWatchdog(turnId, turn);
      return;
    }
    if (turn.runningSince !== null) {
      turn.usedMs += Math.max(0, now() - turn.runningSince);
      turn.runningSince = null;
    }
    if (turn.timer) clearTimeout(turn.timer);
    turn.timer = null;
  }

  private armWatchdog(turnId: number, turn: TurnState): void {
    if (turn.timer) clearTimeout(turn.timer);
    const remaining = Math.max(0, this.watchdogTimeout - turn.usedMs);
    turn.timer = setTimeout(() => {
      const running = turn.runningSince === null ? 0 : Math.max(0, now() - turn.runningSince);
      const usedMs = turn.usedMs + running;
      this.onWatchdogKill?.({ turnId, budgetMs: this.watchdogTimeout, usedMs });
      this.failFatal(
        new Error(
          `[keel] native guest watchdog exceeded ${this.watchdogTimeout}ms of guest CPU in turn ${turnId}`
        )
      );
    }, remaining);
  }

  private finishTurn(turnId: number, error?: Error): void {
    const turn = this.turns.get(turnId);
    if (!turn) return;
    if (turn.timer) clearTimeout(turn.timer);
    this.turns.delete(turnId);
    if (error) {
      turn.reject?.(error);
      this.emitError(error);
    } else {
      turn.resolve?.();
    }
  }

  private failFatal(error: Error): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.rejectReady(error);
    this.worker.terminate();
    this.rejectOutstanding(error);
    this.emitError(error);
    for (const listener of this.destroyListeners) listener();
  }

  private rejectOutstanding(error: Error): void {
    for (const turn of this.turns.values()) {
      if (turn.timer) clearTimeout(turn.timer);
      turn.reject?.(error);
    }
    this.turns.clear();
    for (const request of this.memoryRequests.values()) request.reject(error);
    this.memoryRequests.clear();
  }

  private emitError(error: Error): void {
    for (const listener of this.errorListeners) listener(error);
  }
}

export function createNativeGuestWorkerEngine(
  options: NativeGuestWorkerOptions
): NativeGuestWorkerView {
  return new NativeGuestWorkerView(options);
}
