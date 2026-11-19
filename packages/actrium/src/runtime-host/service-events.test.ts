import { describe, expect, test } from 'bun:test';

import type { CapabilityManifestV1 } from './capability-verify';
import {
  createHttpServiceEventSource,
  createNativeServiceEventSource,
  serviceEventsFromManifest,
} from './service-events';
import { createTestServiceEventSource } from './service-events.test-utils';

async function waitFor(condition: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 2));
  }
  expect(condition()).toBe(true);
}

describe('serviceEventsFromManifest', () => {
  test('reads events from the v1 "service" module key', () => {
    const manifest: CapabilityManifestV1 = {
      schemaVersion: 1,
      keelApi: '1.0',
      guestEntry: 'workload/guest.js',
      imports: ['host:service'],
      hostModules: {
        service: {
          rpc: [],
          events: ['notes.v1.changed'],
        },
      },
      components: [],
    };

    expect(serviceEventsFromManifest(manifest)).toEqual(['notes.v1.changed']);
  });

  test('reads events from the legacy "host:service" module key', () => {
    const manifest: CapabilityManifestV1 = {
      schemaVersion: 1,
      keelApi: '1.0',
      guestEntry: 'workload/guest.js',
      imports: ['host:service'],
      hostModules: {
        'host:service': {
          events: ['legacy.event'],
        },
      },
      components: [],
    };

    expect(serviceEventsFromManifest(manifest)).toEqual(['legacy.event']);
  });

  test('merges and deduplicates the v1 and legacy module keys', () => {
    const manifest: CapabilityManifestV1 = {
      schemaVersion: 1,
      keelApi: '1.0',
      guestEntry: 'workload/guest.js',
      imports: ['host:service'],
      hostModules: {
        service: { events: ['shared.event', 'v1.event'] },
        'host:service': { events: ['shared.event', 'legacy.event'] },
      },
      components: [],
    };

    expect(serviceEventsFromManifest(manifest)).toEqual([
      'shared.event',
      'v1.event',
      'legacy.event',
    ]);
  });

  test('rejects a malformed events declaration instead of iterating it', () => {
    const manifest = {
      hostModules: { service: { events: 'notes.v1.changed' } },
    } as unknown as CapabilityManifestV1;
    expect(() => serviceEventsFromManifest(manifest)).toThrow('must be a string array');
  });

  test('returns an empty array when no service module is declared', () => {
    const manifest: CapabilityManifestV1 = {
      schemaVersion: 1,
      keelApi: '1.0',
      guestEntry: 'workload/guest.js',
      imports: [],
      hostModules: {},
      components: [],
    };

    expect(serviceEventsFromManifest(manifest)).toEqual([]);
  });
});

describe('createTestServiceEventSource', () => {
  test('delivers events to subscribers', () => {
    const { source, emit } = createTestServiceEventSource(['a', 'b']);
    const received: { event: string; payloadBase64: string }[] = [];

    source.subscribe((event) => received.push(event));
    emit({ event: 'a', payloadBase64: 'AAEC' });
    emit({ event: 'b', payloadBase64: 'AQID' });

    expect(received).toEqual([
      { event: 'a', payloadBase64: 'AAEC' },
      { event: 'b', payloadBase64: 'AQID' },
    ]);
  });

  test('unsubscribe stops delivery', () => {
    const { source, emit } = createTestServiceEventSource(['a']);
    const received: { event: string; payloadBase64: string }[] = [];

    const unsubscribe = source.subscribe((event) => received.push(event));
    emit({ event: 'a', payloadBase64: 'AAEC' });
    unsubscribe();
    emit({ event: 'a', payloadBase64: 'AAEC' });

    expect(received).toHaveLength(1);
  });

  test('rejects events outside the allowed set', () => {
    const { source, emit } = createTestServiceEventSource(['a']);
    source.subscribe(() => {});

    expect(() => emit({ event: 'b', payloadBase64: 'AAEC' })).toThrow(
      'not in the allowed test event set'
    );
  });
});

describe('createNativeServiceEventSource', () => {
  test('fails closed when the native module is unavailable', () => {
    const source = createNativeServiceEventSource(undefined, 'tab-1', ['a']);

    expect(() => source.subscribe(() => {})).toThrow(
      'native Realm Service event bridge is unavailable'
    );
  });

  test('shares one actor subscription and filters native emitter events', async () => {
    const subscribed: { tabId: string; events: string[] }[] = [];
    const cancelled: string[] = [];
    const nativeModule = {
      addListener: () => {},
      removeListeners: () => {},
      subscribeServiceEvents: async (tabId: string, events: string[]) => {
        subscribed.push({ tabId, events });
        return 'subscription-1';
      },
      unsubscribeServiceEvents: (_tabId: string, subscriptionId: string) => {
        cancelled.push(subscriptionId);
      },
    };
    let emitNative!: (event: unknown) => void;
    const createEmitter = () => ({
      addListener: (_event: 'realmServiceEvent', listener: (event: unknown) => void) => {
        emitNative = listener;
        return { remove: () => {} };
      },
    });
    const source = createNativeServiceEventSource(nativeModule, 'tab-1', ['a'], createEmitter);
    const first: string[] = [];
    const second: string[] = [];
    const unsubscribeFirst = source.subscribe((event) => first.push(event.event));
    const unsubscribeSecond = source.subscribe((event) => second.push(event.event));
    await Promise.resolve();
    await Promise.resolve();

    emitNative({
      tabId: 'tab-1', subscriptionId: 'subscription-1', event: 'a', payloadBase64: 'AAEC',
    });
    emitNative({
      tabId: 'other-tab', subscriptionId: 'subscription-1', event: 'a', payloadBase64: 'AAEC',
    });
    emitNative({
      tabId: 'tab-1', subscriptionId: 'subscription-1', event: 'b', payloadBase64: 'AAEC',
    });

    expect(subscribed).toEqual([{ tabId: 'tab-1', events: ['a'] }]);
    expect(first).toEqual(['a']);
    expect(second).toEqual(['a']);

    unsubscribeFirst();
    expect(cancelled).toEqual([]);
    unsubscribeSecond();
    unsubscribeSecond();
    expect(cancelled).toEqual(['subscription-1']);
  });

  test('buffers events emitted before the native subscribe reply', async () => {
    let resolveSubscription!: (id: string) => void;
    const nativeModule = {
      addListener: () => {},
      removeListeners: () => {},
      subscribeServiceEvents: () => new Promise<string>((resolve) => {
        resolveSubscription = resolve;
      }),
      unsubscribeServiceEvents: () => {},
    };
    let emitNative!: (event: unknown) => void;
    const source = createNativeServiceEventSource(nativeModule, 'tab-1', ['a'], () => ({
      addListener: (_event, listener) => {
        emitNative = listener;
        return { remove: () => {} };
      },
    }));
    const received: string[] = [];
    const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));

    emitNative({
      tabId: 'tab-1', subscriptionId: 'subscription-early', event: 'a', payloadBase64: 'AAEC',
    });
    emitNative({
      tabId: 'tab-1', subscriptionId: 'older-subscription', event: 'a', payloadBase64: 'BAUG',
    });
    expect(received).toEqual([]);
    const lateReceived: string[] = [];
    const unsubscribeLate = source.subscribe((event) => lateReceived.push(event.payloadBase64));

    resolveSubscription('subscription-early');
    await Promise.resolve();
    await Promise.resolve();
    expect(received).toEqual(['AAEC']);
    expect(lateReceived).toEqual([]);

    emitNative({
      tabId: 'tab-1', subscriptionId: 'subscription-early', event: 'a', payloadBase64: 'AQID',
    });
    expect(received).toEqual(['AAEC', 'AQID']);
    expect(lateReceived).toEqual(['AQID']);
    unsubscribeLate();
    unsubscribe();
  });

  test('delivers an empty native payload and rejects malformed base64', async () => {
    const nativeModule = {
      addListener: () => {},
      removeListeners: () => {},
      subscribeServiceEvents: async () => 'subscription-empty',
      unsubscribeServiceEvents: () => {},
    };
    let emitNative!: (event: unknown) => void;
    const source = createNativeServiceEventSource(nativeModule, 'tab-1', ['a'], () => ({
      addListener: (_event, listener) => {
        emitNative = listener;
        return { remove: () => {} };
      },
    }));
    const received: string[] = [];
    const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));
    await Promise.resolve();
    await Promise.resolve();
    emitNative({
      tabId: 'tab-1', subscriptionId: 'subscription-empty', event: 'a', payloadBase64: '',
    });
    emitNative({
      tabId: 'tab-1', subscriptionId: 'subscription-empty', event: 'a', payloadBase64: 'invalid!',
    });
    expect(received).toEqual(['']);
    unsubscribe();
  });

  test('retries transient native startup failure without losing the Guest handler', async () => {
    let calls = 0;
    const cancelled: string[] = [];
    const nativeModule = {
      addListener: () => {},
      removeListeners: () => {},
      subscribeServiceEvents: () => {
        calls += 1;
        if (calls === 1) return Promise.reject(new Error('discovery unavailable'));
        return Promise.resolve(`subscription-${calls}`);
      },
      unsubscribeServiceEvents: (_tabId: string, subscriptionId: string) => {
        cancelled.push(subscriptionId);
      },
    };
    let emitNative!: (event: unknown) => void;
    const source = createNativeServiceEventSource(
      nativeModule,
      'tab-1',
      ['a'],
      () => ({
        addListener: (_event, listener) => {
          emitNative = listener;
          return { remove: () => {} };
        },
      }),
      { initialRetryDelayMs: 0, maxRetryDelayMs: 0 }
    );
    const received: string[] = [];
    const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));

    await waitFor(() => calls === 2);
    emitNative({
      tabId: 'tab-1', subscriptionId: 'subscription-2', event: 'a', payloadBase64: 'AAEC',
    });
    await waitFor(() => received.length === 1);
    expect(received).toEqual(['AAEC']);

    // The cleanup returned before the failed generation still owns the
    // recovered subscription and tears it down normally.
    unsubscribe();
    expect(cancelled).toEqual(['subscription-2']);
  });

  test('cancels a native subscription that resolves after unsubscribe', async () => {
    let resolveSubscription!: (id: string) => void;
    const cancelled: string[] = [];
    const nativeModule = {
      addListener: () => {},
      removeListeners: () => {},
      subscribeServiceEvents: () => new Promise<string>((resolve) => {
        resolveSubscription = resolve;
      }),
      unsubscribeServiceEvents: (_tabId: string, subscriptionId: string) => {
        cancelled.push(subscriptionId);
      },
    };
    const source = createNativeServiceEventSource(nativeModule, 'tab-1', ['a'], () => ({
      addListener: () => ({ remove: () => {} }),
    }));
    const unsubscribe = source.subscribe(() => {});
    unsubscribe();
    resolveSubscription('late-subscription');
    await Promise.resolve();

    expect(cancelled).toEqual(['late-subscription']);
  });
});

describe('createHttpServiceEventSource', () => {
  test('subscribes via fetch and parses SSE events', async () => {
    const chunks = [
      'event: a\ndata: {"payloadBase64":"AAEC"}\n\n',
      'event: b\r\ndata: {"payloadBase64":"AQID"}\r\n\r\n',
      'event: ignored\ndata: {"payloadBase64":"BAD"}\n\n',
    ];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async () =>
      new Response(
        new ReadableStream({
          start(controller) {
            for (const chunk of chunks) {
              controller.enqueue(new TextEncoder().encode(chunk));
            }
            controller.close();
          },
        }),
        { status: 200, headers: { 'Content-Type': 'text/event-stream' } }
      )) as unknown as typeof fetch;

    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a', 'b']);
      const received: { event: string; payloadBase64: string }[] = [];
      const unsubscribe = source.subscribe((event) => received.push(event));

      // Allow the async stream consumer to finish.
      await new Promise((resolve) => setTimeout(resolve, 0));
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(received).toEqual([
        { event: 'a', payloadBase64: 'AAEC' },
        { event: 'b', payloadBase64: 'AQID' },
      ]);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('shares one HTTP connection and closes it after the last unsubscribe', async () => {
    const originalFetch = globalThis.fetch;
    let fetchCalls = 0;
    let requestSignal: AbortSignal | undefined;
    let streamController!: ReadableStreamDefaultController<Uint8Array>;
    const encoder = new TextEncoder();
    globalThis.fetch = (async (
      _input: Parameters<typeof fetch>[0],
      init?: Parameters<typeof fetch>[1]
    ) => {
      fetchCalls += 1;
      requestSignal = init?.signal ?? undefined;
      return new Response(
        new ReadableStream({
          start(controller) {
            streamController = controller;
            controller.enqueue(encoder.encode('event: a\ndata: {"payloadBase64":"AAEC"}\n\n'));
          },
        }),
        { status: 200, headers: { 'Content-Type': 'text/event-stream' } }
      );
    }) as unknown as typeof fetch;

    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a']);
      const first: string[] = [];
      const second: string[] = [];
      const unsubscribeFirst = source.subscribe((event) => first.push(event.payloadBase64));
      const unsubscribeSecond = source.subscribe((event) => second.push(event.payloadBase64));
      await waitFor(() => first.length === 1 && second.length === 1);
      expect(fetchCalls).toBe(1);

      unsubscribeFirst();
      expect(requestSignal?.aborted).toBe(false);
      streamController.enqueue(
        encoder.encode('event: a\ndata: {"payloadBase64":"AQID"}\n\n')
      );
      await waitFor(() => second.length === 2);
      expect(first).toEqual(['AAEC']);
      expect(second).toEqual(['AAEC', 'AQID']);
      expect(fetchCalls).toBe(1);

      unsubscribeSecond();
      expect(requestSignal?.aborted).toBe(true);
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('reconnects when an open fetch stream goes silent', async () => {
    const originalFetch = globalThis.fetch;
    let fetchCalls = 0;
    globalThis.fetch = (async (_input: unknown, init?: RequestInit) => {
      fetchCalls += 1;
      return new Response(
        new ReadableStream({
          start(controller) {
            init?.signal?.addEventListener('abort', () => {
              controller.error(new Error('Connection aborted.'));
            });
          },
        }),
        { status: 200, headers: { 'Content-Type': 'text/event-stream' } }
      );
    }) as unknown as typeof fetch;
    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a'], {
        idleTimeoutMs: 20,
        initialRetryDelayMs: 0,
        maxRetryDelayMs: 0,
      });
      const unsubscribe = source.subscribe(() => {});
      await waitFor(() => fetchCalls >= 2);
      expect(fetchCalls).toBeGreaterThanOrEqual(2);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('restarts a terminal HTTP stream for a new handler', async () => {
    const originalFetch = globalThis.fetch;
    const encoder = new TextEncoder();
    let fetchCalls = 0;
    let secondController: ReadableStreamDefaultController<Uint8Array> | undefined;
    globalThis.fetch = (async () => {
      fetchCalls += 1;
      if (fetchCalls === 1) {
        return new Response('missing', { status: 404 });
      }
      return new Response(
        new ReadableStream({
          start(controller) {
            secondController = controller;
          },
        }),
        { status: 200, headers: { 'Content-Type': 'text/event-stream' } }
      );
    }) as unknown as typeof fetch;

    let unsubscribeFirst: (() => void) | undefined;
    let unsubscribeSecond: (() => void) | undefined;
    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a']);
      const first: string[] = [];
      const second: string[] = [];
      unsubscribeFirst = source.subscribe((event) => first.push(event.payloadBase64));
      await waitFor(() => fetchCalls === 1);
      // Let the terminal response finish and release the active-stream slot.
      await new Promise((resolve) => setTimeout(resolve, 0));

      unsubscribeSecond = source.subscribe((event) => second.push(event.payloadBase64));
      await waitFor(() => fetchCalls === 2 && secondController !== undefined);
      secondController!.enqueue(
        encoder.encode('event: a\ndata: {"payloadBase64":"AAEC"}\n\n')
      );
      await waitFor(() => first.length === 1 && second.length === 1);
      expect(first).toEqual(['AAEC']);
      expect(second).toEqual(['AAEC']);
      expect(fetchCalls).toBe(2);
    } finally {
      unsubscribeSecond?.();
      unsubscribeFirst?.();
      globalThis.fetch = originalFetch;
    }
  });

  test('does not deliver an old SSE batch to a replacement generation', async () => {
    const originalFetch = globalThis.fetch;
    const encoder = new TextEncoder();
    const controllers: ReadableStreamDefaultController<Uint8Array>[] = [];
    let fetchCalls = 0;
    globalThis.fetch = (async () => {
      fetchCalls += 1;
      return new Response(
        new ReadableStream({
          start(controller) {
            controllers.push(controller);
            if (fetchCalls === 1) {
              controller.enqueue(encoder.encode(
                'event: a\ndata: {"payloadBase64":"AAEC"}\n\n' +
                'event: a\ndata: {"payloadBase64":"AQID"}\n\n'
              ));
            }
          },
        }),
        { status: 200, headers: { 'Content-Type': 'text/event-stream' } }
      );
    }) as unknown as typeof fetch;

    let unsubscribeFirst: (() => void) | undefined;
    let unsubscribeReplacement: (() => void) | undefined;
    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a']);
      const first: string[] = [];
      const replacement: string[] = [];
      unsubscribeFirst = source.subscribe((event) => {
        first.push(event.payloadBase64);
        unsubscribeFirst?.();
        unsubscribeReplacement = source.subscribe((replacementEvent) => {
          replacement.push(replacementEvent.payloadBase64);
        });
      });

      await waitFor(() => fetchCalls === 2 && controllers.length === 2 && first.length === 1);
      expect(first).toEqual(['AAEC']);
      expect(replacement).toEqual([]);

      controllers[1]!.enqueue(
        encoder.encode('event: a\ndata: {"payloadBase64":"AwQ="}\n\n')
      );
      await waitFor(() => replacement.length === 1);
      expect(replacement).toEqual(['AwQ=']);
      expect(first).toEqual(['AAEC']);
    } finally {
      unsubscribeReplacement?.();
      unsubscribeFirst?.();
      globalThis.fetch = originalFetch;
    }
  });

  test('rejects events with missing or invalid payloadBase64', async () => {
    const chunks = [
      'event: a\ndata: {}\n\n',
      'event: a\ndata: {"payloadBase64":123}\n\n',
      'event: a\ndata: {"payloadBase64":"not-base64!"}\n\n',
      'event: a\ndata: {"payloadBase64":""}\n\n',
    ];
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async () =>
      new Response(
        new ReadableStream({
          start(controller) {
            for (const chunk of chunks) {
              controller.enqueue(new TextEncoder().encode(chunk));
            }
            controller.close();
          },
        }),
        { status: 200 }
      )) as unknown as typeof fetch;

    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a']);
      const received: { event: string; payloadBase64: string }[] = [];
      const unsubscribe = source.subscribe((event) => received.push(event));

      await new Promise((resolve) => setTimeout(resolve, 0));
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(received).toEqual([{ event: 'a', payloadBase64: '' }]);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('keeps reading after a handler throws', async () => {
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async () =>
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(new TextEncoder().encode(
              'event: a\ndata: {"payloadBase64":"AAEC"}\n\n' +
              'event: a\ndata: {"payloadBase64":"AQID"}\n\n'
            ));
          },
        }),
        { status: 200 }
      )) as unknown as typeof fetch;
    const received: string[] = [];
    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a']);
      const unsubscribe = source.subscribe((event) => {
        received.push(event.payloadBase64);
        if (received.length === 1) throw new Error('Guest failed');
      });
      await waitFor(() => received.length === 2);
      expect(received).toEqual(['AAEC', 'AQID']);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('backs off accept-then-close connections instead of resetting on HTTP 200', async () => {
    const originalFetch = globalThis.fetch;
    const connectedAt: number[] = [];
    globalThis.fetch = (async () => {
      connectedAt.push(Date.now());
      return new Response(new ReadableStream({
        start(controller) { controller.close(); },
      }), { status: 200 });
    }) as unknown as typeof fetch;
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/events',
        ['a'],
        { initialRetryDelayMs: 20 }
      );
      const unsubscribe = source.subscribe(() => {});
      await waitFor(() => connectedAt.length >= 3);
      unsubscribe();
      expect(connectedAt.length).toBeGreaterThanOrEqual(3);
      expect(connectedAt[1] - connectedAt[0]).toBeGreaterThanOrEqual(15);
      expect(connectedAt[2] - connectedAt[1]).toBeGreaterThanOrEqual(30);
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('reconnects with Last-Event-ID and resets backoff after a valid event', async () => {
    const originalFetch = globalThis.fetch;
    const resumeIds: string[] = [];
    let calls = 0;
    globalThis.fetch = (async (_input: unknown, init?: RequestInit) => {
      calls += 1;
      const headers = new Headers(init?.headers);
      resumeIds.push(headers.get('Last-Event-ID') ?? '');
      const body = new ReadableStream({
        start(controller) {
          if (calls === 1) {
            controller.enqueue(new TextEncoder().encode(
              'id: event-41\r\nevent: a\r\ndata: {"payloadBase64":"AAEC"}\r\n\r\n'
            ));
            controller.close();
          }
          // Keep the second connection open until the test unsubscribes.
        },
      });
      return new Response(body, { status: 200 });
    }) as unknown as typeof fetch;
    const received: string[] = [];
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/events',
        ['a'],
        { initialRetryDelayMs: 1 }
      );
      const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));
      await waitFor(() => calls >= 2 && received.length === 1);
      expect(resumeIds).toEqual(['', 'event-41']);
      expect(received).toEqual(['AAEC']);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('ID-only SSE frames set and clear Last-Event-ID without event data', async () => {
    const originalFetch = globalThis.fetch;
    const resumeIds: (string | null)[] = [];
    let calls = 0;
    globalThis.fetch = (async (_input: unknown, init?: RequestInit) => {
      calls += 1;
      resumeIds.push(new Headers(init?.headers).get('Last-Event-ID'));
      if (calls >= 3) return new Response(null, { status: 204 });
      return new Response(new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode(calls === 1 ? 'id: event-41\n\n' : 'id:\n\n'));
          controller.close();
        },
      }), { status: 200 });
    }) as unknown as typeof fetch;
    const originalError = console.error;
    console.error = () => {};
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/events',
        ['a'],
        { initialRetryDelayMs: 0, maxRetryDelayMs: 0 }
      );
      const unsubscribe = source.subscribe(() => {});
      await waitFor(() => calls === 3);
      expect(resumeIds).toEqual([null, 'event-41', null]);
      unsubscribe();
    } finally {
      console.error = originalError;
      globalThis.fetch = originalFetch;
    }
  });

  test('reconnects when an incomplete SSE frame exceeds the buffer limit', async () => {
    const originalFetch = globalThis.fetch;
    let calls = 0;
    globalThis.fetch = (async () => {
      calls += 1;
      const body = new ReadableStream({
        start(controller) {
          if (calls === 1) {
            controller.enqueue(new TextEncoder().encode('data: "too-large"'));
          }
          // No delimiter: the first connection must hit the configured cap.
        },
      });
      return new Response(body, { status: 200 });
    }) as unknown as typeof fetch;
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/events',
        ['a'],
        { initialRetryDelayMs: 1, maxBufferedCharacters: 8 }
      );
      const unsubscribe = source.subscribe(() => {});
      await waitFor(() => calls >= 2);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  test('does not retry a permanent HTTP client error', async () => {
    const originalFetch = globalThis.fetch;
    const originalError = console.error;
    let calls = 0;
    console.error = () => {};
    globalThis.fetch = (async () => {
      calls += 1;
      return new Response(null, { status: 404 });
    }) as unknown as typeof fetch;
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/events',
        ['a'],
        { initialRetryDelayMs: 1 }
      );
      const unsubscribe = source.subscribe(() => {});
      await new Promise((resolve) => setTimeout(resolve, 10));
      expect(calls).toBe(1);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
      console.error = originalError;
    }
  });

  test('uses incremental XHR when fetch has no streaming response body', async () => {
    const originalFetch = globalThis.fetch;
    const originalXhr = globalThis.XMLHttpRequest;
    let fetchSignal: AbortSignal | undefined;
    globalThis.fetch = (async (_input: unknown, init?: RequestInit) => {
      fetchSignal = init?.signal ?? undefined;
      return new Response(null, { status: 200 });
    }) as unknown as typeof fetch;
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = '';
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      ontimeout: (() => void) | null = null;
      open(): void {}
      setRequestHeader(): void {}
      abort(): void {}
      send(): void {
        this.readyState = 3;
        this.status = 200;
        this.responseText = 'event: a\ndata: {"payloadBase64":""}\n\n';
        this.onreadystatechange?.();
        this.onprogress?.();
      }
    }
    globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
    const received: string[] = [];
    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a']);
      const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));
      await waitFor(() => received.length === 1);
      expect(received).toEqual(['']);
      expect(fetchSignal?.aborted).toBe(true);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
      globalThis.XMLHttpRequest = originalXhr;
    }
  });

  test('uses incremental XHR before awaiting fetch on React Native', async () => {
    const originalFetch = globalThis.fetch;
    const originalXhr = globalThis.XMLHttpRequest;
    const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
    let fetchCalls = 0;
    // React Native's whatwg-fetch does not resolve until the open SSE
    // response ends, so this promise deliberately never settles.
    globalThis.fetch = (() => {
      fetchCalls += 1;
      return new Promise<Response>(() => {});
    }) as unknown as typeof fetch;
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { product: 'ReactNative' },
      writable: true,
    });
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = '';
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      ontimeout: (() => void) | null = null;
      open(): void {}
      setRequestHeader(): void {}
      abort(): void {}
      send(): void {
        this.readyState = 3;
        this.status = 200;
        this.responseText = 'event: a\ndata: {"payloadBase64":"AAEC"}\n\n';
        this.onreadystatechange?.();
        this.onprogress?.();
      }
    }
    globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
    const received: string[] = [];
    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a']);
      const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));
      await waitFor(() => received.length === 1);
      expect(received).toEqual(['AAEC']);
      expect(fetchCalls).toBe(0);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
      globalThis.XMLHttpRequest = originalXhr;
      if (originalNavigator) {
        Object.defineProperty(globalThis, 'navigator', originalNavigator);
      } else {
        delete (globalThis as { navigator?: Navigator }).navigator;
      }
    }
  });

  test('resets the XHR liveness deadline on comment heartbeats', async () => {
    const originalFetch = globalThis.fetch;
    const originalXhr = globalThis.XMLHttpRequest;
    const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
    globalThis.fetch = (() => new Promise<Response>(() => {})) as unknown as typeof fetch;
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { product: 'ReactNative' },
      writable: true,
    });
    const requests: FakeXhr[] = [];
    let heartbeat: ReturnType<typeof setInterval> | undefined;
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = '';
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      ontimeout: (() => void) | null = null;
      open(): void {}
      setRequestHeader(): void {}
      abort(): void {}
      send(): void {
        requests.push(this);
        this.readyState = 3;
        this.status = 200;
        this.onreadystatechange?.();
        if (requests.length === 1) {
          heartbeat = setInterval(() => {
            // Comment-only SSE frames carry no event or ID, but their bytes
            // still prove that the connection is alive.
            this.responseText += ': keep-alive\n\n';
            this.onprogress?.();
          }, 5);
        }
      }
    }
    globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
    try {
      const source = createHttpServiceEventSource('https://example.com/events', ['a'], {
        idleTimeoutMs: 100,
        initialRetryDelayMs: 0,
        maxRetryDelayMs: 0,
      });
      const unsubscribe = source.subscribe(() => {});
      await new Promise((resolve) => setTimeout(resolve, 150));
      expect(requests.length).toBe(1);

      if (heartbeat !== undefined) clearInterval(heartbeat);
      heartbeat = undefined;
      await waitFor(() => requests.length >= 2);
      expect(requests.length).toBeGreaterThanOrEqual(2);
      unsubscribe();
    } finally {
      if (heartbeat !== undefined) clearInterval(heartbeat);
      globalThis.fetch = originalFetch;
      globalThis.XMLHttpRequest = originalXhr;
      if (originalNavigator) {
        Object.defineProperty(globalThis, 'navigator', originalNavigator);
      } else {
        delete (globalThis as { navigator?: Navigator }).navigator;
      }
    }
  });

  test('preserves the SSE pathname with React Native URL behavior', async () => {
    const originalFetch = globalThis.fetch;
    const originalXhr = globalThis.XMLHttpRequest;
    const originalUrl = globalThis.URL;
    const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
    globalThis.fetch = (() => new Promise<Response>(() => {})) as unknown as typeof fetch;
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { product: 'ReactNative' },
      writable: true,
    });
    // react-native-macos 0.81's Libraries/Blob/URL.js appends a slash when the
    // input has no query or fragment. Keep that behavior in this regression.
    class ReactNativeUrl extends originalUrl {
      constructor(input: string | URL, base?: string | URL) {
        const raw = String(input);
        super(!/[?#]/.test(raw) && !raw.endsWith('/') ? `${raw}/` : raw, base);
      }
    }
    globalThis.URL = ReactNativeUrl as typeof URL;
    let openedUrl: string | undefined;
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = '';
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      ontimeout: (() => void) | null = null;
      open(_method: string, url: string): void {
        openedUrl = url;
      }
      setRequestHeader(): void {}
      abort(): void {}
      send(): void {
        this.readyState = 3;
        this.status = 200;
      }
    }
    globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/v1/events',
        ['notes.changed']
      );
      const unsubscribe = source.subscribe(() => {});
      await waitFor(() => openedUrl !== undefined);
      const request = new originalUrl(openedUrl!);
      expect(request.pathname).toBe('/v1/events');
      expect(request.searchParams.getAll('event')).toEqual(['notes.changed']);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
      globalThis.XMLHttpRequest = originalXhr;
      globalThis.URL = originalUrl;
      if (originalNavigator) {
        Object.defineProperty(globalThis, 'navigator', originalNavigator);
      } else {
        delete (globalThis as { navigator?: Navigator }).navigator;
      }
    }
  });

  test('rotates incremental XHR at a parsed frame boundary when chunks end mid-frame', async () => {
    const originalFetch = globalThis.fetch;
    const originalXhr = globalThis.XMLHttpRequest;
    const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
    globalThis.fetch = (() => new Promise<Response>(() => {})) as unknown as typeof fetch;
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { product: 'ReactNative' },
      writable: true,
    });
    const frame = (id: number, payloadBase64: string) =>
      `id: ${id}\nevent: a\ndata: {"payloadBase64":"${payloadBase64}"}\n\n`;
    const frames = [frame(1, 'AAEC'), frame(2, 'AQID'), frame(3, 'AgME')];
    // Every progress event contains one complete frame and the start of the
    // next one, so the client never observes an empty buffer at a chunk edge.
    const firstChunk = frames[0] + frames[1].slice(0, 8);
    const secondChunk = frames[1].slice(8) + frames[2].slice(0, 8);
    const requests: { headers: Record<string, string>; responseText: string }[] = [];
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = '';
      headers: Record<string, string> = {};
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      ontimeout: (() => void) | null = null;
      open(): void {}
      setRequestHeader(name: string, value: string): void {
        this.headers[name] = value;
      }
      abort(): void {}
      send(): void {
        const request = { headers: this.headers, responseText: '' };
        requests.push(request);
        this.readyState = 3;
        this.status = 200;
        if (requests.length === 1) {
          this.responseText = firstChunk;
          request.responseText = this.responseText;
          this.onreadystatechange?.();
          this.onprogress?.();
          this.responseText += secondChunk;
          request.responseText = this.responseText;
          this.onprogress?.();
        } else {
          // Resume from the last complete event with the partially received
          // frame replayed in full.
          this.responseText = frames[2];
          request.responseText = this.responseText;
          this.onreadystatechange?.();
          this.onprogress?.();
        }
      }
    }
    globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
    const received: string[] = [];
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/events',
        ['a'],
        { maxXhrResponseCharacters: frames[0].length + frames[1].length }
      );
      const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));
      await waitFor(() => received.length === 3 && requests.length === 2);
      expect(received).toEqual(['AAEC', 'AQID', 'AgME']);
      expect(requests[0].headers['Last-Event-ID']).toBeUndefined();
      expect(requests[1].headers['Last-Event-ID']).toBe('2');
      expect(requests[0].responseText.length).toBeGreaterThanOrEqual(
        frames[0].length + frames[1].length
      );
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
      globalThis.XMLHttpRequest = originalXhr;
      if (originalNavigator) {
        Object.defineProperty(globalThis, 'navigator', originalNavigator);
      } else {
        delete (globalThis as { navigator?: Navigator }).navigator;
      }
    }
  });

  test('rotates incremental XHR and resumes with Last-Event-ID', async () => {
    const originalFetch = globalThis.fetch;
    const originalXhr = globalThis.XMLHttpRequest;
    const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
    globalThis.fetch = (() => new Promise<Response>(() => {})) as unknown as typeof fetch;
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { product: 'ReactNative' },
      writable: true,
    });
    const requests: { headers: Record<string, string>; responseText: string }[] = [];
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = '';
      headers: Record<string, string> = {};
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      ontimeout: (() => void) | null = null;
      open(): void {}
      setRequestHeader(name: string, value: string): void {
        this.headers[name] = value;
      }
      abort(): void {}
      send(): void {
        const request = { headers: this.headers, responseText: '' };
        requests.push(request);
        this.readyState = 3;
        this.status = 200;
        if (requests.length === 1) {
          this.responseText = [
            'id: 1\nevent: a\ndata: {"payloadBase64":"AAEC"}\n\n',
            'id: 2\nevent: a\ndata: {"payloadBase64":"AQID"}\n\n',
            'id: 3\nevent: a\ndata: {"payloadBase64":"AgME"}\n\n',
          ].join('');
        } else {
          this.responseText = 'id: 4\nevent: a\ndata: {"payloadBase64":"BAUG"}\n\n';
        }
        request.responseText = this.responseText;
        this.onreadystatechange?.();
        this.onprogress?.();
      }
    }
    globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
    const received: string[] = [];
    try {
      const source = createHttpServiceEventSource(
        'https://example.com/events',
        ['a'],
        { maxXhrResponseCharacters: 100 }
      );
      const unsubscribe = source.subscribe((event) => received.push(event.payloadBase64));
      await waitFor(() => received.length === 4 && requests.length === 2);
      expect(received).toEqual(['AAEC', 'AQID', 'AgME', 'BAUG']);
      expect(requests[0].headers['Last-Event-ID']).toBeUndefined();
      expect(requests[1].headers['Last-Event-ID']).toBe('3');
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
      globalThis.XMLHttpRequest = originalXhr;
      if (originalNavigator) {
        Object.defineProperty(globalThis, 'navigator', originalNavigator);
      } else {
        delete (globalThis as { navigator?: Navigator }).navigator;
      }
    }
  });
});
