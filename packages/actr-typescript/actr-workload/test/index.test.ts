import { beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  ActrId,
  ActrType,
  DataChunk,
  InvocationCtx,
  StreamCallback,
  Workload,
} from '../src/index.js';

async function loadRuntime() {
  vi.resetModules();
  const host = await import('./host-imports.js');
  host.resetHostCalls();
  const runtime = await import('../src/index.js');
  return { host, runtime };
}

function testActorId(serialNumber: number | bigint = 7n): ActrId {
  return {
    realm: 'test@example.com',
    serialNumber,
  };
}

function testChunk(overrides: Partial<DataChunk> = {}): DataChunk {
  return {
    streamId: 'stream-1',
    sequence: 3n,
    payload: new Uint8Array([1, 2, 3]),
    metadata: [{ key: 'lane', value: 'reliable' }],
    timestampMs: 1234n,
    ...overrides,
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

describe('@actrium/actr-workload', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('returns the original workload from defineWorkload', async () => {
    const { runtime } = await loadRuntime();
    const workload: Workload = {
      dispatch: () => new Uint8Array([1]),
    };

    expect(runtime.defineWorkload(workload)).toBe(workload);
  });

  it('registers a stream and dispatches chunks to the callback', async () => {
    const { host, runtime } = await loadRuntime();
    const chunk = testChunk();
    const sender = testActorId();
    const ctx = testInvocationCtx(11n);
    const received: Array<{
      chunk: DataChunk;
      sender: ActrId;
      ctx: InvocationCtx;
    }> = [];
    const callback: StreamCallback = async (incoming, from, callbackCtx) => {
      await Promise.resolve();
      received.push({ chunk: incoming, sender: from, ctx: callbackCtx });
    };

    await runtime.registerStream('stream-1', callback, ctx);
    await runtime.__dispatchDataChunk(chunk, sender, ctx);

    expect(host.hostCalls.registerStream).toEqual(['stream-1']);
    expect(received).toEqual([{ chunk, sender, ctx }]);
  });

  it('forwards an explicit invocation context token to host imports', async () => {
    const { host, runtime } = await loadRuntime();

    await runtime.registerStream(
      'stream-with-context',
      () => undefined,
      testInvocationCtx(37n),
    );

    expect(host.hostCalls.ctxTokens).toEqual([37n]);
  });

  it('resolves every host import token from AsyncLocalStorage', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(91n);
    const payload = new Uint8Array([4, 2]);
    const targetType: ActrType = {
      publisher: 'acme',
      name: 'EchoService',
      version: '1.0.0',
    };

    await runtime.withInvocationCtx(ctx, async () => {
      expect(runtime.getCurrentInvocationCtx()).toBe(ctx);
      await runtime.call('host', 'echo', payload, 30_000);
      await runtime.tell('workload', 'notify', payload);
      await runtime.callRaw(testActorId(), 'raw', payload, 30_000);
      await runtime.discover(targetType);
      await runtime.resolveDependency('echo');
      await runtime.registerStream('als-stream', () => undefined);
      await runtime.unregisterStream('als-stream');
      await runtime.sendDataChunk(
        'host',
        testChunk(),
        runtime.PayloadType.StreamReliable,
      );
      await runtime.logMessage('info', 'from active invocation');
    });

    expect(host.hostCalls.ctxTokens).toEqual(Array(9).fill(91n));
    expect(host.hostCalls.operations).toEqual([
      'call',
      'tell',
      'callRaw',
      'discover',
      'resolveDependency',
      'registerStream',
      'unregisterStream',
      'sendDataChunk',
      'logMessage',
    ]);
    expect(host.hostCalls.timeouts).toEqual([30_000n, 30_000n]);
    // call/tell/callRaw default to the reliable lane when no payloadType is
    // given; the sendDataChunk lane is asserted separately below.
    expect(host.hostCalls.payloadTypes).toEqual([
      { tag: 'rpc-reliable' },
      { tag: 'rpc-reliable' },
      { tag: 'rpc-reliable' },
    ]);
  });

  it('forwards an explicit payloadType to the host import', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(92n);
    const payload = new Uint8Array([7]);

    await runtime.withInvocationCtx(ctx, async () => {
      await runtime.call(
        'host',
        'echo',
        payload,
        30_000,
        undefined,
        runtime.PayloadType.RpcSignal,
      );
      await runtime.tell(
        'workload',
        'notify',
        payload,
        undefined,
        runtime.PayloadType.RpcSignal,
      );
      await runtime.callRaw(
        testActorId(),
        'raw',
        payload,
        30_000,
        undefined,
        runtime.PayloadType.RpcSignal,
      );
    });

    expect(host.hostCalls.payloadTypes).toEqual([
      { tag: 'rpc-signal' },
      { tag: 'rpc-signal' },
      { tag: 'rpc-signal' },
    ]);
  });

  it('binds typed context calls to their timeout and declared lane', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = new runtime.ActrContext(testInvocationCtx(93n));
    const request = {
      routeKey: 'echo',
      payloadType: runtime.PayloadType.RpcSignal,
      encode: () => new Uint8Array([9]),
      decode: (payload: Uint8Array) => payload[0],
    };

    await expect(ctx.call('host', request, 1_234)).resolves.toBe(9);
    await expect(
      ctx.callRaw(testActorId(), 'raw', new Uint8Array(), 0),
    ).rejects.toThrow('timeoutMs must be > 0');

    await ctx.callRaw(
      testActorId(),
      'raw-signal',
      new Uint8Array(),
      1_234,
      runtime.PayloadType.RpcSignal,
    );

    expect(host.hostCalls.timeouts).toEqual([1_234n, 1_234n]);
    expect(host.hostCalls.payloadTypes).toEqual([
      { tag: 'rpc-signal' },
      { tag: 'rpc-signal' },
    ]);
  });

  it('delivers raw replies through the explicit host boundary', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = new runtime.ActrContext(testInvocationCtx(94n));
    const payload = new Uint8Array([8, 9]);

    await ctx.replyRaw(payload);

    expect(host.hostCalls.ctxTokens).toEqual([94n]);
    expect(host.hostCalls.operations).toEqual(['reply']);
    expect(host.hostCalls.replyPayloads).toEqual([payload]);
  });

  it('delivers deferred errors through the explicit host boundary', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = new runtime.ActrContext(testInvocationCtx(95n));
    const error = { tag: 'internal', val: 'deferred failure' } as const;

    await ctx.replyError(error);
    await new runtime.Reply(ctx, () => new Uint8Array()).sendError(error);

    expect(host.hostCalls.ctxTokens).toEqual([95n, 95n]);
    expect(host.hostCalls.operations).toEqual(['replyError', 'replyError']);
    expect(host.hostCalls.replyErrors).toEqual([error, error]);
  });

  it('rejects host imports when no invocation context is active', async () => {
    const { host, runtime } = await loadRuntime();

    await expect(
      runtime.registerStream('missing-context', () => undefined),
    ).rejects.toThrow('requires an InvocationCtx');
    expect(host.hostCalls.ctxTokens).toEqual([]);
    expect(host.hostCalls.registerStream).toEqual([]);
  });

  it('unregisters streams and rejects later dispatches', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(12n);

    await runtime.registerStream('stream-1', () => undefined, ctx);
    await runtime.unregisterStream('stream-1', ctx);

    expect(host.hostCalls.unregisterStream).toEqual(['stream-1']);
    await expect(
      runtime.__dispatchDataChunk(testChunk(), testActorId(), ctx),
    ).rejects.toThrow('No stream callback registered for stream-1');
  });

  it('sends data chunks using WIT-shaped peer destinations', async () => {
    const { host, runtime } = await loadRuntime();
    const peer = testActorId(9);
    const ctx = testInvocationCtx(13n);

    await runtime.sendDataChunk(
      { peer },
      testChunk({
        sequence: 4,
        timestampMs: 5678,
      }),
      runtime.PayloadType.StreamReliable,
      ctx,
    );

    expect(host.hostCalls.sendDataChunk).toEqual([
      {
        target: {
          tag: 'peer',
          val: {
            ...peer,
            serialNumber: 9n,
          },
        },
        chunk: {
          streamId: 'stream-1',
          sequence: 4n,
          payload: new Uint8Array([1, 2, 3]),
          metadata: [{ key: 'lane', value: 'reliable' }],
          timestampMs: 5678n,
        },
        payloadType: { tag: 'stream-reliable' },
      },
    ]);
  });

  it('sends data chunks using host and workload destinations', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(14n);

    await runtime.sendDataChunk(
      'host',
      testChunk({ streamId: 'host-stream' }),
      runtime.PayloadType.StreamLatencyFirst,
      ctx,
    );
    await runtime.sendDataChunk(
      'workload',
      testChunk({ streamId: 'workload-stream' }),
      runtime.PayloadType.StreamReliable,
      ctx,
    );

    expect(host.hostCalls.sendDataChunk.map((call) => call.target)).toEqual([
      { tag: 'host' },
      { tag: 'workload' },
    ]);
    expect(
      host.hostCalls.sendDataChunk.map((call) => call.payloadType),
    ).toEqual([{ tag: 'stream-latency-first' }, { tag: 'stream-reliable' }]);
  });

  it('normalizes payload, metadata, sequence, and optional timestamp fields', async () => {
    const { host, runtime } = await loadRuntime();
    const buffer = new Uint8Array([9, 8, 7]).buffer;
    const ctx = testInvocationCtx(15n);

    await runtime.sendDataChunk(
      { peer: testActorId(11n) },
      {
        streamId: 'array-buffer-stream',
        sequence: 12,
        payload: buffer,
      },
      runtime.PayloadType.StreamLatencyFirst,
      ctx,
    );
    await runtime.sendDataChunk(
      { peer: testActorId(12n) },
      {
        streamId: 'array-like-stream',
        sequence: 13,
        payload: [6, 5, 4],
      },
      runtime.PayloadType.StreamReliable,
      ctx,
    );

    expect(host.hostCalls.sendDataChunk[0]?.chunk).toEqual({
      streamId: 'array-buffer-stream',
      sequence: 12n,
      payload: new Uint8Array([9, 8, 7]),
      metadata: [],
      timestampMs: undefined,
    });
    expect(host.hostCalls.sendDataChunk[1]?.chunk).toEqual({
      streamId: 'array-like-stream',
      sequence: 13n,
      payload: new Uint8Array([6, 5, 4]),
      metadata: [],
      timestampMs: undefined,
    });
  });
});
