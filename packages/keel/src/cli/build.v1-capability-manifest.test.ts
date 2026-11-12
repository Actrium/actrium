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

async function expectModalViolation(
  files: Record<string, string>,
  expected: string = 'Modal'
) {
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
    ).rejects.toThrow(expected);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

type V1Manifest = {
  hostModules: Record<string, { rpc: string[]; events: string[] }>;
  components: string[];
};

async function buildManifest(entrySource: string): Promise<V1Manifest> {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-manifest-'));
  try {
    const srcDir = path.join(tempDir, 'src');
    fs.mkdirSync(srcDir, { recursive: true });
    fs.writeFileSync(path.join(srcDir, 'entry.tsx'), entrySource);
    const manifestPath = path.join(tempDir, 'dist/capability-manifest.json');
    await build({
      entry: path.join(srcDir, 'entry.tsx'),
      outfile: path.join(tempDir, 'dist/bundle.js'),
      minify: true,
      sourcemap: false,
      watch: false,
      contract,
      v1CapabilityManifest: manifestPath,
    });
    return JSON.parse(fs.readFileSync(manifestPath, 'utf8')) as V1Manifest;
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

async function expectBuildFailure(
  entrySource: string,
  expected: string
): Promise<void> {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-failure-'));
  try {
    const srcDir = path.join(tempDir, 'src');
    fs.mkdirSync(srcDir, { recursive: true });
    fs.writeFileSync(path.join(srcDir, 'entry.tsx'), entrySource);
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
    ).rejects.toThrow(expected);
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

  it('rejects a shadowed parameter component', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

function C() {
  return <View />;
}

function Wrapper(C) {
  return <C />;
}

export default function App() {
  return <Wrapper C={Modal} />;
}
`,
      },
      'provenance'
    );
  });

  it('rejects components reassigned after declaration', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

let M = View;
M = Modal;

export default function App() {
  return <M />;
}
`,
    });
  });

  it('rejects components reassigned through destructuring', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

let M = View;
({ M } = { M: Modal });

export default function App() {
  return <M />;
}
`,
      },
      'provenance'
    );
  });

  it('rejects components reassigned through member assignment', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const bag = { C: View };
bag.C = Modal;

export default function App() {
  return <bag.C />;
}
`,
    });
  });

  it('rejects components wrapped in memo', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { memo } from 'react';
import { Modal } from 'keel/guest';

const M = memo(Modal);

export default function App() {
  return <M />;
}
`,
    });
  });

  it('allows forward const arrow components', async () => {
    const manifest = await buildManifest(`
import { View } from 'keel/guest';

export default function App() {
  return <Child />;
}

const Child = () => <View />;
`);

    expect(manifest.components).toEqual(['View']);
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

function send() {
  return call({ route: 'echo.EchoService.Echo', payloadBase64: '' });
}

export default function App() {
  return <View onLayout={() => void send()} />;
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

  it('records routes through a service wrapper object', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

const service = { call };

function send() {
  return service.call({ route: 'echo.EchoService.Echo', payloadBase64: '' });
}

export default function App() {
  return <View onLayout={() => void send()} />;
}
`);

    expect(manifest.hostModules.service).toEqual({
      rpc: ['echo.EchoService.Echo'],
      events: [],
    });
  });

  it('fails closed for service-shaped calls from unknown callees', async () => {
    await expectBuildFailure(
      `
import { call } from 'host:service';
import { View } from 'keel/guest';

function keep() {
  return call({ route: 'keep.Route', payloadBase64: '' });
}

function aliased(payload: { route: string; payloadBase64: string }) {
  return payload;
}

function send() {
  return aliased({ route: 'sneaky.Route', payloadBase64: '' });
}

export default function App() {
  return <View onLayout={() => { void keep(); void send(); }} />;
}
`,
      'host:service provenance'
    );
  });

  it('allows unrelated route/event payloads from local APIs', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

function navigate(input: { route: string }) {
  return input;
}

function track(input: { event: string }) {
  return input;
}

function send() {
  return call({ route: 'echo.EchoService.Echo', payloadBase64: '' });
}

export default function App() {
  return (
    <View
      onLayout={() => {
        navigate({ route: '/home' });
        track({ event: 'click' });
        void send();
      }}
    />
  );
}
`);

    expect(manifest.hostModules.service).toEqual({
      rpc: ['echo.EchoService.Echo'],
      events: [],
    });
  });

  it('rejects Fragment members resolved through unknown objects', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal } from 'keel/guest';

function identity(value: unknown) {
  return value;
}

const bag = identity({ Fragment: Modal });

export default function App() {
  return <bag.Fragment />;
}
`,
      },
      'provenance'
    );
  });

  it('records the effective route for duplicate keys', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

function send() {
  return call({
    route: 'allowed.Route',
    route: 'private.Route',
    payloadBase64: '',
  });
}

export default function App() {
  return <View onLayout={() => void send()} />;
}
`);

    expect(manifest.hostModules.service).toEqual({
      rpc: ['private.Route'],
      events: [],
    });
  });

  it('fails closed when a spread can override the service route', async () => {
    await expectBuildFailure(
      `
import { call } from 'host:service';
import { View } from 'keel/guest';

const override = { route: 'private.Route' };

function send() {
  return call({ route: 'allowed.Route', payloadBase64: '', ...override });
}

export default function App() {
  return <View onLayout={() => void send()} />;
}
`,
      'host:service'
    );
  });

  it('records literal subscription events', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

function subscribe() {
  return on('demo.chat.v1.messageAdded', () => {});
}

export default function App() {
  return <View onLayout={() => void subscribe()} />;
}
`);

    expect(manifest.hostModules.service).toEqual({
      rpc: [],
      events: ['demo.chat.v1.messageAdded'],
    });
  });

  it('fails closed for subscription handlers without a static event name', async () => {
    await expectBuildFailure(
      `
import { on } from 'host:service';
import { View } from 'keel/guest';

function subscribe() {
  return on(() => {});
}

export default function App() {
  return <View onLayout={() => void subscribe()} />;
}
`,
      'statically known event name'
    );
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

function send(route: string) {
  return call({ route, payloadBase64: '' });
}

export default function App() {
  return <View onLayout={() => void send('echo.EchoService.Echo')} />;
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
