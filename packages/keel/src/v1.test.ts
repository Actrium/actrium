import { expect, test } from 'bun:test';
import { defineKeelContract, rpc } from './contract';
import {
  createV1CapabilityManifest,
  V1_SERVICE_PREFIX,
  v1ServiceLink,
} from './v1';

const contract = defineKeelContract({
  version: '1.0.0',
  hostModules: {
    'host:service': { call: rpc(), tell: rpc(), on: rpc() },
    'host:clipboard': { writeText: rpc() },
  },
  guestExports: {},
});

test('records a fixed business route call in the manifest', () => {
  const route = 'echo.v1.EchoService.Echo';
  const manifest = createV1CapabilityManifest(
    {
      guestEntry: 'workload/guest.js',
      links: [[`${V1_SERVICE_PREFIX}${route}`, 'call']],
    },
    contract
  );
  expect(manifest.imports).toEqual(['host:service']);
  expect(manifest.hostModules).toEqual({
    service: { rpc: [route] },
  });
});

test('keeps raw call, tell and on imports rejected on host:service', () => {
  for (const exportName of ['call', 'tell', 'on']) {
    expect(() =>
      createV1CapabilityManifest(
        {
          guestEntry: 'workload/guest.js',
          links: [['host:service', exportName]],
        },
        contract
      )
    ).toThrow(/fixed route or event/);
  }
});

test('mixes several fixed business routes', () => {
  const first = 'echo.v1.EchoService.Echo';
  const second = 'notes.NotesService.Save';
  const manifest = createV1CapabilityManifest(
    {
      guestEntry: 'workload/guest.js',
      links: [
        [`${V1_SERVICE_PREFIX}${first}`, 'call'],
        [`${V1_SERVICE_PREFIX}${second}`, 'call'],
      ],
    },
    contract
  );
  expect(manifest.hostModules).toEqual({
    service: { rpc: [first, second].sort() },
  });
});

test('fixed business route links still only accept call, tell and on', () => {
  expect(() =>
    v1ServiceLink(`${V1_SERVICE_PREFIX}echo.v1.EchoService.Echo`, 'notAllowed')
  ).toThrow(/invalid fixed route or event/);
});
