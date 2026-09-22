import {
  ActrRef as NativeActrRef,
  ActrId,
  ActrType,
  PayloadType,
} from '../index';
import { callNative } from './error';

/**
 * ActrRef – reference to a running actor.
 *
 * Provides methods to interact with the actor: RPC calls, messaging, discovery, etc.
 */
export class ActrRef {
  constructor(private native: NativeActrRef) {}

  /**
   * Get the actor ID.
   *
   * @returns The actor's unique identifier
   */
  actorId(): ActrId {
    return this.native.actorId();
  }

  /**
   * Discover actors of the given type.
   *
   * @param targetType - Target actor type (publisher + name)
   * @param count - Number of actors to discover
   * @returns List of actor IDs
   *
   * @example
   * ```typescript
   * const servers = await actorRef.discover(
   *   { publisher: 'acme', name: 'EchoTwiceService' },
   *   1
   * );
   * ```
   */
  async discover(targetType: ActrType, count: number): Promise<ActrId[]> {
    return await callNative(() => this.native.discover(targetType, count));
  }

  /**
   * Resolve a dependency alias declared by this workload's manifest.
   *
   * Packaged workloads resolve from their authenticated lock; linked
   * workloads resolve from the dynamic manifest loaded by fromConfig().
   */
  async resolveDependency(alias: string): Promise<ActrType> {
    return await callNative(() => this.native.resolveDependency(alias));
  }

  /**
   * Call remote actor (RPC).
   *
   * @param target - Target actor ID
   * @param routeKey - Route key (e.g. 'service.Method')
   * @param payloadType - Payload type
   * @param requestPayload - Request payload (protobuf-encoded)
   * @param timeoutMs - Timeout in ms, default 30000
   * @returns Response payload
   *
   * @example
   * ```typescript
   * const request = Buffer.from('Hello');
   * const response = await actorRef.call(
   *   serverId,
   *   'echo_twice.EchoTwiceService.EchoTwice',
   *   PayloadType.RpcReliable,
   *   request,
   *   5000
   * );
   * ```
   */
  async call(
    target: ActrId,
    routeKey: string,
    payloadType: PayloadType,
    requestPayload: Buffer,
    timeoutMs: number = 30000,
  ): Promise<Buffer> {
    return await callNative(() =>
      this.native.call(
        target,
        routeKey,
        payloadType,
        requestPayload,
        timeoutMs,
      ),
    );
  }

  /**
   * Type-safe RPC call using Protobuf messages.
   *
   * @param routeKey - Route key
   * @param request - Request object (must have encode() method)
   * @param responseType - Response type (must have decode() method)
   * @param payloadType - Payload type, default RpcReliable
   * @param timeoutMs - Timeout in ms, default 30000
   * @returns Decoded response object
   *
   * @example
   * ```typescript
   * import { EchoTwiceRequest, EchoTwiceResponse } from './proto/echo-twice';
   *
   * const request = new EchoTwiceRequest({ message: 'Hello' });
   * const response = await actorRef.callTyped(
   *   serverId,
   *   'echo_twice.EchoTwiceService.EchoTwice',
   *   request,
   *   EchoTwiceResponse,
   *   PayloadType.RpcReliable,
   *   5000
   * );
   * console.log(response.message);
   * ```
   */
  async callTyped<Req, Res>(
    target: ActrId,
    routeKey: string,
    request: Req & EncodableRequest,
    responseType: { decode: (buf: Buffer) => Res },
    payloadType: PayloadType = PayloadType.RpcReliable,
    timeoutMs: number = 30000,
  ): Promise<Res> {
    const requestBuf = request.encode();
    const responseBuf = await this.call(
      target,
      routeKey,
      payloadType,
      requestBuf,
      timeoutMs,
    );
    return responseType.decode(responseBuf);
  }

  /**
   * Type-safe RPC call consuming a generated request companion.
   *
   * The companion carries the route key and the `(actr.payload_type)` lane
   * declared in proto (falling back to RpcReliable), so callers no longer
   * restate them — and cannot accidentally pin the wrong lane.
   *
   * @param target - Target actor ID
   * @param companion - Generated request companion (`{ routeKey, payloadType, encode, response }`)
   * @param request - Request init object accepted by the companion's encode()
   * @param timeoutMs - Timeout in milliseconds
   * @returns Decoded response object
   *
   * @example
   * ```typescript
   * import { EchoRequest } from './generated/echo_client';
   *
   * const response = await actorRef.callRpc(
   *   serverId,
   *   EchoRequest,
   *   { message: 'Hello' },
   *   5000
   * );
   * console.log(response.reply);
   * ```
   */
  async callRpc<Req, Res>(
    target: ActrId,
    companion: RpcCompanion<Req, Res>,
    request: Req,
    timeoutMs: number,
  ): Promise<Res> {
    const requestBuf = companion.encode(request);
    const responseBuf = await this.call(
      target,
      companion.routeKey,
      // Generated companions carry a runtime-neutral numeric lane; the Node
      // runtime's PayloadType is the numeric const enum with the same values.
      companion.payloadType as PayloadType,
      requestBuf,
      timeoutMs,
    );
    return companion.response.decode(responseBuf);
  }

  /**
   * Send one-way message (fire-and-forget).
   *
   * @param target - Target actor ID
   * @param routeKey - Route key
   * @param payloadType - Payload type
   * @param messagePayload - Message payload
   *
   * @example
   * ```typescript
   * await actorRef.tell(
   *   serverId,
   *   'notification.Service/Notify',
   *   PayloadType.RpcSignal,
   *   Buffer.from('Event occurred')
   * );
   * ```
   */
  async tell(
    target: ActrId,
    routeKey: string,
    payloadType: PayloadType,
    messagePayload: Buffer,
  ): Promise<void> {
    await callNative(() =>
      this.native.tell(target, routeKey, payloadType, messagePayload),
    );
  }

  /**
   * Trigger shutdown.
   *
   * Starts the shutdown process but does not wait for completion.
   */
  shutdown(): void {
    this.native.shutdown();
  }

  /**
   * Wait for shutdown to complete.
   *
   * Blocks until the actor has fully shut down.
   */
  async waitForShutdown(): Promise<void> {
    await this.native.waitForShutdown();
  }

  /**
   * Check if shutdown is in progress.
   *
   * @returns true if shutting down
   */
  isShuttingDown(): boolean {
    return this.native.isShuttingDown();
  }

  /**
   * Stop the actor (shutdown and wait).
   *
   * Convenience for shutdown() + waitForShutdown().
   *
   * @example
   * ```typescript
   * await actorRef.stop();
   * console.log('Actor stopped');
   * ```
   */
  async stop(): Promise<void> {
    this.shutdown();
    await this.waitForShutdown();
  }
}

type EncodableRequest = {
  encode(): Buffer;
};

/**
 * Structural shape of a request companion emitted by
 * protoc-gen-actrframework-typescript.
 *
 * `payloadType` is the runtime-neutral numeric value of the proto
 * `(actr.payload_type)` option (0 = RPC_RELIABLE fallback). It deliberately
 * does not import a runtime package. The Node binding adapts the numeric value
 * to its `PayloadType` at the `ActrRef.callRpc` boundary. Workload/WIT lane
 * propagation is handled separately.
 */
export interface RpcCompanion<Req, Res> {
  readonly routeKey: string;
  readonly payloadType: number;
  encode(message: Req): Buffer;
  response: {
    decode(bytes: Uint8Array): Res;
  };
}
