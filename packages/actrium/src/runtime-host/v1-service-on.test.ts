import { describe, expect, test } from 'bun:test';
import { createHostModuleDispatch } from 'keel/contract';
import { createV1CapabilityManifest, v1ServiceDispatchId, writeV1Bundle } from 'keel/v1';

import { CapabilityAuthorizer, type NativeCapabilitySnapshot } from './capability-authorization';

import { contract } from './contract';
import { createHostModules } from './host-modules';
import { createTestServiceEventSource } from './service-events.test-utils';
import { bindV1HostModules, verifyV1RuntimeGuest } from './v1-guest';

const eventName = 'notes.v1.changed';
const sourceModule = `host:service/${eventName}`;

function makeHostModules() {
  const { source, emit } = createTestServiceEventSource([eventName, 'notes.v1.other']);
  const snapshot: NativeCapabilitySnapshot = {
    schemaVersion: 1,
    revision: 1,
    subject: {
      publisher: 'actrium', program: 'notes', majorVersion: 1, packageDigest: 'sha256:test',
    },
    audience: 'third_party',
    declaredCapabilities: ['service'],
    grants: {},
    platform: {},
    session: { generation: 1, state: 'active', allowedCapabilities: ['service'] },
  };
  const authorizer = new CapabilityAuthorizer(snapshot, () => {});
  const hostModules = createHostModules({
    hasServiceCapability: true,
    hasServiceOnCapability: true,
    callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
    serviceEventSource: source,
    authorizeCapability: (capability) => authorizer.authorize(capability),
    onCapabilityRevoked: (capability, listener) => authorizer.onRevoked(capability, listener),
    writeClipboardText: async () => {},
  });
  return { hostModules, emit, authorizer };
}

describe('v1 service.on linkage', () => {
  test('verifyV1RuntimeGuest accepts a fixed event link', () => {
    const linkage = {
      guestEntry: 'workload/guest.js',
      links: [[sourceModule, 'on'] as [string, string]],
    };
    const manifest = createV1CapabilityManifest(linkage, contract);
    const script = writeV1Bundle(linkage, 'module.exports = {};');

    const result = verifyV1RuntimeGuest(script, JSON.stringify(manifest), [
      'service.on',
    ]);
    expect(result.ok).toBe(true);
  });

  test('bindV1HostModules filters events by the fixed event name', async () => {
    const linkage = {
      guestEntry: 'workload/guest.js',
      links: [[sourceModule, 'on'] as [string, string]],
    };
    const { hostModules, emit, authorizer } = makeHostModules();
    const bound = bindV1HostModules(
      linkage,
      hostModules,
      (capability) => authorizer.authorize(capability)
    );
    const id = v1ServiceDispatchId(sourceModule, 'on');
    const dispatch = createHostModuleDispatch(bound.contract, bound.hostModules);

    const received: { event: string; payloadBase64: string }[] = [];
    const on = dispatch[id].on as (
      handler: (event: { event: string; payloadBase64: string }) => void
    ) => () => void;
    const unsubscribe = on((event) => received.push(event));

    // Wait for host-modules.on authorization to resolve.
    await Promise.resolve();

    emit({ event: 'notes.v1.other', payloadBase64: 'BAUG' });
    expect(received).toEqual([]);
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
    const { hostModules, authorizer } = makeHostModules();
    const authorize = (capability: Parameters<CapabilityAuthorizer['authorize']>[0]) =>
      authorizer.authorize(capability);
    const bound = bindV1HostModules(linkage, hostModules, authorize);
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
    const sameBound = bindV1HostModules(sameNameLinkage, hostModules, authorize);
    const sameId = v1ServiceDispatchId(sourceModule, 'call');
    expect(sameBound.hostModules[sameId].call).toBeDefined();
    expect(sameBound.hostModules[sameId].on).toBeDefined();
    expect(sameBound.contract.hostModules[sameId]?.call).toBeDefined();
    expect(sameBound.contract.hostModules[sameId]?.on).toBeDefined();
  });
});
