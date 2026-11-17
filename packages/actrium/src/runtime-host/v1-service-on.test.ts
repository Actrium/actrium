import { describe, expect, test } from 'bun:test';
import { createV1CapabilityManifest, v1ServiceDispatchId, writeV1Bundle } from 'keel/v1';

import { contract } from './contract';
import { createHostModules } from './host-modules';
import { createTestServiceEventSource } from './service-events';
import { bindV1HostModules, verifyV1RuntimeGuest } from './v1-guest';

const eventName = 'notes.v1.changed';
const sourceModule = `host:service/${eventName}`;

function makeHostModules() {
  const { source, emit } = createTestServiceEventSource([eventName]);
  const hostModules = createHostModules({
    hasServiceCapability: true,
    hasServiceOnCapability: true,
    callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
    serviceEventSource: source,
    authorizeCapability: async () => {},
    writeClipboardText: async () => {},
  });
  return { hostModules, emit };
}

describe('v1 service.on linkage', () => {
  test('verifyV1RuntimeGuest rejects a fixed event link while unimplemented', () => {
    const linkage = {
      guestEntry: 'workload/guest.js',
      links: [[sourceModule, 'on'] as [string, string]],
    };
    const manifest = createV1CapabilityManifest(linkage, contract);
    const script = writeV1Bundle(linkage, 'module.exports = {};');

    const result = verifyV1RuntimeGuest(script, JSON.stringify(manifest), [
      'service.on',
    ]);
    expect(result.ok).toBe(false);
    expect(result.code).toBe('unknown_capability');
  });

  test('bindV1HostModules filters events by the fixed event name', async () => {
    const linkage = {
      guestEntry: 'workload/guest.js',
      links: [[sourceModule, 'on'] as [string, string]],
    };
    const { hostModules, emit } = makeHostModules();
    const bound = bindV1HostModules(linkage, hostModules, async () => {});
    const id = v1ServiceDispatchId(sourceModule, 'on');

    const received: { event: string; payloadBase64: string }[] = [];
    const unsubscribe = bound.hostModules[id].on((event) => received.push(event));

    // Wait for host-modules.on authorization to resolve.
    await Promise.resolve();

    emit({ event: eventName, payloadBase64: 'AAEC' });
    expect(received).toEqual([{ event: eventName, payloadBase64: 'AAEC' }]);

    unsubscribe();
    emit({ event: eventName, payloadBase64: 'AAEC' });
    expect(received).toHaveLength(1);
  });

  test('bindV1HostModules merges call and on for the same business name', () => {
    const route = 'notes.v1.NoteService.List';
    const routeModule = `host:service/${route}`;
    const linkage = {
      guestEntry: 'workload/guest.js',
      links: [
        [routeModule, 'call'] as [string, string],
        [sourceModule, 'on'] as [string, string],
      ],
    };
    const { hostModules } = makeHostModules();
    const bound = bindV1HostModules(linkage, hostModules, async () => {});
    const callId = v1ServiceDispatchId(routeModule, 'call');
    const onId = v1ServiceDispatchId(sourceModule, 'on');

    // Different names produce different dispatch ids; both exports must exist.
    expect(bound.hostModules[callId].call).toBeDefined();
    expect(bound.hostModules[onId].on).toBeDefined();

    // Same-name call+on must merge into one module entry.
    const sameNameLinkage = {
      guestEntry: 'workload/guest.js',
      links: [
        [sourceModule, 'call'] as [string, string],
        [sourceModule, 'on'] as [string, string],
      ],
    };
    const sameBound = bindV1HostModules(sameNameLinkage, hostModules, async () => {});
    const sameId = v1ServiceDispatchId(sourceModule, 'call');
    expect(sameBound.hostModules[sameId].call).toBeDefined();
    expect(sameBound.hostModules[sameId].on).toBeDefined();
  });
});
