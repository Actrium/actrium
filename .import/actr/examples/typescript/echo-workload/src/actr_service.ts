import {
  ActrContext,
  PayloadType,
  defineWorkload,
  toUint8Array,
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

    await ctx.registerStream(
      inboundStreamId,
      async (chunk, sender, streamCtx) => {
        const incoming = textDecoder.decode(toUint8Array(chunk.payload));

        await streamCtx.sendDataChunk(
          { peer: sender },
          {
            streamId: replyStreamId,
            sequence: BigInt(chunk.sequence) + 1n,
            payload: textEncoder.encode(`${replyMessage}: ${incoming}`),
            metadata: [{ key: "echo-runtime", value: "typescript-wasm" }],
            timestampMs: BigInt(Date.now()),
          },
          PayloadType.StreamReliable,
        );

        await streamCtx.unregisterStream(inboundStreamId);
      },
    );

    return create(StreamPrepareResponseSchema, {
      status: `registered:${inboundStreamId}`,
    });
  }

  async echoServiceReleaseStream(
    request: StreamReleaseRequest,
    ctx: ActrContext,
  ): Promise<StreamReleaseResponse> {
    await ctx.unregisterStream(request.streamId);
    return create(StreamReleaseResponseSchema, {
      status: `unregistered:${request.streamId}`,
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
