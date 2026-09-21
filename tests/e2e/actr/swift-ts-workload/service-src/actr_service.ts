import { create } from '@bufbuild/protobuf';
import {
  ActrContext,
  PayloadType,
  defineWorkload,
  type ActrId,
  type DataChunk,
  type DataChunkSink,
  type DataChunkStream,
  type MetadataEntry,
} from '@actrium/actr-workload';

import type {
  EchoRequest,
  EchoResponse,
  FinishDuplexStreamRequest,
  FinishDuplexStreamResponse,
  StartDuplexStreamRequest,
  StartDuplexStreamResponse,
} from './generated/duplex_echo_pb.js';
import {
  EchoResponseSchema,
  FinishDuplexStreamResponseSchema,
  StartDuplexStreamResponseSchema,
  StreamPayloadMode,
} from './generated/duplex_echo_pb.js';
import {
  actrDispatch,
  type ActrDispatchHandler,
} from './generated/actr_dispatch.js';

const textDecoder = new TextDecoder();
const textEncoder = new TextEncoder();

type SessionState = {
  sessionId: string;
  clientToServiceStreamId: string;
  serviceToClientStreamId: string;
  payloadType: PayloadType;
  clientChunksReceived: number;
  serviceChunksSent: number;
  stream?: DataChunkStream;
  sink?: DataChunkSink;
};

const sessionsById = new Map<string, SessionState>();
const sessionIdByClientStreamId = new Map<string, string>();

function payloadTypeFor(mode: StreamPayloadMode): PayloadType {
  if (mode === StreamPayloadMode.STREAM_LATENCY_FIRST) {
    return PayloadType.StreamLatencyFirst;
  }
  return PayloadType.StreamReliable;
}

function payloadText(payload: Uint8Array | ArrayBuffer | ArrayLike<number>): string {
  const bytes =
    payload instanceof Uint8Array
      ? payload
      : payload instanceof ArrayBuffer
        ? new Uint8Array(payload)
        : Uint8Array.from(payload);
  return textDecoder.decode(bytes);
}

class DuplexEchoServiceHandlerImpl implements ActrDispatchHandler {
  async duplexEchoServiceEcho(
    request: EchoRequest,
    _ctx: ActrContext,
  ): Promise<EchoResponse> {
    console.log(`[DuplexEchoService] recv Echo message="${request.message}"`);
    return create(EchoResponseSchema, { message: `echo:${request.message}` });
  }

  async duplexEchoServiceStartDuplexStream(
    request: StartDuplexStreamRequest,
    ctx: ActrContext,
  ): Promise<StartDuplexStreamResponse> {
    const payloadType = payloadTypeFor(request.payloadMode);
    const serviceToClientStreamId = `s2c-${request.sessionId}`;

    const state: SessionState = {
      sessionId: request.sessionId,
      clientToServiceStreamId: request.clientToServiceStreamId,
      serviceToClientStreamId,
      payloadType,
      clientChunksReceived: 0,
      serviceChunksSent: 0,
    };

    state.stream = await ctx.dataChunks(request.clientToServiceStreamId);

    sessionsById.set(request.sessionId, state);
    sessionIdByClientStreamId.set(request.clientToServiceStreamId, request.sessionId);

    void this.consumeClientChunks(state.stream, ctx);

    return create(StartDuplexStreamResponseSchema, {
      sessionId: request.sessionId,
      acceptedClientToServiceStreamId: request.clientToServiceStreamId,
      serviceToClientStreamId,
      status: `subscribed:${request.clientToServiceStreamId}`,
    });
  }

  private async consumeClientChunks(
    stream: DataChunkStream,
    ctx: ActrContext,
  ): Promise<void> {
    try {
      for await (const event of stream) {
        await this.onClientChunk(event.chunk, event.sender, ctx);
      }
    } finally {
      await stream.close();
    }
  }

  async duplexEchoServiceFinishDuplexStream(
    request: FinishDuplexStreamRequest,
    ctx: ActrContext,
  ): Promise<FinishDuplexStreamResponse> {
    const state = sessionsById.get(request.sessionId);
    await state?.stream?.close();
    await state?.sink?.close();
    sessionIdByClientStreamId.delete(request.clientToServiceStreamId);
    if (state) {
      sessionsById.delete(request.sessionId);
    }
    return create(FinishDuplexStreamResponseSchema, {
      sessionId: request.sessionId,
      clientChunksReceived: state?.clientChunksReceived ?? 0,
      serviceChunksSent: state?.serviceChunksSent ?? 0,
      status: `closed:${request.clientToServiceStreamId}`,
    });
  }

  private async onClientChunk(
    chunk: DataChunk,
    sender: ActrId,
    ctx: ActrContext,
  ): Promise<void> {
    const sessionId = sessionIdByClientStreamId.get(chunk.streamId);
    const state = sessionId ? sessionsById.get(sessionId) : undefined;
    if (!state) {
      console.log(`[DuplexEchoService] drop chunk, no session stream=${chunk.streamId}`);
      return;
    }

    const text = payloadText(chunk.payload);
    state.clientChunksReceived += 1;
    state.serviceChunksSent += 1;

    const ackSequence = BigInt(chunk.sequence) + 1000n;
    const ackPayload = `echo:${text}`;
    const ackMetadata: MetadataEntry[] = [
      { key: 'session_id', value: state.sessionId },
      { key: 'direction', value: 'service-to-client' },
      { key: 'ack_for_sequence', value: String(chunk.sequence) },
      { key: 'source_stream_id', value: chunk.streamId },
    ];

    if (!state.sink) {
      state.sink = await ctx.dataChunkSink(
        { peer: sender },
        state.serviceToClientStreamId,
        state.payloadType,
      );
    }
    await state.sink.send({
      streamId: state.serviceToClientStreamId,
      sequence: ackSequence,
      payload: textEncoder.encode(ackPayload),
      metadata: ackMetadata,
      timestampMs: BigInt(Date.now()),
    });
  }
}

const handler = new DuplexEchoServiceHandlerImpl();

export default defineWorkload({
  async onStart(): Promise<void> {
    console.log('[DuplexEchoService] workload started');
  },
  async onStop(): Promise<void> {
    console.log('[DuplexEchoService] workload stopped');
  },
  async dispatch(envelope, ctx): Promise<Uint8Array> {
    return actrDispatch(
      new ActrContext(ctx!),
      handler,
      envelope.routeKey,
      envelope.payload ?? new Uint8Array(),
      envelope.timeoutMs,
    );
  },
});
