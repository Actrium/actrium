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

  it('sink.send with no live invocation fails closed instead of using the retired token', async () => {
    const { runtime } = await loadWasmRuntime();
    const sink = await runtime.withInvocationCtx(
      testInvocationCtx(50n),
      async () =>
        runtime.dataChunkSink(
          'host',
          'stream-1',
          runtime.PayloadType.StreamReliable,
          testInvocationCtx(50n),
        ),
    );

    await expect(sink.send(testChunk(1n))).rejects.toThrow('live invocation');
  });
});
