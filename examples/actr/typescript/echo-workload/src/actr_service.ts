import {
  ActrContext,
  PayloadType,
  defineWorkload,
  toUint8Array,
  type DataChunkStream,
} from "@actrium/actr-workload";
import { create } from "@bufbuild/protobuf";

import type {
  EchoRequest,
  EchoResponse,
  StreamPrepareRequest,
  StreamPrepareResponse,
  StreamReleaseRequest,
  StreamReleaseResponse,
} from "./generated/echo_pb.js";
import {
  EchoResponseSchema,
  StreamPrepareResponseSchema,
  StreamReleaseResponseSchema,
} from "./generated/echo_pb.js";
import {
  actrDispatch,
  type ActrDispatchHandler,
} from "./generated/actr_dispatch.js";

const textDecoder = new TextDecoder();
const textEncoder = new TextEncoder();

const activeStreams = new Map<string, DataChunkStream>();

class EchoServiceHandlerImpl implements ActrDispatchHandler {
  echoServiceEcho(request: EchoRequest): EchoResponse {
    console.log(`Received Echo request: ${request.message}`);

    return create(EchoResponseSchema, {
      reply: request.message,
      timestamp: BigInt(Math.floor(Date.now() / 1000)),
    });
  }

  async echoServicePrepareStream(
    request: StreamPrepareRequest,
    ctx: ActrContext,
  ): Promise<StreamPrepareResponse> {
    const inboundStreamId = request.inboundStreamId;
    const replyStreamId = request.replyStreamId;
    const replyMessage = request.replyMessage;

    const stream = await ctx.dataChunks(inboundStreamId);
    activeStreams.set(inboundStreamId, stream);

    void (async () => {
      try {
        for await (const event of stream) {
          const incoming = textDecoder.decode(toUint8Array(event.chunk.payload));

          const sink = await ctx.dataChunkSink(
            { peer: event.sender },
            replyStreamId,
            PayloadType.StreamReliable,
          );
          await sink.send({
            streamId: replyStreamId,
            sequence: BigInt(event.chunk.sequence) + 1n,
            payload: textEncoder.encode(`${replyMessage}: ${incoming}`),
            metadata: [{ key: "echo-runtime", value: "typescript-wasm" }],
            timestampMs: BigInt(Date.now()),
          });
          await sink.close();

          // One reply per subscription, then close (iterator return() also
          // closes; the finally below is the belt-and-braces path).
          break;
        }
      } finally {
        activeStreams.delete(inboundStreamId);
        await stream.close();
      }
    })();

    return create(StreamPrepareResponseSchema, {
      status: `subscribed:${inboundStreamId}`,
    });
  }

  async echoServiceReleaseStream(
    request: StreamReleaseRequest,
    ctx: ActrContext,
  ): Promise<StreamReleaseResponse> {
    const stream = activeStreams.get(request.streamId);
    activeStreams.delete(request.streamId);
    await stream?.close();
    return create(StreamReleaseResponseSchema, {
      status: `closed:${request.streamId}`,
    });
  }
}

export default defineWorkload({
  async onStart(): Promise<void> {
    console.log("Generated TypeScript EchoService workload started");
  },

  async onStop(): Promise<void> {
    console.log("Generated TypeScript EchoService workload stopped");
  },

  async dispatch(envelope, ctx): Promise<Uint8Array> {
    return actrDispatch(
      new ActrContext(ctx!),
      new EchoServiceHandlerImpl(),
      envelope.routeKey ?? envelope.method,
      envelope.payload ?? new Uint8Array(),
      30_000,
    );
  },
});
