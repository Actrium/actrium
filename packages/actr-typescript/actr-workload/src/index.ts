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
}

export type Dest = 'host' | 'workload' | { peer: ActrId };

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

export type StreamCallback = (
  chunk: DataChunk,
  sender: ActrId,
  ctx: InvocationCtx,
) => void | Promise<void>;

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

const streamCallbacks = new Map<string, StreamCallback>();

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

/**
 * Enter an invocation context for the duration of `fn`. Used by the
 * componentize shim at each export entry so host-import wrappers can read the
 * current `ctx-token` without an explicit argument on the Node side. In the
 * wasm guest this is a pass-through (no `AsyncLocalStorage`); callers pass
 * `ctx` to the wrappers directly.
 */
export function withInvocationCtx<T>(
  ctx: InvocationCtx,
  fn: () => T | Promise<T>,
): T | Promise<T> {
  if (invocationStorage) {
    return invocationStorage.run(ctx, fn);
  }
  return fn();
}

/**
 * Read the currently-active invocation context, if any. Available on the Node
 * side (where `AsyncLocalStorage` is initialized); always `undefined` inside
 * the wasm guest.
 */
export function getCurrentInvocationCtx(): InvocationCtx | undefined {
  return invocationStorage?.getStore();
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

export async function registerStream(
  streamId: string,
  callback: StreamCallback,
  ctx?: InvocationCtx,
): Promise<void> {
  // Register host-side first so the runner queues any DataChunk delivery behind
  // this invocation; installing the guest callback immediately after the
  // awaited import cannot miss a delivery, and avoids retaining a guest
  // callback when host registration fails.
  await hostRegisterStream(resolveCtxToken(ctx), streamId);
  streamCallbacks.set(streamId, callback);
}

export async function unregisterStream(
  streamId: string,
  ctx?: InvocationCtx,
): Promise<void> {
  await hostUnregisterStream(resolveCtxToken(ctx), streamId);
  streamCallbacks.delete(streamId);
}

export async function sendDataChunk(
  target: Dest,
  chunk: DataChunk,
  payloadType: PayloadType,
  ctx?: InvocationCtx,
): Promise<void> {
  await hostSendDataChunk(
    resolveCtxToken(ctx),
    toWitDest(target),
    toWitDataChunk(chunk),
    { tag: payloadType } satisfies WitPayloadType,
  );
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
  ctx: InvocationCtx,
): Promise<void> {
  const callback = streamCallbacks.get(chunk.streamId);
  if (!callback) {
    throw new Error(`No stream callback registered for ${chunk.streamId}`);
  }
  await callback(chunk, sender, ctx);
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

  async registerStream(
    streamId: string,
    callback: (
      chunk: DataChunk,
      sender: ActrId,
      ctx: ActrContext,
    ) => Promise<void>,
  ): Promise<void> {
    return registerStream(
      streamId,
      async (chunk, sender, ctx) =>
        callback(chunk, sender, new ActrContext(ctx)),
      this.ctx,
    );
  }

  async unregisterStream(streamId: string): Promise<void> {
    return unregisterStream(streamId, this.ctx);
  }

  async sendDataChunk(
    target: Dest,
    chunk: DataChunk,
    payloadType: PayloadType,
  ): Promise<void> {
    return sendDataChunk(target, chunk, payloadType, this.ctx);
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
