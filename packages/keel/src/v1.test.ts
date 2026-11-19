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
    'host:service': { call: rpc(), tell: rpc(), on: rpc(), uploadAttachment: rpc() },
    'host:clipboard': { writeText: rpc() },
  },
  guestExports: {},
});

test('records the fixed uploadAttachment transport in the manifest', () => {
  const manifest = createV1CapabilityManifest(
    {
      guestEntry: 'workload/guest.js',
      links: [['host:service', 'uploadAttachment']],
    },
    contract
  );
  expect(manifest.imports).toEqual(['host:service']);
  expect(manifest.hostModules).toEqual({
    service: { rpc: ['uploadAttachment'] },
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

test('rejects an uploadAttachment link when the contract lacks the method', () => {
  const withoutUpload = defineKeelContract({
    version: '1.0.0',
    hostModules: {
      'host:service': { call: rpc() },
    },
    guestExports: {},
  });
  expect(() =>
    createV1CapabilityManifest(
      {
        guestEntry: 'workload/guest.js',
        links: [['host:service', 'uploadAttachment']],
      },
      withoutUpload
    )
  ).toThrow(/absent from the contract/);
});

test('mixes the fixed transport with fixed business routes', () => {
  const route = 'echo.v1.EchoService.Echo';
  const manifest = createV1CapabilityManifest(
    {
      guestEntry: 'workload/guest.js',
      links: [
        ['host:service', 'uploadAttachment'],
        [`${V1_SERVICE_PREFIX}${route}`, 'call'],
      ],
    },
    contract
  );
  expect(manifest.hostModules).toEqual({
    service: { rpc: [route, 'uploadAttachment'] },
  });
});

test('fixed business route links still only accept call, tell and on', () => {
  expect(() =>
    v1ServiceLink(`${V1_SERVICE_PREFIX}echo.v1.EchoService.Echo`, 'uploadAttachment')
  ).toThrow(/invalid fixed route or event/);
});
