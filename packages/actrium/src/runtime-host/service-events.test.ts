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
  test('fails closed when the native module lacks the event method', () => {
    const source = createNativeServiceEventSource({}, 'tab-1', ['a']);

    expect(() => source.subscribe(() => {})).toThrow(
      'native Realm Service event bridge is unavailable'
    );
  });

  test('registers through the native module and returns its unsubscribe', () => {
    const received: { tabId: string; events: string[] }[] = [];
    const nativeModule = {
      addServiceEventListener: (
        tabId: string,
        events: string[],
        _listener: (event: { event: string; payloadBase64: string }) => void
      ) => {
        received.push({ tabId, events });
        return () => {
          received.pop();
        };
      },
    };
    const source = createNativeServiceEventSource(
      nativeModule,
      'tab-1',
      ['a', 'b']
    );

    const unsubscribe = source.subscribe(() => {});
    expect(received).toEqual([{ tabId: 'tab-1', events: ['a', 'b'] }]);

    unsubscribe();
    expect(received).toHaveLength(0);
  });
});

describe('createHttpServiceEventSource', () => {
  test('fails closed when EventSource is not available', () => {
    const source = createHttpServiceEventSource('https://example.com/events', [
      'a',
    ]);

    expect(() => source.subscribe(() => {})).toThrow(
      'Server-Sent Events are not available'
    );
  });
});
