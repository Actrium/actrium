import { beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  ActrId,
  ActrType,
  DataChunk,
  DataChunkEvent,
  InvocationCtx,
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

  it('subscribes to a stream and yields dispatched chunks', async () => {
    const { host, runtime } = await loadRuntime();
    const chunk = testChunk();
    const sender = testActorId();
    const ctx = testInvocationCtx(11n);

    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    const iterator = stream[Symbol.asyncIterator]();
    await runtime.__dispatchDataChunk(chunk, sender, ctx);

    expect(host.hostCalls.registerStream).toEqual(['stream-1']);
    const item = await iterator.next();
    expect(item).toEqual({ done: false, value: { chunk, sender } });
    await stream.close();
  });

  it('forwards an explicit invocation context token to host imports', async () => {
    const { host, runtime } = await loadRuntime();

    const stream = await runtime.dataChunks(
      'stream-with-context',
      undefined,
      testInvocationCtx(37n),
    );

    expect(host.hostCalls.ctxTokens).toEqual([37n]);
    await stream.close();
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
      const stream = await runtime.dataChunks('als-stream');
      await stream.close();
      const sink = await runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
      );
      await sink.send(testChunk());
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
    // given; the sink lane is asserted separately below.
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

    await expect(runtime.dataChunks('missing-context')).rejects.toThrow(
      'requires an InvocationCtx',
    );
    expect(host.hostCalls.ctxTokens).toEqual([]);
    expect(host.hostCalls.registerStream).toEqual([]);
  });

  it('close unregisters the stream and rejects later dispatches', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(12n);

    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    // Inside a live invocation the host unregister import fires.
    await runtime.withInvocationCtx(ctx, () => stream.close());

    expect(host.hostCalls.unregisterStream).toEqual(['stream-1']);
    await expect(
      runtime.__dispatchDataChunk(testChunk(), testActorId(), ctx),
    ).rejects.toThrow('No stream subscription for stream-1');
  });

  it('close outside a live invocation tears down locally only', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(13n);

    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    // No active invocation: the subscription-time token may be retired, so
    // close() must not touch the host import at all.
    await stream.close();

    expect(host.hostCalls.unregisterStream).toEqual([]);
    await expect(
      runtime.__dispatchDataChunk(testChunk(), testActorId(), ctx),
    ).rejects.toThrow('No stream subscription for stream-1');

    // Local teardown still frees the stream for re-subscription.
    const reopened = await runtime.dataChunks('stream-1', undefined, ctx);
    await reopened.close();
  });

  it('rejects duplicate subscriptions for the same stream', async () => {
    const { runtime } = await loadRuntime();
    const ctx = testInvocationCtx(16n);

    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    await expect(
      runtime.dataChunks('stream-1', undefined, ctx),
    ).rejects.toThrow("stream 'stream-1' already has an active subscription");
    await stream.close();

    // After close, subscribing again succeeds.
    const reopened = await runtime.dataChunks('stream-1', undefined, ctx);
    await reopened.close();
  });

  it('reserves the stream ID until host unregistration finishes', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(16n);
    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    let finish!: () => void;
    vi.spyOn(host, 'unregisterStream').mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const closing = runtime.withInvocationCtx(ctx, () => stream.close());
    await expect(
      runtime.dataChunks('stream-1', undefined, ctx),
    ).rejects.toThrow('already has an active subscription');
    await expect(
      runtime.__dispatchDataChunk(testChunk(), testActorId(), ctx),
    ).rejects.toThrow('No stream subscription');
    finish();
    await closing;
    const reopened = await runtime.dataChunks('stream-1', undefined, ctx);
    await reopened.close();
  });

  it('releases the stream ID if host unregistration fails', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(16n);
    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    vi.spyOn(host, 'unregisterStream').mockRejectedValueOnce(
      new Error('failed'),
    );
    await expect(
      runtime.withInvocationCtx(ctx, () => stream.close()),
    ).rejects.toThrow('failed');
    const reopened = await runtime.dataChunks('stream-1', undefined, ctx);
    await reopened.close();
  });

  it('reserves the stream ID while host registration is pending', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(16n);
    let finish!: () => void;
    const registration = vi
      .spyOn(host, 'registerStream')
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            finish = resolve;
          }),
      );
    const first = runtime.dataChunks('stream-1', undefined, ctx);
    await expect(
      runtime.dataChunks('stream-1', undefined, ctx),
    ).rejects.toThrow('already has an active subscription');
    expect(registration).toHaveBeenCalledTimes(1);
    finish();
    const stream = await first;
    const iterator = stream[Symbol.asyncIterator]();
    await runtime.__dispatchDataChunk(testChunk(), testActorId(), ctx);
    expect((await iterator.next()).value.chunk.streamId).toBe('stream-1');
    await stream.close();
  });

  it('releases a stream ID when host registration fails', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(16n);
    vi.spyOn(host, 'registerStream').mockRejectedValueOnce(new Error('failed'));
    await expect(
      runtime.dataChunks('stream-1', undefined, ctx),
    ).rejects.toThrow('failed');
    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    await stream.close();
  });

  it('rejects zero capacity', async () => {
    const { runtime } = await loadRuntime();
    await expect(
      runtime.dataChunks('stream-1', { capacity: 0 }, testInvocationCtx(17n)),
    ).rejects.toThrow('capacity must be a positive integer');
  });

  it('is single-consumer and rejects concurrent next()', async () => {
    const { runtime } = await loadRuntime();
    const ctx = testInvocationCtx(18n);
    const stream = await runtime.dataChunks('stream-1', undefined, ctx);

    const iterator = stream[Symbol.asyncIterator]();
    expect(() => stream[Symbol.asyncIterator]()).toThrow('single-consumer');

    const first = iterator.next();
    await expect(iterator.next()).rejects.toThrow('concurrent next()');
    await stream.close();
    await first;
  });

  it('iterator return() closes the stream', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(19n);
    const stream = await runtime.dataChunks('stream-1', undefined, ctx);

    await runtime.__dispatchDataChunk(testChunk(), testActorId(), ctx);
    await runtime.withInvocationCtx(ctx, async () => {
      for await (const event of stream) {
        void event;
        break;
      }
    });

    expect(host.hostCalls.unregisterStream).toEqual(['stream-1']);
  });

  it('close is idempotent', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(20n);
    const stream = await runtime.dataChunks('stream-1', undefined, ctx);

    await runtime.withInvocationCtx(ctx, async () => {
      await stream.close();
      await stream.close();
    });

    expect(host.hostCalls.unregisterStream).toEqual(['stream-1']);
  });

  it('sends data chunks using WIT-shaped peer destinations', async () => {
    const { host, runtime } = await loadRuntime();
    const peer = testActorId(9);
    const ctx = testInvocationCtx(13n);

    const sink = await runtime.dataChunkSink(
      { peer },
      'stream-1',
      runtime.PayloadType.StreamReliable,
      ctx,
    );
    await sink.send(
      testChunk({
        sequence: 4,
        timestampMs: 5678,
      }),
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

    const hostSink = await runtime.dataChunkSink(
      'host',
      'host-stream',
      runtime.PayloadType.StreamLatencyFirst,
      ctx,
    );
    await hostSink.send(testChunk({ streamId: 'host-stream' }));
    const workloadSink = await runtime.dataChunkSink(
      'workload',
      'workload-stream',
      runtime.PayloadType.StreamReliable,
      ctx,
    );
    await workloadSink.send(testChunk({ streamId: 'workload-stream' }));

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

    const sink = await runtime.dataChunkSink(
      { peer: testActorId(11n) },
      'array-stream',
      runtime.PayloadType.StreamLatencyFirst,
      ctx,
    );
    await sink.send({
      streamId: 'array-stream',
      sequence: 12,
      payload: buffer,
    });
    await sink.send({
      streamId: 'array-stream',
      sequence: 13,
      payload: [6, 5, 4],
    });

    expect(host.hostCalls.sendDataChunk[0]?.chunk).toEqual({
      streamId: 'array-stream',
      sequence: 12n,
      payload: new Uint8Array([9, 8, 7]),
      metadata: [],
      timestampMs: undefined,
    });
    expect(host.hostCalls.sendDataChunk[1]?.chunk).toEqual({
      streamId: 'array-stream',
      sequence: 13n,
      payload: new Uint8Array([6, 5, 4]),
      metadata: [],
      timestampMs: undefined,
    });
  });

  it('rejects non-stream payload types when creating a sink', async () => {
    const { runtime } = await loadRuntime();
    await expect(
      runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.RpcReliable,
        testInvocationCtx(21n),
      ),
    ).rejects.toThrow('is not a DataChunk stream lane');
  });

  it('rejects chunks whose streamId does not match the sink', async () => {
    const { runtime } = await loadRuntime();
    const sink = await runtime.dataChunkSink(
      'host',
      'stream-1',
      runtime.PayloadType.StreamReliable,
      testInvocationCtx(22n),
    );

    await expect(
      sink.send(testChunk({ streamId: 'other-stream' })),
    ).rejects.toThrow('does not match sink streamId');
  });

  it('every sink close waits for accepted sends', async () => {
    const { host, runtime } = await loadRuntime();
    let finish!: () => void;
    vi.spyOn(host, 'sendDataChunk').mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const sink = await runtime.dataChunkSink(
      { peer: testActorId() },
      'stream-1',
      runtime.PayloadType.StreamReliable,
      testInvocationCtx(16n),
    );
    const send = sink.send(testChunk());
    await Promise.resolve();
    const firstClose = sink.close();
    let secondClosed = false;
    const secondClose = sink.close().then(() => {
      secondClosed = true;
    });
    await Promise.resolve();
    await Promise.resolve();
    expect(secondClosed).toBe(false);
    finish();
    await Promise.all([send, firstClose, secondClose]);
    expect(secondClosed).toBe(true);
  });

  it('serializes sink sends and rejects sends after close', async () => {
    const { host, runtime } = await loadRuntime();
    const sink = await runtime.dataChunkSink(
      'host',
      'stream-1',
      runtime.PayloadType.StreamReliable,
      testInvocationCtx(23n),
    );

    await sink.send(testChunk({ sequence: 1 }));
    await sink.send(testChunk({ sequence: 2 }));
    await sink.close();
    await sink.close();
    await expect(sink.send(testChunk({ sequence: 3 }))).rejects.toThrow(
      'DataChunkSink is closed',
    );

    expect(
      host.hostCalls.sendDataChunk.map((call) => call.chunk.sequence),
    ).toEqual([1n, 2n, 3n]);
    // The close-time end-of-stream marker follows the data chunks.
    expect(host.hostCalls.sendDataChunk[2].chunk.endOfStream).toBe(true);
  });

  it('sink close sends an in-band end-of-stream marker inside a live invocation', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(31n);

    await runtime.withInvocationCtx(ctx, async () => {
      const sink = await runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
        ctx,
      );
      await sink.send(testChunk({ sequence: 5n }));
      await sink.close();
    });

    const chunks = host.hostCalls.sendDataChunk.map((call) => call.chunk);
    expect(chunks).toHaveLength(2);
    expect(chunks[0].sequence).toBe(5n);
    expect(chunks[0].endOfStream).toBeUndefined();
    expect(chunks[1].endOfStream).toBe(true);
    expect(chunks[1].sequence).toBe(6n);
    expect(chunks[1].payload).toHaveLength(0);
  });

  it('sink close outside a live invocation still sends the marker via the captured token', async () => {
    const { host, runtime } = await loadRuntime();
    const sink = await runtime.dataChunkSink(
      'host',
      'stream-1',
      runtime.PayloadType.StreamReliable,
      testInvocationCtx(32n),
    );

    await sink.send(testChunk({ sequence: 1n }));
    await sink.close();

    // The host routes actor-global stream sends with a retired token through
    // the instance's standing ABI, so the marker still goes out.
    const chunks = host.hostCalls.sendDataChunk.map((call) => call.chunk);
    expect(chunks.map((chunk) => chunk.sequence)).toEqual([1n, 2n]);
    expect(chunks[1].endOfStream).toBe(true);
  });

  it('an application-sent end marker suppresses the close-time marker', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(34n);

    await runtime.withInvocationCtx(ctx, async () => {
      const sink = await runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
        ctx,
      );
      await sink.send(
        testChunk({
          sequence: 9n,
          endOfStream: true,
          payload: new Uint8Array(0),
        }),
      );
      await sink.close();
    });

    const calls = host.hostCalls.sendDataChunk;
    expect(calls).toHaveLength(1);
    expect(calls[0].chunk.endOfStream).toBe(true);
  });

  it('rejects sends after an end-of-stream marker', async () => {
    const { runtime } = await loadRuntime();
    const sink = await runtime.dataChunkSink(
      'host',
      'stream-1',
      runtime.PayloadType.StreamReliable,
      testInvocationCtx(35n),
    );

    await sink.send(
      testChunk({ endOfStream: true, payload: new Uint8Array(0) }),
    );
    await expect(sink.send(testChunk())).rejects.toThrow(
      'endOfStream marker already sent',
    );
  });

  it('a rejected application-sent marker leaves close() retryable', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(38n);
    const original = host.sendDataChunk;
    let rejectOnce = true;
    vi.spyOn(host, 'sendDataChunk').mockImplementation(
      async (token, target, chunk, payloadType) => {
        if (rejectOnce && chunk.endOfStream === true) {
          rejectOnce = false;
          throw new Error('simulated marker send failure');
        }
        return original(token, target, chunk, payloadType);
      },
    );

    await runtime.withInvocationCtx(ctx, async () => {
      const sink = await runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
        ctx,
      );
      await expect(
        sink.send(
          testChunk({
            sequence: 9n,
            endOfStream: true,
            payload: new Uint8Array(0),
          }),
        ),
      ).rejects.toThrow('simulated marker send failure');
      // The failed application marker was never confirmed: close() sends
      // its own marker rather than treating the stream as ended.
      await sink.close();
    });

    const markers = host.hostCalls.sendDataChunk.filter(
      (call) => call.chunk.endOfStream === true,
    );
    // The mock's rejected attempt is never recorded; the single recorded
    // marker is close()'s retry, carrying the sink-side sequence.
    expect(markers).toHaveLength(1);
    expect(markers[0].chunk.sequence).toBe(10n);
  });

  it('send stays rejected after a failed close; close itself stays retryable', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(39n);
    const original = host.sendDataChunk;
    let rejectOnce = true;
    vi.spyOn(host, 'sendDataChunk').mockImplementation(
      async (token, target, chunk, payloadType) => {
        if (rejectOnce && chunk.endOfStream === true) {
          rejectOnce = false;
          throw new Error('simulated marker failure');
        }
        return original(token, target, chunk, payloadType);
      },
    );

    await runtime.withInvocationCtx(ctx, async () => {
      const sink = await runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
        ctx,
      );
      await sink.send(testChunk({ sequence: 1n }));
      await expect(sink.close()).rejects.toThrow('simulated marker failure');
      // A failed close must not reopen the sink for sends.
      await expect(sink.send(testChunk({ sequence: 2n }))).rejects.toThrow(
        'DataChunkSink is closed',
      );
      // close() itself remains retryable and eventually confirms.
      await sink.close();
    });

    const markers = host.hostCalls.sendDataChunk.filter(
      (call) => call.chunk.endOfStream === true,
    );
    expect(markers).toHaveLength(1);
  });

  it('stream ends on an in-band marker without yielding it', async () => {
    const { runtime } = await loadRuntime();
    const ctx = testInvocationCtx(33n);
    const stream = await runtime.dataChunks('stream-1', undefined, ctx);
    const iterator = stream[Symbol.asyncIterator]();
    const sender = testActorId();

    await runtime.__dispatchDataChunk(testChunk({ sequence: 1n }), sender, ctx);
    await runtime.__dispatchDataChunk(testChunk({ sequence: 2n }), sender, ctx);
    await runtime.__dispatchDataChunk(
      testChunk({
        sequence: 3n,
        endOfStream: true,
        payload: new Uint8Array(0),
      }),
      sender,
      ctx,
    );

    const first = await iterator.next();
    const second = await iterator.next();
    expect(first.done).toBe(false);
    expect(second.done).toBe(false);
    if (!first.done && !second.done) {
      expect(first.value.chunk.sequence).toBe(1n);
      expect(second.value.chunk.sequence).toBe(2n);
    }
    expect(await iterator.next()).toEqual({ done: true, value: undefined });
    // The marker-driven end unregisters the subscription.
    await expect(
      runtime.dataChunks('stream-1', undefined, ctx),
    ).resolves.toBeDefined();
  });

  it('sink close sends the marker with any live token when its own ctx is retired', async () => {
    const { host, runtime } = await loadRuntime();
    const creatingCtx = testInvocationCtx(50n);
    const sink = await runtime.dataChunkSink(
      'host',
      'stream-1',
      runtime.PayloadType.StreamReliable,
      creatingCtx,
    );
    await runtime.withInvocationCtx(creatingCtx, async () => {
      await sink.send(testChunk({ sequence: 2n }));
    });
    // Token 50n settled with its invocation; the close happens inside a
    // different live invocation (the wasm guest shape: no AsyncLocalStorage,
    // so only the explicit-token liveness set is available).
    await runtime.withInvocationCtx(testInvocationCtx(77n), async () => {
      await sink.close();
    });

    const chunks = host.hostCalls.sendDataChunk.map((call) => call.chunk);
    expect(chunks).toHaveLength(2);
    expect(chunks[1].endOfStream).toBe(true);
    expect(chunks[1].sequence).toBe(3n);
  });

  it('rejects an end marker carrying a payload', async () => {
    const { runtime } = await loadRuntime();
    const sink = await runtime.dataChunkSink(
      'host',
      'stream-1',
      runtime.PayloadType.StreamReliable,
      testInvocationCtx(36n),
    );

    await expect(sink.send(testChunk({ endOfStream: true }))).rejects.toThrow(
      'endOfStream marker must not carry a payload',
    );
    // The sink stays usable: the rejected marker was never accepted.
    await sink.send(testChunk({ sequence: 1n }));
  });

  it('a failed close can be retried and never reports the marker as sent', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = testInvocationCtx(37n);
    let failOnce = true;
    const original = host.sendDataChunk;
    vi.spyOn(host, 'sendDataChunk').mockImplementation(
      async (token, target, chunk, payloadType) => {
        if (failOnce && chunk.endOfStream === true) {
          failOnce = false;
          throw new Error('simulated send failure');
        }
        return original(token, target, chunk, payloadType);
      },
    );

    await runtime.withInvocationCtx(ctx, async () => {
      const sink = await runtime.dataChunkSink(
        'host',
        'stream-1',
        runtime.PayloadType.StreamReliable,
        ctx,
      );
      await sink.send(testChunk({ sequence: 1n }));
      await expect(sink.close()).rejects.toThrow('simulated send failure');
      // Retry: the failed close did not confirm the marker.
      await sink.close();
    });

    const markers = host.hostCalls.sendDataChunk.filter(
      (call) => call.chunk.endOfStream === true,
    );
    expect(markers).toHaveLength(1);
  });

  it('ActrContext exposes stream-first adapters', async () => {
    const { host, runtime } = await loadRuntime();
    const ctx = new runtime.ActrContext(testInvocationCtx(96n));

    const stream = await ctx.dataChunks('ctx-stream');
    const invocation = testInvocationCtx(96n);
    await runtime.__dispatchDataChunk(
      testChunk({ streamId: 'ctx-stream' }),
      testActorId(),
      invocation,
    );
    const events: DataChunkEvent[] = [];
    await runtime.withInvocationCtx(invocation, async () => {
      for await (const event of stream) {
        events.push(event);
        break;
      }
    });
    expect(events).toHaveLength(1);

    const sink = await ctx.dataChunkSink(
      'host',
      'ctx-stream',
      runtime.PayloadType.StreamReliable,
    );
    await sink.send(testChunk({ streamId: 'ctx-stream' }));

    expect(host.hostCalls.registerStream).toEqual(['ctx-stream']);
    expect(host.hostCalls.unregisterStream).toEqual(['ctx-stream']);
    expect(host.hostCalls.sendDataChunk).toHaveLength(1);
  });
});
