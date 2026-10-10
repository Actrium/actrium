import {
  call as hostCall,
  callRaw as hostCallRaw,
  discover as hostDiscover,
  logMessage as hostLogMessage,
  registerStream as hostRegisterStream,
  reply as hostReply,
  replyError as hostReplyError,
  resolveDependency as hostResolveDependency,
  sendDataChunk as hostSendDataChunk,
  tell as hostTell,
  unregisterStream as hostUnregisterStream,
} from 'actr:workload/host@0.8.0';

export type PayloadBytes = Uint8Array | ArrayBuffer | ArrayLike<number>;

// V2 `rpc-envelope` is `{request-id, route-key, payload, timeout-ms}`. `method` is kept as
// a friendly alias for `route-key` so existing generated dispatchers that route
// on `envelope.method` keep working; `route-key` and `request-id` expose the
// raw V2 fields. The V1 `contentType` / `correlationId` / `deadlineMs` fields do
// not exist in V2 and are intentionally dropped (per the V2 WIT types interface).
export interface RpcEnvelope {
  method: string;
  routeKey: string;
  requestId: string;
  payload?: Uint8Array;
  /** Request timeout in milliseconds; responses and tells carry zero. */
  timeoutMs: number;
}

export interface ActrType {
  publisher: string;
  name: string;
  version: string;
}

export interface ActrId {
  /** Realm name, `<realm_id>@<domain>`. */
  realm: string;
  serialNumber: bigint | number;
}

export interface MetadataEntry {
  key: string;
  value: string;
}

export interface DataChunk {
  streamId: string;
  sequence: bigint | number;
  payload: PayloadBytes;
  metadata?: MetadataEntry[];
  timestampMs?: bigint | number;
  /**
   * Graceful stream-end marker: a sink sends it on close; the receiving
   * stream yields everything before it, then ends. Never surfaced to
   * consumers as data. Can be lost on StreamLatencyFirst lanes, so back
   * completion with an application-level timeout there.
   */
  endOfStream?: boolean;
}

export type Dest = 'host' | 'workload' | { peer: ActrId };

/** One inbound DataChunk together with its sender. */
export interface DataChunkEvent {
  chunk: DataChunk;
  sender: ActrId;
}

// Per-invocation context threaded through every V2 export. Carries the
// identity triple that the 0.1.0 world exposed via getter imports, plus the
// `ctx-token` that routes outbound host imports to the host's per-invocation
// table. Under run_concurrent several dispatches may interleave on one
// instance; each carries its own `invocation-ctx`, so identity never
// cross-reads a shared slot.
export interface InvocationCtx {
  ctxToken: bigint;
  selfId: ActrId;
  callerId?: ActrId;
  requestId: string;
  expectsResponse: boolean;
  replyEnabled: boolean;
}

export type ErrorCategory =
  | { tag: 'handler-panic' }
  | { tag: 'handler-error' }
  | { tag: 'signaling-failure' }
  | { tag: 'transport-failure' }
  | { tag: 'data-chunk-delivery-uncertain' };

export type WebRtcPeerStatus =
  | { tag: 'idle' }
  | { tag: 'connecting' }
  | { tag: 'connected' }
  | { tag: 'recovering' };

export interface WebRtcPeerState {
  peer: ActrId;
  pcGeneration: bigint;
  revision: bigint;
  status: WebRtcPeerStatus;
  sendable: boolean;
  networkPath: { tag: 'unknown' | 'direct' | 'relayed' };
  relayed?: boolean;
  unavailableReason?: {
    tag:
      | 'connection-not-ready'
      | 'peer-connection-build-failed'
      | 'negotiation-failed'
      | 'ice-restart-failed'
      | 'peer-connection-rebuild-failed'
      | 'unsupported-rtc-capability'
      | 'rtc-resource-limit-exceeded'
      | 'recovery-exhausted'
      | 'closed';
  };
}

export interface ErrorEvent {
  source: ActrError;
  category: ErrorCategory;
  context: string;
  timestamp: { seconds: bigint; nanoseconds: number };
}

export interface PeerEvent {
  peer: ActrId;
  relayed?: boolean;
  status?: WebRtcPeerStatus;
}

export interface CredentialEvent {
  newExpiry: { seconds: bigint; nanoseconds: number };
}

export interface BackpressureEvent {
  queueLen: bigint | number;
  threshold: bigint | number;
}

export type ActrError =
  | { tag: 'unavailable'; val: string }
  | { tag: 'connection-not-ready'; val: { retryAfterMs?: bigint } }
  | { tag: 'timed-out' }
  | { tag: 'not-found'; val: string }
  | { tag: 'permission-denied'; val: string }
  | { tag: 'invalid-argument'; val: string }
  | { tag: 'unknown-route'; val: string }
  | {
      tag: 'dependency-not-found';
      val: { serviceName: string; message: string };
    }
  | { tag: 'decode-failure'; val: string }
  | { tag: 'not-implemented'; val: string }
  | { tag: 'internal'; val: string }
  | { tag: 'delivery-uncertain'; val: { operationId: string; cause: string } };

type WitActrId = Omit<ActrId, 'serialNumber'> & {
  serialNumber: bigint;
};

type WitDest =
  { tag: 'host' } | { tag: 'workload' } | { tag: 'peer'; val: WitActrId };

type WitPayloadType = { tag: PayloadType };

type WitDataChunk = Omit<
  DataChunk,
  'sequence' | 'payload' | 'metadata' | 'timestampMs'
> & {
  sequence: bigint;
  payload: Uint8Array;
  metadata: MetadataEntry[];
  timestampMs?: bigint;
};

export const PayloadType = {
  RpcReliable: 'rpc-reliable',
  RpcSignal: 'rpc-signal',
  StreamReliable: 'stream-reliable',
  StreamLatencyFirst: 'stream-latency-first',
  MediaRtp: 'media-rtp',
} as const;

export type PayloadType = (typeof PayloadType)[keyof typeof PayloadType];

/** Typed request metadata and codec implemented by generated RPC classes. */
export interface RpcRequest<Response> {
  readonly routeKey: string;
  readonly payloadType: PayloadType;
  encode(): Uint8Array;
  decode(payload: Uint8Array): Response;
}

// `ctx` is optional on every hook: the host always threads an `invocation-ctx`
// into the export, but callers may ignore it. Host-import wrappers below read
// the `ctx-token` from an explicitly-passed `ctx` (the concurrency-safe path,
// since StarlingMonkey has no `AsyncLocalStorage`), with an optional
// `AsyncLocalStorage` store as a Node-side convenience. A host import without
// either source fails closed instead of guessing a token.
export interface Workload {
  dispatch(
    envelope: RpcEnvelope,
    ctx?: InvocationCtx,
  ): Uint8Array | ArrayBuffer | Promise<Uint8Array | ArrayBuffer>;
  onStart?(ctx?: InvocationCtx): void | Promise<void>;
  onReady?(ctx?: InvocationCtx): void | Promise<void>;
  onStop?(ctx?: InvocationCtx): void | Promise<void>;
  onError?(event: ErrorEvent, ctx?: InvocationCtx): void | Promise<void>;
  onSignalingConnecting?(ctx?: InvocationCtx): void | Promise<void>;
  onSignalingConnected?(ctx?: InvocationCtx): void | Promise<void>;
  onSignalingDisconnected?(ctx?: InvocationCtx): void | Promise<void>;
  onWebsocketConnecting?(
    event: PeerEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onWebsocketConnected?(
    event: PeerEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onWebsocketDisconnected?(
    event: PeerEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onWebrtcConnecting?(
    event: PeerEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onWebrtcConnected?(
    event: PeerEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onWebrtcDisconnected?(
    event: PeerEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onWebrtcStateChanged?(
    event: WebRtcPeerState,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onCredentialRenewed?(
    event: CredentialEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onCredentialExpiring?(
    event: CredentialEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onMailboxBackpressure?(
    event: BackpressureEvent,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
  onDataChunk?(
    chunk: DataChunk,
    sender: ActrId,
    ctx?: InvocationCtx,
  ): void | Promise<void>;
}

export function defineWorkload(workload: Workload): Workload {
  return workload;
}

const streamQueues = new Map<string, BoundedQueue<DataChunkEvent>>();
const closingStreams = new Set<string>();

// Optional `AsyncLocalStorage` for the Node (vitest / non-component) path. The
// StarlingMonkey wasm guest has no `node:async_hooks`, so in a real component
// the dynamic import rejects and `invocationStorage` stays `undefined`; callers
// then thread `ctx` explicitly. Top-level `await` keeps this lazy and avoids a
// static `node:` import that would break componentization.
let invocationStorage:
  import('node:async_hooks').AsyncLocalStorage<InvocationCtx> | undefined;
try {
  const { AsyncLocalStorage } = await import('node:async_hooks');
  invocationStorage = new AsyncLocalStorage<InvocationCtx>();
} catch {
  // StarlingMonkey wasm guest: no node:async_hooks. ctx is threaded explicitly.
}

// Tokens of invocations currently inside an export call. Tracked explicitly
// because the wasm guest has no AsyncLocalStorage; a token is removed the
// moment its export settles, so a member of this set is a live token (host
// imports with a retired token trap the store).
const liveTokens = new Set<bigint>();

/**
 * Enter an invocation context for the duration of `fn`. Used by the
 * componentize shim at each export entry so host-import wrappers can read the
 * current `ctx-token` without an explicit argument on the Node side, and so
 * token liveness is tracked on both sides. In the wasm guest this is a
 * pass-through for the value (no `AsyncLocalStorage`); callers pass `ctx` to
 * the wrappers directly.
 */
export function withInvocationCtx<T>(
  ctx: InvocationCtx,
  fn: () => T | Promise<T>,
): T | Promise<T> {
  liveTokens.add(ctx.ctxToken);
  const exit = () => {
    liveTokens.delete(ctx.ctxToken);
  };
  const run = (): T | Promise<T> => {
    let result: T | Promise<T>;
    try {
      result = fn();
    } catch (error) {
      exit();
      throw error;
    }
    if (result instanceof Promise) {
      return result.finally(exit);
    }
    exit();
    return result;
  };
  return invocationStorage ? invocationStorage.run(ctx, run) : run();
}

/**
 * Read the currently-active invocation context, if any. Available on the Node
 * side (where `AsyncLocalStorage` is initialized); always `undefined` inside
 * the wasm guest.
 */
export function getCurrentInvocationCtx(): InvocationCtx | undefined {
  return invocationStorage?.getStore();
}

/**
 * Resolve a token that is live right now, for actor-global host imports
 * (stream register/unregister, chunk sends) that run outside their creating
 * invocation. Order: the explicit ctx when its invocation is still active,
 * then the AsyncLocalStorage store (Node), then any currently-live token
 * (wasm guest, where actor-global imports are not invocation-scoped).
 * `undefined` when no invocation is active anywhere.
 */
function resolveLiveToken(explicit?: InvocationCtx): bigint | undefined {
  if (explicit && liveTokens.has(explicit.ctxToken)) {
    return explicit.ctxToken;
  }
  const stored = invocationStorage?.getStore();
  if (stored) {
    return stored.ctxToken;
  }
  return liveTokens.values().next().value;
}

function resolveCtxToken(ctx?: InvocationCtx): bigint {
  if (ctx?.ctxToken !== undefined) {
    return ctx.ctxToken;
  }
  const stored = invocationStorage?.getStore();
  if (stored?.ctxToken !== undefined) {
    return stored.ctxToken;
  }
  throw new Error(
    'ACTR host import requires an InvocationCtx; pass the ctx received by the workload export',
  );
}

export function toUint8Array(value: PayloadBytes): Uint8Array {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value);
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  return Uint8Array.from(value);
}

function toWitActrId(id: ActrId): WitActrId {
  return {
    realm: id.realm,
    serialNumber: BigInt(id.serialNumber),
  };
}

function toWitDest(dest: Dest): WitDest {
  if (dest === 'host') {
    return { tag: 'host' };
  }
  if (dest === 'workload') {
    return { tag: 'workload' };
  }
  return {
    tag: 'peer',
    val: toWitActrId(dest.peer),
  };
}

function toWitDataChunk(chunk: DataChunk): WitDataChunk {
  return {
    streamId: chunk.streamId,
    sequence: BigInt(chunk.sequence),
    payload: toUint8Array(chunk.payload),
    metadata: chunk.metadata ?? [],
    timestampMs:
      chunk.timestampMs === undefined ? undefined : BigInt(chunk.timestampMs),
    endOfStream: chunk.endOfStream,
  };
}

function fromWitActrId(id: WitActrId): ActrId {
  return {
    realm: id.realm,
    serialNumber: id.serialNumber,
  };
}

// ── Host-import wrappers ──────────────────────────────────────────────────
//
// Each wrapper resolves the current `ctx-token` (explicit `ctx` arg first,
// then the `AsyncLocalStorage` store) and forwards to the V2 host import. It
// fails closed when neither source exists so a missing context can never route
// through another invocation's token. V2 imports surface WIT `result<T, E>`
// as a resolved value on success and a thrown JS `Error` on the `err` arm, so
// the wrappers simply await and let errors propagate.

export async function call(
  target: Dest,
  routeKey: string,
  payload: PayloadBytes,
  timeoutMs: bigint | number,
  ctx?: InvocationCtx,
  payloadType: PayloadType = PayloadType.RpcReliable,
): Promise<Uint8Array> {
  const timeout = validateTimeout(timeoutMs);
  return hostCall(
    resolveCtxToken(ctx),
    toWitDest(target),
    routeKey,
    toUint8Array(payload),
    timeout,
    { tag: payloadType } satisfies WitPayloadType,
  );
}

export async function tell(
  target: Dest,
  routeKey: string,
  payload: PayloadBytes,
  ctx?: InvocationCtx,
  payloadType: PayloadType = PayloadType.RpcReliable,
): Promise<void> {
  await hostTell(
    resolveCtxToken(ctx),
    toWitDest(target),
    routeKey,
    toUint8Array(payload),
    { tag: payloadType } satisfies WitPayloadType,
  );
}

export async function callRaw(
  target: ActrId,
  routeKey: string,
  payload: PayloadBytes,
  timeoutMs: bigint | number,
  ctx?: InvocationCtx,
  payloadType: PayloadType = PayloadType.RpcReliable,
): Promise<Uint8Array> {
  const timeout = validateTimeout(timeoutMs);
  return hostCallRaw(
    resolveCtxToken(ctx),
    toWitActrId(target),
    routeKey,
    toUint8Array(payload),
    timeout,
    { tag: payloadType } satisfies WitPayloadType,
  );
}

export async function replyRaw(
  payload: PayloadBytes,
  ctx?: InvocationCtx,
): Promise<void> {
  await hostReply(resolveCtxToken(ctx), toUint8Array(payload));
}

export async function replyError(
  error: ActrError,
  ctx?: InvocationCtx,
): Promise<void> {
  await hostReplyError(resolveCtxToken(ctx), error);
}

export async function discover(
  targetType: ActrType,
  ctx?: InvocationCtx,
): Promise<ActrId> {
  const id = await hostDiscover(resolveCtxToken(ctx), targetType);
  return fromWitActrId(id);
}

export async function resolveDependency(
  alias: string,
  ctx?: InvocationCtx,
): Promise<ActrType> {
  return hostResolveDependency(resolveCtxToken(ctx), alias);
}

const DEFAULT_DATA_CHUNK_CAPACITY = 64;

/**
 * Bounded FIFO queue feeding one stream subscription. Producers suspend when
 * the buffer is full — safe because the host awaits each per-stream dispatch,
 * so a full queue backpressures only that stream. `close()` drops buffered
 * items and resolves every waiter.
 */
class BoundedQueue<T> {
  private buffer: T[] = [];
  private putWaiters: Array<() => void> = [];
  private takeWaiters: Array<(result: { done: boolean; value?: T }) => void> =
    [];
  private closedFlag = false;

  constructor(private readonly capacity: number) {}

  async enqueue(item: T): Promise<void> {
    while (!this.closedFlag && this.buffer.length >= this.capacity) {
      await new Promise<void>((resolve) => this.putWaiters.push(resolve));
    }
    if (this.closedFlag) {
      return;
    }
    const waiter = this.takeWaiters.shift();
    if (waiter) {
      waiter({ done: false, value: item });
    } else {
      this.buffer.push(item);
    }
    this.wakePutWaiters();
  }

  async dequeue(): Promise<{ done: boolean; value?: T }> {
    if (this.buffer.length > 0) {
      const value = this.buffer.shift() as T;
      this.wakePutWaiters();
      return { done: false, value };
    }
    if (this.closedFlag) {
      return { done: true };
    }
    return new Promise((resolve) => this.takeWaiters.push(resolve));
  }

  close(): void {
    if (this.closedFlag) {
      return;
    }
    this.closedFlag = true;
    this.buffer = [];
    for (const waiter of this.takeWaiters.splice(0)) {
      waiter({ done: true });
    }
    for (const waiter of this.putWaiters.splice(0)) {
      waiter();
    }
  }

  private wakePutWaiters(): void {
    while (this.buffer.length < this.capacity && this.putWaiters.length > 0) {
      const waiter = this.putWaiters.shift();
      waiter?.();
    }
  }
}

/**
 * Single-consumer async iterable over one DataChunk subscription. `close()`
 * unregisters immediately, drops buffered items, and wakes pending readers;
 * the iterator's `return()` (e.g. `break` out of `for await`) closes too.
 *
 * Graceful end: when the peer's sink closes it sends an in-band
 * `endOfStream` marker; the iterator yields every chunk queued before it,
 * then returns `done` (the marker itself is never yielded). On
 * StreamLatencyFirst lanes the marker can be dropped like any other chunk,
 * so applications on lossy lanes should still back completion with a
 * timeout.
 *
 * Host-side teardown note: the subscription-time ctx token is retired once
 * the subscribing invocation returns, and calling a host import with a
 * retired token traps the store. `close()` therefore only calls
 * `hostUnregisterStream` with a token that is live right now (the explicit
 * ctx when its invocation is still active, the AsyncLocalStorage store on
 * Node, or any live token in the wasm guest); otherwise it tears down
 * locally and the host reclaims its registration on the next delivery (the
 * dispatch fails against the missing queue) or when the guest re-subscribes
 * (the host retires the stale forwarder first).
 */
export class DataChunkStream implements AsyncIterable<DataChunkEvent> {
  private iterator?: AsyncIterator<DataChunkEvent>;
  private closePromise?: Promise<void>;

  constructor(
    private readonly streamId: string,
    private readonly queue: BoundedQueue<DataChunkEvent>,
    private readonly ctx?: InvocationCtx,
  ) {}

  async close(): Promise<void> {
    if (!this.closePromise) {
      this.closePromise = (async () => {
        this.queue.close();
        streamQueues.delete(this.streamId);
        closingStreams.add(this.streamId);
        try {
          const token = resolveLiveToken(this.ctx);
          if (token !== undefined) {
            await hostUnregisterStream(token, this.streamId);
          }
        } finally {
          closingStreams.delete(this.streamId);
        }
      })();
    }
    return this.closePromise;
  }

  [Symbol.asyncIterator](): AsyncIterator<DataChunkEvent> {
    if (this.iterator) {
      throw new Error(
        `DataChunkStream for '${this.streamId}' is single-consumer`,
      );
    }
    let pending = false;
    const iterator: AsyncIterator<DataChunkEvent> = {
      next: async (): Promise<IteratorResult<DataChunkEvent>> => {
        if (pending) {
          throw new Error('concurrent next() on DataChunkStream iterator');
        }
        pending = true;
        try {
          const item = await this.queue.dequeue();
          if (item.done) {
            return { done: true, value: undefined };
          }
          const event = item.value as DataChunkEvent;
          if (event.chunk.endOfStream === true) {
            // Graceful end: FIFO delivery means every data chunk was already
            // yielded; unwrap the marker into termination (never surfaced).
            await this.close();
            return { done: true, value: undefined };
          }
          return { done: false, value: event };
        } finally {
          pending = false;
        }
      },
      return: async (): Promise<IteratorResult<DataChunkEvent>> => {
        await this.close();
        return { done: true, value: undefined };
      },
    };
    this.iterator = iterator;
    return iterator;
  }
}

/**
 * Outbound DataChunk sink bound to one destination, stream ID, and lane.
 * Sends are serialized in acceptance order; `close()` stops accepting new
 * sends, waits for accepted sends to finish, sends the in-band
 * `endOfStream` marker (sequence one past the last chunk), and is
 * idempotent. A `send()` after close — or after an application-sent end
 * marker — throws. A failed `close()` can be retried: it never reports an
 * unsent marker as success. Note the marker is at-least-once under retries:
 * if its send response was lost in transit, a retried close re-sends it —
 * the receiver's runtime drops the duplicate (the stream already ended).
 */
export class DataChunkSink {
  private closed = false;
  private tail: Promise<void> = Promise.resolve();
  private lastSequence?: bigint;
  private endMarkerSent = false;
  private closePromise?: Promise<void>;

  constructor(
    private readonly target: Dest,
    readonly streamId: string,
    readonly payloadType: PayloadType,
    private readonly ctx?: InvocationCtx,
  ) {}

  async send(chunk: DataChunk): Promise<void> {
    if (this.closed) {
      throw new Error('DataChunkSink is closed');
    }
    if (this.endMarkerSent) {
      throw new Error('endOfStream marker already sent for this stream');
    }
    if (chunk.streamId !== this.streamId) {
      // Never silently rewrite the wire item.
      throw new Error(
        `chunk streamId '${chunk.streamId}' does not match sink streamId '${this.streamId}'`,
      );
    }
    if (chunk.endOfStream === true) {
      // An end marker is a control signal: it must not carry a payload (the
      // receiving stream never surfaces it as data).
      if (toUint8Array(chunk.payload).length > 0) {
        throw new Error('endOfStream marker must not carry a payload');
      }
      // The application ended the stream explicitly; close() will not send
      // a duplicate marker.
      this.endMarkerSent = true;
    }
    this.lastSequence = BigInt(chunk.sequence);
    const send = this.tail.then(() => {
      // In the wasm guest (no AsyncLocalStorage) the host traps or rejects
      // imports on a retired token, so resolve a currently-live one: the
      // captured construction ctx may already be retired when the sink is
      // reused across invocations. Stream sends are actor-global, so any
      // live token routes them. On the Node host the token is inert
      // bookkeeping, so the explicit/ALS resolution is kept verbatim.
      const token =
        invocationStorage === undefined
          ? resolveLiveToken(this.ctx)
          : resolveCtxToken(this.ctx);
      if (token === undefined) {
        throw new Error(
          'ACTR host import requires a live invocation; the sink outlived its creating invocation',
        );
      }
      return hostSendDataChunk(
        token,
        toWitDest(this.target),
        toWitDataChunk(chunk),
        { tag: this.payloadType } satisfies WitPayloadType,
      ).then(() => undefined);
    });
    // Later sends queue behind this one even if it fails.
    this.tail = send.catch(() => undefined);
    return send;
  }

  async close(): Promise<void> {
    this.closed = true;
    if (!this.closePromise) {
      this.closePromise = (async () => {
        await this.tail;
        if (this.endMarkerSent) {
          return;
        }
        // The marker rides the same lane so it travels behind every data
        // chunk in FIFO order. It is sent only with a currently-live token
        // (host imports trap on a retired one); otherwise the peer observes
        // the end via its own timeout/abort path.
        const token = resolveLiveToken(this.ctx);
        if (token === undefined) {
          return;
        }
        const marker: DataChunk = {
          streamId: this.streamId,
          sequence:
            this.lastSequence === undefined ? 0n : this.lastSequence + 1n,
          payload: new Uint8Array(0),
          metadata: [],
          endOfStream: true,
        };
        await hostSendDataChunk(
          token,
          toWitDest(this.target),
          toWitDataChunk(marker),
          { tag: this.payloadType } satisfies WitPayloadType,
        ).then(() => undefined);
        this.endMarkerSent = true;
      })();
      // A failed close (marker send error) does not confirm: the next
      // close() retries from scratch instead of reporting success.
      this.closePromise.catch(() => {
        this.closePromise = undefined;
      });
    }
    return this.closePromise;
  }
}

/**
 * Subscribe to inbound DataChunks for a stream ID. Registration happens at
 * call time, reserving the ID until host registration finishes; only one active
 * subscription is allowed per stream ID within one actor/context instance.
 */
export async function dataChunks(
  streamId: string,
  options?: { capacity?: number },
  ctx?: InvocationCtx,
): Promise<DataChunkStream> {
  const capacity = options?.capacity ?? DEFAULT_DATA_CHUNK_CAPACITY;
  if (!Number.isInteger(capacity) || capacity <= 0) {
    throw new RangeError(
      `capacity must be a positive integer, got ${capacity}`,
    );
  }
  if (streamQueues.has(streamId) || closingStreams.has(streamId)) {
    throw new Error(`stream '${streamId}' already has an active subscription`);
  }
  // Reserve the ID before yielding so concurrent registrations cannot replace
  // each other's host forwarder or guest queue.
  const queue = new BoundedQueue<DataChunkEvent>(capacity);
  streamQueues.set(streamId, queue);
  try {
    await hostRegisterStream(resolveCtxToken(ctx), streamId);
  } catch (error) {
    streamQueues.delete(streamId);
    queue.close();
    throw error;
  }
  return new DataChunkStream(streamId, queue, ctx);
}

/** Create an outbound sink; only the two stream lanes are accepted. */
export async function dataChunkSink(
  target: Dest,
  streamId: string,
  payloadType: PayloadType,
  ctx?: InvocationCtx,
): Promise<DataChunkSink> {
  if (
    payloadType !== PayloadType.StreamReliable &&
    payloadType !== PayloadType.StreamLatencyFirst
  ) {
    throw new RangeError(
      `payloadType '${payloadType}' is not a DataChunk stream lane`,
    );
  }
  return new DataChunkSink(target, streamId, payloadType, ctx);
}

export async function logMessage(
  level: string,
  message: string,
  ctx?: InvocationCtx,
): Promise<void> {
  await hostLogMessage(resolveCtxToken(ctx), level, message);
}

export async function __dispatchDataChunk(
  chunk: DataChunk,
  sender: ActrId,
  _ctx: InvocationCtx,
): Promise<void> {
  const queue = streamQueues.get(chunk.streamId);
  if (!queue) {
    throw new Error(`No stream subscription for ${chunk.streamId}`);
  }
  await queue.enqueue({ chunk, sender });
}

function validateTimeout(timeoutMs: bigint | number): bigint {
  const timeout = BigInt(timeoutMs);
  if (timeout <= 0n) {
    throw new RangeError(`timeoutMs must be > 0, got ${timeoutMs}`);
  }
  return timeout;
}

/** Minimal per-invocation adapter used by generated unified dispatchers. */
export class ActrContext {
  constructor(private readonly ctx: InvocationCtx) {}

  get replyEnabled(): boolean {
    return this.ctx.replyEnabled;
  }

  get selfId(): ActrId {
    return this.ctx.selfId;
  }

  get callerId(): ActrId | undefined {
    return this.ctx.callerId;
  }

  get requestId(): string {
    return this.ctx.requestId;
  }

  get expectsResponse(): boolean {
    return this.ctx.expectsResponse;
  }

  async resolveDependency(alias: string): Promise<ActrType> {
    return resolveDependency(alias, this.ctx);
  }

  async discover(targetType: ActrType): Promise<ActrId> {
    return discover(targetType, this.ctx);
  }

  async call<Response>(
    target: Dest,
    request: RpcRequest<Response>,
    timeoutMs: bigint | number,
  ): Promise<Response> {
    const response = await call(
      target,
      request.routeKey,
      request.encode(),
      timeoutMs,
      this.ctx,
      request.payloadType,
    );
    return request.decode(response);
  }

  async callRaw(
    target: ActrId,
    routeKey: string,
    payload: Uint8Array,
    timeoutMs: bigint | number,
    payloadType: PayloadType = PayloadType.RpcReliable,
  ): Promise<Uint8Array> {
    return callRaw(target, routeKey, payload, timeoutMs, this.ctx, payloadType);
  }

  async replyRaw(payload: PayloadBytes): Promise<void> {
    return replyRaw(payload, this.ctx);
  }

  async replyError(error: ActrError): Promise<void> {
    return replyError(error, this.ctx);
  }

  async tell(target: Dest, message: RpcRequest<unknown>): Promise<void> {
    return tell(
      target,
      message.routeKey,
      message.encode(),
      this.ctx,
      message.payloadType,
    );
  }

  async dataChunks(
    streamId: string,
    options?: { capacity?: number },
  ): Promise<DataChunkStream> {
    return dataChunks(streamId, options, this.ctx);
  }

  async dataChunkSink(
    target: Dest,
    streamId: string,
    payloadType: PayloadType,
  ): Promise<DataChunkSink> {
    return dataChunkSink(target, streamId, payloadType, this.ctx);
  }
}

export class Reply<Response> {
  /**
   * The handle may be retained after its handler returns. If it is never sent,
   * the host expires the pending reply at the RFC-0323 in-flight TTL.
   */
  constructor(
    private readonly ctx: InvocationCtx | ActrContext,
    private readonly encode: (response: Response) => PayloadBytes,
  ) {}

  async send(response: Response): Promise<void> {
    // A RETURN-only v0.6 guest has no reply import/context. Generated
    // dispatchers may still construct a Reply while sharing code across
    // targets, so do not manufacture a host call with an invalid token.
    if (!this.ctx.replyEnabled) {
      return;
    }
    if (this.ctx instanceof ActrContext) {
      await this.ctx.replyRaw(this.encode(response));
      return;
    }
    await replyRaw(this.encode(response), this.ctx);
  }

  async sendError(error: ActrError): Promise<void> {
    if (!this.ctx.replyEnabled) {
      return;
    }
    if (this.ctx instanceof ActrContext) {
      await this.ctx.replyError(error);
      return;
    }
    await replyError(error, this.ctx);
  }
}
