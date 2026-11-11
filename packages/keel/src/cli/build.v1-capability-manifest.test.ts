import { describe, expect, it } from 'bun:test';
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

  it('uses literal service routes and events from usage analysis', () => {
    const manifest = createV1CapabilityManifest(
      {
        hostModuleIds: ['host:service'],
        hostCapabilities: ['host:service.call', 'host:service.on'],
      },
      {
        keelApi: '1.0',
        guestEntry: 'ui/guest.bundle.js',
        service: {
          rpc: ['echo.EchoService.Echo'],
          events: ['demo.chat.v1.messageAdded'],
        },
      },
      contract
    );

    expect(manifest.hostModules.service).toEqual({
      rpc: ['echo.EchoService.Echo'],
      events: ['demo.chat.v1.messageAdded'],
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

const ENTRY_TEMPLATE = (moduleId: string) => `
import { View } from 'keel/guest';
import { Child } from '${moduleId}';

export default function App() {
  return (
    <View>
      <Child />
    </View>
  );
}
`;

const DIRECT_CHILD = `
import { Modal, Text } from 'keel/guest';

export function Child() {
  return (
    <Modal>
      <Text>hello</Text>
    </Modal>
  );
}
`;

const ALIASED_CHILD = `
import { Modal as M, Text } from 'keel/guest';

export function Child() {
  return (
    <M>
      <Text>hello</Text>
    </M>
  );
}
`;

async function expectModalViolation(files: Record<string, string>) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-components-'));
  try {
    for (const [relativePath, content] of Object.entries(files)) {
      const target = path.join(tempDir, relativePath);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, content);
    }

    await expect(
      build({
        entry: path.join(tempDir, 'src/entry.tsx'),
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
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

describe('v1 component allowlist integration', () => {
  it('rejects components rendered by imported modules in minified builds', async () => {
    await expectModalViolation({
      'src/entry.tsx': ENTRY_TEMPLATE('./child'),
      'src/child.tsx': DIRECT_CHILD,
    });
  });

  it('rejects aliased SDK components in minified builds', async () => {
    await expectModalViolation({
      'src/entry.tsx': ENTRY_TEMPLATE('./child'),
      'src/child.tsx': ALIASED_CHILD,
    });
  });

  it('rejects components behind a tsconfig path alias in minified builds', async () => {
    await expectModalViolation({
      'tsconfig.json': JSON.stringify({
        compilerOptions: {
          baseUrl: '.',
          paths: { '@/*': ['src/*'] },
        },
      }),
      'src/entry.tsx': ENTRY_TEMPLATE('@/child'),
      'src/child.tsx': DIRECT_CHILD,
    });
  });

  it('rejects components behind a barrel re-export in minified builds', async () => {
    await expectModalViolation({
      'src/entry.tsx': ENTRY_TEMPLATE('./barrel'),
      'src/barrel.ts': "export { Child } from './child';",
      'src/child.tsx': DIRECT_CHILD,
    });
  });

  it('rejects aliased SDK components in identifier position', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal } from 'keel/guest';

const M = Modal;

export default function App() {
  return <M />;
}
`,
    });
  });

  it('rejects member-expression SDK components in minified builds', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal } from 'keel/guest';

const bag = { C: Modal };

export default function App() {
  return <bag.C>hidden</bag.C>;
}
`,
    });
  });

  it('allows React fragments in minified builds', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-fragment-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';

export default function App() {
  return (
    <>
      <View />
    </>
  );
}
`
      );

      await build({
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
      });
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('fails closed when component provenance is unresolved', async () => {
    const tempDir = fs.mkdtempSync(
      path.join(os.tmpdir(), 'keel-v1-provenance-')
    );
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';

function Wrapper(C) {
  return <C />;
}

export default function App() {
  return <Wrapper C={View} />;
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
      ).rejects.toThrow('provenance');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('emits literal service routes in the v1 manifest', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-service-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { call } from 'host:service';
import { View } from 'keel/guest';

export function send() {
  return call({ route: 'echo.EchoService.Echo', payloadBase64: '' });
}

export default function App() {
  return <View />;
}
`
      );
      const manifestPath = path.join(
        tempDir,
        'dist/capability-manifest.json'
      );

      await build({
        entry: path.join(srcDir, 'entry.tsx'),
        outfile: path.join(tempDir, 'dist/bundle.js'),
        minify: true,
        sourcemap: false,
        watch: false,
        contract,
        v1CapabilityManifest: manifestPath,
      });

      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
      expect(manifest.hostModules.service).toEqual({
        rpc: ['echo.EchoService.Echo'],
        events: [],
      });
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('rejects dynamic service routes', async () => {
    const tempDir = fs.mkdtempSync(
      path.join(os.tmpdir(), 'keel-v1-service-dynamic-')
    );
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { call } from 'host:service';
import { View } from 'keel/guest';

export function send(route: string) {
  return call({ route, payloadBase64: '' });
}

export default function App() {
  return <View />;
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
      ).rejects.toThrow('literal "route"');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('rejects components behind a dynamic import in minified builds', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { View } from 'keel/guest';

void import('./child');

export default function App() {
  return <View />;
}
`,
      'src/child.tsx': DIRECT_CHILD,
    });
  });
});
