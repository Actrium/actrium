// StarlingMonkey wasm guest simulation: `node:async_hooks` is unavailable,
// so AsyncLocalStorage never initializes and only the explicit live-token
// set tracks invocation liveness.

import { describe, expect, it, vi } from 'vitest';

vi.mock('node:async_hooks', () => {
  throw new Error('node:async_hooks is not available in the wasm guest');
});

import type { ActrId, DataChunk, InvocationCtx } from '../src/index.js';

async function loadWasmRuntime() {
  vi.resetModules();
  const host = await import('./host-imports.js');
  host.resetHostCalls();
  const runtime = await import('../src/index.js');
  return { host, runtime };
}

function testActorId(serialNumber: number | bigint = 7n): ActrId {
  return {
    realm: { realmId: 42 },
    serialNumber,
    type: { manufacturer: 'acme', name: 'EchoService', version: '1.0.0' },
  };
}

function testInvocationCtx(ctxToken: bigint): InvocationCtx {
  return {
    ctxToken,
    selfId: testActorId(),
    requestId: 'request-1',
    expectsResponse: true,
    replyEnabled: true,
  };
}

function testChunk(sequence: bigint): DataChunk {
  return {
    streamId: 'stream-1',
    sequence,
    payload: new Uint8Array([1]),
    metadata: [],
  };
}

describe('wasm guest mode (no AsyncLocalStorage)', () => {
  it('sink.send and close use the explicit ctx while its invocation is live', async () => {
    const { host, runtime } = await loadWasmRuntime();
    const ctx = testInvocationCtx(50n);

    await runtime.withInvocationCtx(ctx, async () => {
      const sink = await runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
        ctx,
      );
      await sink.send(testChunk(1n));
      await sink.close();
    });

    const chunks = host.hostCalls.sendDataChunk.map((call) => call.chunk);
    expect(chunks.map((chunk) => chunk.sequence)).toEqual([1n, 2n]);
    expect(chunks[1].endOfStream).toBe(true);
    expect(host.hostCalls.ctxTokens).toEqual([50n, 50n]);
  });

  it('sink.send after its creating invocation retired uses another live token', async () => {
    const { host, runtime } = await loadWasmRuntime();
    const creatingCtx = testInvocationCtx(50n);
    const sink = await runtime.withInvocationCtx(creatingCtx, async () =>
      runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
        creatingCtx,
      ),
    );

    // Token 50n settled with its invocation; the send happens during a
    // different live invocation and must not reuse the retired token.
    await runtime.withInvocationCtx(testInvocationCtx(77n), async () => {
      await sink.send(testChunk(1n));
    });

    expect(host.hostCalls.sendDataChunk).toHaveLength(1);
    expect(host.hostCalls.ctxTokens).toEqual([77n]);
  });

  it('sink with no ctx at all fails closed on send and close when nothing is live', async () => {
    const { runtime } = await loadWasmRuntime();
    const sink = await runtime.dataChunkSink(
      'host',
      'stream-1',
      runtime.PayloadType.StreamReliable,
    );

    await expect(sink.send(testChunk(1n))).rejects.toThrow('InvocationCtx');
    // close() fails closed the same way: silently skipping the marker would
    // leave the peer hanging on a graceful end that never comes.
    await expect(sink.close()).rejects.toThrow('InvocationCtx');
  });

  it('a detached consumer resumed by a later delivery sends through the delivery token', async () => {
    const { host, runtime } = await loadWasmRuntime();
    // Registration invocation: subscribe and create the reply sink.
    const registerCtx = testInvocationCtx(10n);
    const stream = await runtime.withInvocationCtx(registerCtx, async () =>
      runtime.dataChunks('in-stream', undefined, registerCtx),
    );
    const sink = await runtime.withInvocationCtx(registerCtx, async () =>
      runtime.dataChunkSink(
        'host',
        'reply-stream',
        runtime.PayloadType.StreamReliable,
        registerCtx,
      ),
    );

    // Detached consumer: resumes only when a delivery lands (the production
    // echo workload's `void` for-await pattern).
    const consumed = (async () => {
      for await (const event of stream) {
        await sink.send({
          streamId: 'reply-stream',
          sequence: 1n,
          payload: event.chunk.payload,
          metadata: [],
        });
        break;
      }
    })();

    // The delivery arrives in a later invocation; the shim wraps the export
    // in withInvocationCtx, so token 20n is live while the consumer resumes.
    await runtime.withInvocationCtx(testInvocationCtx(20n), async () => {
      await runtime.__dispatchDataChunk(
        {
          streamId: 'in-stream',
          sequence: 1n,
          payload: new Uint8Array([9]),
        },
        testActorId(),
        testInvocationCtx(20n),
      );
    });
    await consumed;

    const sends = host.hostCalls.sendDataChunk;
    expect(sends.map((call) => call.chunk.sequence)).toEqual([1n]);
    // The consumer resumes after the delivery invocation settled, so the
    // send presents the sink's captured token; the host routes it via the
    // instance's standing ABI (actor-global import). The consumer's `break`
    // closes the stream: with nothing live, close() presents the captured
    // (retired) token and the host routes the unregister the same way —
    // skipping it would leak the host forwarder.
    expect(host.hostCalls.operations).toEqual([
      'registerStream',
      'sendDataChunk',
      'unregisterStream',
    ]);
    expect(host.hostCalls.ctxTokens).toEqual([10n, 10n, 10n]);
  });
});
