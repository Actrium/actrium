import { afterEach, describe, expect, it } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { defineKeelContract, rpc, subscription } from '../contract';
import {
  assertAllowedComponents,
  build,
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

  it('requires a contract', () => {
    expect(() =>
      createV1CapabilityManifest(
        { hostModuleIds: [], hostCapabilities: [] },
        { keelApi: '1.0', guestEntry: 'ui/guest.bundle.js' }
      )
    ).toThrow('require a contract');
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
      },
      contract
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

describe('v1 component allowlist integration', () => {
  let tempDir: string | undefined;

  afterEach(() => {
    if (tempDir) {
      fs.rmSync(tempDir, { recursive: true, force: true });
      tempDir = undefined;
    }
  });

  it('rejects components rendered by imported modules in minified builds', async () => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-components-'));
    const srcDir = path.join(tempDir, 'src');
    fs.mkdirSync(srcDir, { recursive: true });
    fs.writeFileSync(
      path.join(srcDir, 'entry.tsx'),
      `
import { View } from 'keel/guest';
import { Child } from './child';

export default function App() {
  return (
    <View>
      <Child />
    </View>
  );
}
`
    );
    fs.writeFileSync(
      path.join(srcDir, 'child.tsx'),
      `
import { Modal, Text } from 'keel/guest';

export function Child() {
  return (
    <Modal>
      <Text>hello</Text>
    </Modal>
  );
}
`
    );

    await expect(
      build({
        entry: path.join(srcDir, 'entry.tsx'),
        outfile: path.join(tempDir, 'dist/bundle.js'),
        minify: true,
        sourcemap: false,
        watch: false,
        contract,
        v1CapabilityManifest: path.join(
          tempDir,
          'dist/capability-manifest.json'
        ),
      })
    ).rejects.toThrow('Modal');
  });
});
