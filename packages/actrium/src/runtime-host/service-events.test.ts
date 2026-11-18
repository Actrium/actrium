import { describe, expect, test } from 'bun:test';

import type { CapabilityManifestV1 } from './capability-verify';
import {
  createHttpServiceEventSource,
  createNativeServiceEventSource,
  createTestServiceEventSource,
  serviceEventsFromManifest,
} from './service-events';

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

  test('prefers the v1 "service" key over "host:service"', () => {
    const manifest: CapabilityManifestV1 = {
      schemaVersion: 1,
      keelApi: '1.0',
      guestEntry: 'workload/guest.js',
      imports: ['host:service'],
      hostModules: {
        service: { events: ['v1.event'] },
        'host:service': { events: ['legacy.event'] },
      },
      components: [],
    };

    expect(serviceEventsFromManifest(manifest)).toEqual(['v1.event']);
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
      'event: b\ndata: {"payloadBase64":"AQID"}\n\n',
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

  test('rejects events with missing or invalid payloadBase64', async () => {
    const chunks = [
      'event: a\ndata: {}\n\n',
      'event: a\ndata: {"payloadBase64":123}\n\n',
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

      expect(received).toEqual([]);
      unsubscribe();
    } finally {
      globalThis.fetch = originalFetch;
    }
  });
});
