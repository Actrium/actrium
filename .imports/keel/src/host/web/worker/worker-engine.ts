/**
 * WorkerEngine (issue #19, L1) — main-thread proxy for an Engine that runs off the UI thread.
 *
 * The real {@link Engine} + QuickJS-WASM sandbox live in a Web Worker (see `worker-host.ts`), so
 * a guest infinite loop or heavy compute no longer freezes the host page. Two robustness wins an
 * integrator cannot get without forking rill:
 *  - **off-main-thread execution**: guest CPU is on the worker;
 *  - **hard kill**: a per-turn watchdog `terminate()`s a runaway guest (a true kill, not a flag).
 *
 * This class implements the slice of the engine surface that {@link useEngineView} needs
 * (`createReceiver`/`getReceiver`/`loadBundle`/`sendEvent`/`on`/lifecycle), so a `WebEngineView`
 * can drive it exactly like an in-thread `Engine`. The Receiver lives here on the main thread
 * (that is where react-dom renders); the worker ships SERIALIZED render batches, which this
 * class decodes — turning each `{__fnId}` marker back into a proxy that invokes the guest
 * callback over postMessage.
 *
 * Not yet bridged across the worker in v1 (fail-closed or no-op, tracked as follow-ups):
 *  - `host:*` capability modules (the integrator's async host calls);
 *  - `useRemoteRef` (host→guest REF_CALL);
 *  - guest-callback release (the guest registry's own GC covers unmounted trees).
 */

import type { ReviewedUnknown } from '../../../shared';
import { CallbackRegistryImpl as CallbackRegistry } from '../../../shared';
import { Bridge } from '../../../shared/bridge/bridge';
import { Receiver } from '../../receiver';
import { type ComponentMap, ComponentRegistry } from '../../registry';
import type { BridgeValueObject, OperationBatch, SerializedOperationBatch } from '../../types';
import {
  type DbgBreakpointResult,
  type DbgCallFrame,
  type DbgEvalResult,
  type DbgPauseReason,
  type DbgPropertyDescriptor,
  type DbgScript,
  type DbgSetBreakpointOptions,
  type DbgStepAction,
  deserializeWorkerError,
  type MainToWorkerMessage,
  type WorkerSandbox,
  type WorkerToMainMessage,
} from './protocol';

/** Payload of the {@link WorkerEngineEventMap.paused} event. */
export interface DbgPausedEvent {
  turnId: number;
  reason: DbgPauseReason;
  callFrames: DbgCallFrame[];
  hitBreakpoints: string[];
}

/** Payload of the {@link WorkerEngineEventMap.resumed} event. */
export interface DbgResumedEvent {
  turnId: number;
}

/** Events emitted by a {@link WorkerEngine}. Mirrors the subset of the core engine's events. */
export interface WorkerEngineEventMap {
  update: () => void;
  error: (error: Error) => void;
  destroy: () => void;
  load: () => void;
  fatalError: (error: Error) => void;
  // Reason: guest->host message payload is any serializable value
  message: (message: { event: string; payload: unknown }) => void;
  operation: (batch: OperationBatch) => void;
  /** Guest execution paused at a breakpoint (distinct from the clock-freeze `pause()`). */
  paused: (info: DbgPausedEvent) => void;
  /** Guest execution resumed from a breakpoint pause. */
  resumed: (info: DbgResumedEvent) => void;
  /** A guest script was parsed (Debugger.scriptParsed source). */
  scriptParsed: (script: DbgScript) => void;
}

/** Information passed to {@link WorkerEngineOptions.onWatchdogKill}. */
export interface WatchdogKillInfo {
  /** Internal turn id of the guest turn that overran. */
  turnId: number;
  /** What kind of turn overran: bundle load, host event, or guest-callback invocation. */
  kind: 'load' | 'event' | 'invoke';
  /** The configured budget (ms) that was exceeded. */
  timeoutMs: number;
}

/** Options for {@link WorkerEngine}. */
export interface WorkerEngineOptions {
  /** Sandbox backend to run in the worker. Only `wasm-quickjs` is valid off-thread. */
  sandbox?: WorkerSandbox;
  /** QuickJS `.wasm` bytes (transferred to the worker; no fetch — strict-CSP friendly). */
  wasmBinary?: Uint8Array | ArrayBuffer;
  /** Override the `.wasm` URL the in-worker loader fetches (when `wasmBinary` is not given). */
  wasmPath?: string;
  /** Inner engine execution timeout (ms) passed through to the worker's Engine. */
  timeout?: number;
  /** Enable debug logging in the worker's Engine. */
  debug?: boolean;
  /**
   * Factory for the backing Worker. Defaults to
   * `new Worker(new URL('./worker-host.js', import.meta.url), { type: 'module' })`, which a
   * bundler that understands the `new URL(..., import.meta.url)` worker idiom (Vite, webpack,
   * esbuild) resolves automatically. Pass this to control worker creation explicitly.
   */
  createWorker?: () => Worker;
  /**
   * Watchdog budget (ms): the maximum a single guest turn (bundle load, host event, or callback)
   * may run before the worker is `terminate()`d as a runaway. `0` disables the watchdog. The
   * value is integrator policy. Note the first load also pays WASM instantiation. Default 5000.
   */
  watchdogTimeout?: number;
  /** Called when the watchdog hard-kills a runaway guest. Integrator decides what to do next. */
  onWatchdogKill?: (info: WatchdogKillInfo) => void;
  /** Max operations per batch applied by the main-thread Receiver. */
  receiverMaxBatchSize?: number;
}

type Listener = (arg?: unknown) => void;

let _workerEngineSeq = 0;

function toArrayBuffer(bin: Uint8Array | ArrayBuffer): ArrayBuffer {
  if (bin instanceof ArrayBuffer) {
    return bin;
  }
  // Copy out an exact-length buffer (the view may be a window into a larger buffer).
  return bin.slice().buffer;
}

export class WorkerEngine {
  readonly id: string;

  #worker: Worker;
  #registry = new ComponentRegistry();
  #callbacks = new CallbackRegistry();
  #bridge: Bridge;
  #receiver: Receiver | null = null;

  #listeners = new Map<keyof WorkerEngineEventMap, Set<Listener>>();

  #ready = false;
  #loaded = false;
  #destroyed = false;
  #paused = false;

  #resolveReady!: () => void;
  #rejectReady!: (error: Error) => void;
  #readyPromise: Promise<void>;
  #preReadyQueue: Array<() => void> = [];

  #turnSeq = 0;
  #watchdogs = new Map<number, ReturnType<typeof setTimeout>>();
  #watchdogKinds = new Map<number, WatchdogKillInfo['kind']>();
  #pendingLoads = new Map<number, { resolve: () => void; reject: (error: Error) => void }>();

  #watchdogTimeout: number;
  #onWatchdogKill?: (info: WatchdogKillInfo) => void;
  #receiverMaxBatchSize?: number;
  #warnedReceiverToSandbox = false;

  // Debugger (dbg.*) sub-protocol state. requestId correlates a dbg request to its
  // reply (ack/breakpointResolved/evalResult). Turns paused at a breakpoint have
  // their watchdog kind parked here so it can be rearmed on resume.
  #dbgSeq = 0;
  // Reason: dbg replies resolve with differently-shaped serializable values
  #dbgPending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (e: Error) => void }
  >();
  #pausedKinds = new Map<number, WatchdogKillInfo['kind']>();

  // Debugger-suspend watchdog hold. While the worker's TurnGate is suspended
  // (an explicit dbgPause() or a breakpoint `dbg.paused`), guest-eval turns
  // posted to the worker are QUEUED behind the gate, not running — so their
  // watchdogs must not count down: their `turnDone` cannot arrive until the
  // debugger resumes, and letting the timers run would terminate() a perfectly
  // healthy worker. Kinds of held turns are parked in #heldWatchdogKinds and
  // rearmed with a fresh budget once no suspend reason remains.
  /** True between a dbgPause() request and the matching dbgResume()/dbgDisable(). */
  #dbgExplicitPause = false;
  /** turnIds currently parked at a breakpoint (`dbg.paused` seen, `dbg.resumed` not yet). */
  #dbgPausedTurns = new Set<number>();
  /** Whether the watchdog hold is currently in effect (derived, cached for transitions). */
  #dbgSuspendActive = false;
  /** Watchdog kinds of turns held while the debugger suspend is active. */
  #heldWatchdogKinds = new Map<number, WatchdogKillInfo['kind']>();

  constructor(options: WorkerEngineOptions = {}) {
    _workerEngineSeq += 1;
    this.id = `worker-engine-${_workerEngineSeq}`;
    this.#watchdogTimeout = options.watchdogTimeout ?? 5000;
    this.#onWatchdogKill = options.onWatchdogKill;
    this.#receiverMaxBatchSize = options.receiverMaxBatchSize;

    // Main-thread bridge: decode serialized batches from the worker into live operation batches.
    // Decoded callbacks become proxies that invoke the guest callback over postMessage.
    this.#bridge = new Bridge({
      callbackRegistry: this.#callbacks,
      guestInvoker: (fnId, args) => {
        this.#invokeGuestCallback(fnId, args);
        // Fire-and-forget across the worker boundary: UI event handlers don't use the return.
        return undefined;
      },
      guestReleaseCallback: (fnId) => this.#enqueue(() => this.#postRaw({ type: 'release', fnId })),
      onGuestOperations: (batch) => {
        this.#receiver?.applyBatch(batch);
      },
      onHostMessage: () => {
        // Main thread never sends host messages through this bridge; sendEvent posts directly.
      },
    });

    this.#readyPromise = new Promise<void>((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });

    this.#worker = options.createWorker
      ? options.createWorker()
      : new Worker(new URL('./worker-host.js', import.meta.url), { type: 'module' });

    this.#worker.onmessage = (event: MessageEvent) => {
      this.#onWorkerMessage(event.data as WorkerToMainMessage);
    };
    this.#worker.onerror = (event: ErrorEvent) => {
      this.#emit('error', new Error(`[rill] worker error: ${event.message ?? 'unknown'}`));
    };

    const init: MainToWorkerMessage = {
      type: 'init',
      sandbox: options.sandbox ?? 'wasm-quickjs',
      wasmBinary: options.wasmBinary ? toArrayBuffer(options.wasmBinary) : undefined,
      wasmPath: options.wasmPath,
      timeout: options.timeout,
      debug: options.debug,
    };
    this.#postRaw(init);
  }

  // ============================================
  // Engine surface used by useEngineView
  // ============================================

  register(components: ComponentMap): void {
    this.#registry.registerAll(components);
  }

  getRegistry(): ComponentRegistry {
    return this.#registry;
  }

  createReceiver(): Receiver {
    this.#receiver = new Receiver(
      this.#registry,
      (message) => this.#onReceiverToSandbox(message),
      () => this.#emit('update'),
      {
        maxBatchSize: this.#receiverMaxBatchSize,
        releaseCallback: (fnId) => this.#bridge.releaseCallback(fnId),
      }
    );
    return this.#receiver;
  }

  getReceiver(): Receiver | null {
    return this.#receiver;
  }

  loadBundle(source: string, initialProps?: Record<string, unknown>): Promise<void> {
    if (this.#destroyed) {
      return Promise.reject(new Error('[rill] WorkerEngine is destroyed'));
    }
    return this.#readyPromise.then(
      () =>
        new Promise<void>((resolve, reject) => {
          const turnId = this.#nextTurn();
          this.#pendingLoads.set(turnId, { resolve, reject });
          this.#armWatchdog(turnId, 'load');
          this.#postRaw({ type: 'load', turnId, source, initialProps });
        })
    );
  }

  // Reason: host event payload is any serializable value (matches Engine.sendEvent)
  sendEvent(eventName: string, payload?: unknown): void {
    if (this.#destroyed) return;
    this.#enqueue(() => {
      const turnId = this.#nextTurn();
      this.#armWatchdog(turnId, 'event');
      this.#postRaw({ type: 'event', turnId, eventName, payload });
    });
  }

  updateConfig(config: BridgeValueObject): void {
    if (this.#destroyed) return;
    this.#enqueue(() => this.#postRaw({ type: 'config', config }));
  }

  pause(): void {
    if (this.#destroyed || this.#paused) return;
    this.#paused = true;
    this.#enqueue(() => this.#postRaw({ type: 'pause' }));
  }

  resume(): void {
    if (this.#destroyed || !this.#paused) return;
    this.#paused = false;
    this.#enqueue(() => this.#postRaw({ type: 'resume' }));
  }

  // ============================================
  // Debugger (dbg.*) surface — Milestone B web side
  //
  // These drive a Chrome DevTools Protocol Debugger conversation across the
  // worker boundary. They are DISTINCT from the turn-level pause()/resume()
  // above, which only freeze the guest clock (TimerManager). A breakpoint pause
  // is an intentional, unbounded stop: on `dbg.paused` the offending turn's
  // watchdog is disarmed so it is never mistaken for a runaway and terminated;
  // on `dbg.resumed` it is rearmed.
  //
  // The suspend/resume the guest actually performs at a breakpoint is driven by
  // the QuickJS-asyncify debug wasm (Milestone A) inside the worker; wiring that
  // eval-entry gate is deferred (see worker-host.ts / TurnGate).
  // ============================================

  /** Enable the guest debugger. */
  dbgEnable(): Promise<void> {
    return this.#dbgRequest((requestId) => ({ type: 'dbg.enable', requestId })).then(
      () => undefined
    );
  }

  /** Disable the guest debugger and clear its breakpoints. */
  dbgDisable(): Promise<void> {
    // Leaving debug drains the worker's gate unconditionally: drop every suspend
    // reason so held watchdogs rearm and queued turns are accounted for again.
    if (!this.#destroyed) {
      this.#dbgExplicitPause = false;
      this.#dbgPausedTurns.clear();
      this.#updateDbgSuspension();
    }
    return this.#dbgRequest((requestId) => ({ type: 'dbg.disable', requestId })).then(
      () => undefined
    );
  }

  /** Set a breakpoint; resolves once the worker reports the resolved location. */
  dbgSetBreakpoint(options: DbgSetBreakpointOptions): Promise<DbgBreakpointResult> {
    return this.#dbgRequest((requestId) => ({
      type: 'dbg.setBreakpoint',
      requestId,
      ...options,
    })) as Promise<DbgBreakpointResult>;
  }

  /** Remove a previously set breakpoint. */
  dbgRemoveBreakpoint(breakpointId: string): Promise<void> {
    return this.#dbgRequest((requestId) => ({
      type: 'dbg.removeBreakpoint',
      requestId,
      breakpointId,
    })).then(() => undefined);
  }

  /** Request that the guest pause at the next opportunity. */
  dbgPause(): Promise<void> {
    // Mark the suspend BEFORE posting: the worker processes `dbg.pause` ahead of
    // any turn posted after this call (postMessage is FIFO), so every later turn
    // will queue behind the gate — its watchdog must be held from the start.
    if (!this.#destroyed) {
      this.#dbgExplicitPause = true;
      this.#updateDbgSuspension();
    }
    return this.#dbgRequest((requestId) => ({ type: 'dbg.pause', requestId })).then(
      () => undefined
    );
  }

  /** Resume the guest from a breakpoint pause. */
  dbgResume(): Promise<void> {
    if (!this.#destroyed) {
      this.#dbgExplicitPause = false;
      this.#updateDbgSuspension();
    }
    return this.#dbgRequest((requestId) => ({ type: 'dbg.resume', requestId })).then(
      () => undefined
    );
  }

  /** Step the paused guest (over/into/out). */
  dbgStep(action: DbgStepAction): Promise<void> {
    return this.#dbgRequest((requestId) => ({ type: 'dbg.step', requestId, action })).then(
      () => undefined
    );
  }

  /** Evaluate an expression in the scope of a paused call frame. */
  dbgEvaluateOnCallFrame(callFrameId: string, expression: string): Promise<DbgEvalResult> {
    return this.#dbgRequest((requestId) => ({
      type: 'dbg.evaluateOnCallFrame',
      requestId,
      callFrameId,
      expression,
    })) as Promise<DbgEvalResult>;
  }

  /**
   * Read the own properties of a paused scope/object handle (expands a scope in
   * DevTools). Like {@link dbgEvaluateOnCallFrame} this is a control-plane query
   * against the already-suspended turn: it does not open a new guest-eval turn,
   * so it resolves even while the guest is parked at a breakpoint.
   */
  dbgGetProperties(objectId: string): Promise<DbgPropertyDescriptor[]> {
    return this.#dbgRequest((requestId) => ({
      type: 'dbg.getProperties',
      requestId,
      objectId,
    })) as Promise<DbgPropertyDescriptor[]>;
  }

  on<K extends keyof WorkerEngineEventMap>(
    event: K,
    listener: WorkerEngineEventMap[K] extends () => void
      ? () => void
      : (data: Parameters<WorkerEngineEventMap[K]>[0]) => void
  ): () => void {
    let set = this.#listeners.get(event);
    if (!set) {
      set = new Set();
      this.#listeners.set(event, set);
    }
    set.add(listener as Listener);
    return () => {
      this.#listeners.get(event)?.delete(listener as Listener);
    };
  }

  get isLoaded(): boolean {
    return this.#loaded;
  }

  get isDestroyed(): boolean {
    return this.#destroyed;
  }

  get isPaused(): boolean {
    return this.#paused;
  }

  destroy(): void {
    if (this.#destroyed) return;
    this.#destroyed = true;
    this.#clearAllWatchdogs();
    const destroyError = new Error('[rill] WorkerEngine destroyed');
    this.#rejectAllPendingLoads(destroyError);
    this.#rejectAllDbgPending(destroyError);
    try {
      this.#postRaw({ type: 'destroy' });
    } catch {
      // worker may already be gone
    }
    this.#worker.terminate();
    this.#receiver?.clear();
    this.#receiver = null;
    this.#emit('destroy');
  }

  // ============================================
  // Internals
  // ============================================

  #nextTurn(): number {
    this.#turnSeq += 1;
    return this.#turnSeq;
  }

  // Reason: dbg replies resolve with differently-shaped serializable values
  #dbgRequest(build: (requestId: number) => MainToWorkerMessage): Promise<unknown> {
    if (this.#destroyed) {
      return Promise.reject(new Error('[rill] WorkerEngine is destroyed'));
    }
    // Reason: dbg replies resolve with differently-shaped serializable values
    return new Promise<unknown>((resolve, reject) => {
      this.#dbgSeq += 1;
      const requestId = this.#dbgSeq;
      this.#dbgPending.set(requestId, { resolve, reject });
      this.#enqueue(() => this.#postRaw(build(requestId)));
    });
  }

  // Reason: dbg replies resolve with differently-shaped serializable values
  #resolveDbg(requestId: number, value: unknown): void {
    const pending = this.#dbgPending.get(requestId);
    if (!pending) return;
    this.#dbgPending.delete(requestId);
    pending.resolve(value);
  }

  #rejectAllDbgPending(error: Error): void {
    for (const pending of this.#dbgPending.values()) {
      pending.reject(error);
    }
    this.#dbgPending.clear();
    this.#pausedKinds.clear();
    this.#dbgPausedTurns.clear();
    this.#dbgExplicitPause = false;
    this.#dbgSuspendActive = false;
  }

  /** Run now if the worker is ready, otherwise queue until the `ready` message arrives. */
  #enqueue(action: () => void): void {
    if (this.#ready) {
      action();
    } else {
      this.#preReadyQueue.push(action);
    }
  }

  #postRaw(message: MainToWorkerMessage): void {
    this.#worker.postMessage(message);
  }

  #invokeGuestCallback(fnId: string, args: ReviewedUnknown[]): void {
    if (this.#destroyed) return;
    const turnId = this.#nextTurn();
    this.#armWatchdog(turnId, 'invoke');
    this.#postRaw({ type: 'invoke', turnId, fnId, args: args as unknown[] });
  }

  // Reason: Receiver→sandbox messages (REF_CALL) are an opaque host-message shape, dropped in v1
  #onReceiverToSandbox(_message: unknown): void {
    if (!this.#warnedReceiverToSandbox) {
      this.#warnedReceiverToSandbox = true;
      // Remote refs (useRemoteRef → host REF_CALL) are not bridged across the worker in v1.
      console.warn(
        '[rill/host/web] WorkerEngine: host→guest REF_CALL (useRemoteRef) is not supported in ' +
          'worker mode yet; the call was dropped.'
      );
    }
  }

  #armWatchdog(turnId: number, kind: WatchdogKillInfo['kind']): void {
    if (this.#watchdogTimeout <= 0) return;
    if (this.#dbgSuspendActive) {
      // The worker's gate is suspended by the debugger: this turn is queued, not
      // running. Hold its watchdog; #updateDbgSuspension rearms it on resume.
      this.#heldWatchdogKinds.set(turnId, kind);
      console.debug(
        `[rill/host/web] WorkerEngine: watchdog for turn ${turnId} (${kind}) held — debugger suspend active`
      );
      return;
    }
    this.#watchdogKinds.set(turnId, kind);
    const timer = setTimeout(() => this.#onWatchdogFire(turnId), this.#watchdogTimeout);
    this.#watchdogs.set(turnId, timer);
  }

  #disarmWatchdog(turnId: number): void {
    const timer = this.#watchdogs.get(turnId);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.#watchdogs.delete(turnId);
    }
    this.#watchdogKinds.delete(turnId);
    this.#heldWatchdogKinds.delete(turnId);
  }

  #clearAllWatchdogs(): void {
    for (const timer of this.#watchdogs.values()) {
      clearTimeout(timer);
    }
    this.#watchdogs.clear();
    this.#watchdogKinds.clear();
    this.#heldWatchdogKinds.clear();
  }

  /**
   * Recompute whether the debugger has the worker's turn gate suspended, and on a
   * transition either hold every armed watchdog (suspend: queued turns are not
   * running; counting them would terminate() a healthy worker) or rearm every
   * held one with a fresh budget (resume: turns start draining now).
   */
  #updateDbgSuspension(): void {
    const suspended = this.#dbgExplicitPause || this.#dbgPausedTurns.size > 0;
    if (suspended === this.#dbgSuspendActive) return;
    this.#dbgSuspendActive = suspended;
    if (suspended) {
      for (const [turnId, timer] of this.#watchdogs) {
        clearTimeout(timer);
        this.#heldWatchdogKinds.set(turnId, this.#watchdogKinds.get(turnId) ?? 'event');
      }
      this.#watchdogs.clear();
      this.#watchdogKinds.clear();
      if (this.#heldWatchdogKinds.size > 0) {
        console.debug(
          `[rill/host/web] WorkerEngine: debugger suspend — holding ${this.#heldWatchdogKinds.size} turn watchdog(s)`
        );
      }
    } else {
      const held = Array.from(this.#heldWatchdogKinds);
      this.#heldWatchdogKinds.clear();
      if (held.length > 0) {
        console.debug(
          `[rill/host/web] WorkerEngine: debugger resumed — rearming ${held.length} turn watchdog(s)`
        );
      }
      for (const [turnId, kind] of held) {
        this.#armWatchdog(turnId, kind);
      }
    }
  }

  #onWatchdogFire(turnId: number): void {
    if (this.#destroyed) return;
    const kind = this.#watchdogKinds.get(turnId) ?? 'event';
    this.#disarmWatchdog(turnId);

    const error = new Error(
      `[rill] worker watchdog: guest turn exceeded ${this.#watchdogTimeout}ms (${kind}); worker terminated`
    );

    // Hard kill: a synchronously-wedged worker can't be recovered, only terminated.
    this.#destroyed = true;
    this.#clearAllWatchdogs();
    this.#worker.terminate();
    this.#rejectAllPendingLoads(error);
    this.#rejectAllDbgPending(error);

    this.#onWatchdogKill?.({ turnId, kind, timeoutMs: this.#watchdogTimeout });
    this.#emit('fatalError', error);
    this.#emit('error', error);
  }

  #rejectAllPendingLoads(error: Error): void {
    for (const pending of this.#pendingLoads.values()) {
      pending.reject(error);
    }
    this.#pendingLoads.clear();
  }

  #onWorkerMessage(message: WorkerToMainMessage): void {
    switch (message.type) {
      case 'ready': {
        this.#ready = true;
        this.#resolveReady();
        const queued = this.#preReadyQueue;
        this.#preReadyQueue = [];
        for (const action of queued) {
          action();
        }
        break;
      }
      case 'initError':
        this.#rejectReady(deserializeWorkerError(message.error));
        this.#emit('error', deserializeWorkerError(message.error));
        break;
      case 'batch':
        this.#applySerializedBatch(message.batch);
        break;
      case 'message':
        this.#emit('message', { event: message.event, payload: message.payload });
        break;
      case 'loaded': {
        this.#disarmWatchdog(message.turnId);
        this.#loaded = true;
        const pending = this.#pendingLoads.get(message.turnId);
        this.#pendingLoads.delete(message.turnId);
        pending?.resolve();
        this.#emit('load');
        break;
      }
      case 'loadError': {
        this.#disarmWatchdog(message.turnId);
        const pending = this.#pendingLoads.get(message.turnId);
        this.#pendingLoads.delete(message.turnId);
        pending?.reject(deserializeWorkerError(message.error));
        break;
      }
      case 'turnDone':
        this.#disarmWatchdog(message.turnId);
        break;
      case 'engineError':
        this.#emit('error', deserializeWorkerError(message.error));
        break;
      case 'fatal': {
        const error = deserializeWorkerError(message.error);
        this.#emit('fatalError', error);
        this.#emit('error', error);
        break;
      }
      case 'dbg.ack':
        this.#resolveDbg(message.requestId, undefined);
        break;
      case 'dbg.breakpointResolved':
        this.#resolveDbg(message.requestId, {
          breakpointId: message.breakpointId,
          location: message.location,
        });
        break;
      case 'dbg.evalResult':
        this.#resolveDbg(message.requestId, {
          ok: message.ok,
          value: message.value,
          error: message.error,
        });
        break;
      case 'dbg.propertiesResult':
        this.#resolveDbg(message.requestId, message.properties);
        break;
      case 'dbg.paused': {
        // A breakpoint is an intentional, unbounded pause: disarm the offending
        // turn's watchdog so it is not mistaken for a runaway and terminate()d.
        // Park its kind so `dbg.resumed` can rearm the same budget.
        const kind =
          this.#watchdogKinds.get(message.turnId) ?? this.#heldWatchdogKinds.get(message.turnId);
        this.#disarmWatchdog(message.turnId);
        if (kind) this.#pausedKinds.set(message.turnId, kind);
        // While this turn is parked the worker's gate is suspended, so every
        // OTHER pending turn is queued, not running: hold their watchdogs too.
        this.#dbgPausedTurns.add(message.turnId);
        this.#updateDbgSuspension();
        this.#emit('paused', {
          turnId: message.turnId,
          reason: message.reason,
          callFrames: message.callFrames,
          hitBreakpoints: message.hitBreakpoints,
        });
        break;
      }
      case 'dbg.resumed': {
        const kind = this.#pausedKinds.get(message.turnId);
        this.#pausedKinds.delete(message.turnId);
        // Drop this suspend reason first so held watchdogs rearm; if another
        // pause is still outstanding, rearming parks them (and this turn) again.
        this.#dbgPausedTurns.delete(message.turnId);
        this.#updateDbgSuspension();
        if (kind) this.#armWatchdog(message.turnId, kind);
        this.#emit('resumed', { turnId: message.turnId });
        break;
      }
      case 'dbg.scriptParsed':
        this.#emit('scriptParsed', message.script);
        break;
    }
  }

  #applySerializedBatch(batch: SerializedOperationBatch): void {
    if (this.#destroyed) return;
    // Decode → onGuestOperations → receiver.applyBatch → receiver schedules an 'update'.
    this.#bridge.sendSerializedBatch(batch);
  }

  #emit<K extends keyof WorkerEngineEventMap>(
    event: K,
    arg?: Parameters<WorkerEngineEventMap[K]>[0]
  ): void {
    const set = this.#listeners.get(event);
    if (!set) return;
    for (const listener of set) {
      listener(arg);
    }
  }
}

/** Convenience constructor mirroring `new WorkerEngine(options)`. */
export function createWorkerEngine(options?: WorkerEngineOptions): WorkerEngine {
  return options ? new WorkerEngine(options) : new WorkerEngine();
}
