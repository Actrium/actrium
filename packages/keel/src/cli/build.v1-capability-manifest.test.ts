import { describe, expect, it } from 'bun:test';
import { defineKeelContract, rpc, subscription } from '../contract';
import {
  assertAllowedComponents,
  createV1CapabilityManifest,
} from './build';

const contract = defineKeelContract({
  version: '1.0.0',
  hostModules: {
    'host:service': {
      call: rpc<{ route: string }, { payloadBase64: string }>(),
      on: subscription<{ event: string }>(),
    },
    'host:clipboard': {
      read: rpc<void, string>(),
      write: rpc<string, void>(),
    },
  },
  guestExports: {},
});

describe('createV1CapabilityManifest', () => {
  it('emits the v1 12 §5 shape', () => {
    const manifest = createV1CapabilityManifest(
      {
        hostModuleIds: ['host:service', 'host:clipboard'],
        hostCapabilities: [
          'host:service.call',
          'host:service.on',
          'host:clipboard.read',
          'host:clipboard.write',
        ],
      },
      { keelApi: '1.0', guestEntry: 'ui/guest.bundle.js' },
      contract
    );

    expect(manifest).toEqual({
      schemaVersion: 1,
      keelApi: '1.0',
      guestEntry: 'ui/guest.bundle.js',
      imports: ['host:clipboard', 'host:service'],
      hostModules: {
        service: { rpc: ['call'], events: ['on'] },
        clipboard: { rpc: ['read', 'write'], events: [] },
      },
      components: [],
    });
  });

  it('rejects components outside the allowlist', () => {
    expect(() => assertAllowedComponents(['View', 'WebView'])).toThrow(
      'WebView'
    );
  });

  it('maps the preset Image component to ImageAsset', () => {
    expect(() => assertAllowedComponents(['Image', 'View'])).not.toThrow();
    const manifest = createV1CapabilityManifest(
      { hostModuleIds: [], hostCapabilities: [] },
      {
        keelApi: '1.0',
        guestEntry: 'ui/guest.bundle.js',
        components: ['Image', 'View'],
      }
    );
    expect(manifest.components).toEqual(['ImageAsset', 'View']);
  });

  it('keeps imports and components supplied by bundle analysis', () => {
    const manifest = createV1CapabilityManifest(
      {
        hostModuleIds: ['host:service'],
        hostCapabilities: ['host:service.call'],
      },
      {
        keelApi: '1.0',
        guestEntry: 'ui/guest.bundle.js',
        imports: ['react', 'keel/guest'],
        components: ['View', 'Text'],
      },
      contract
    );

    expect(manifest.imports).toEqual([
      'host:service',
      'keel/guest',
      'react',
    ]);
    expect(manifest.components).toEqual(['View', 'Text']);
  });
});
