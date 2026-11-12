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
        v1GuestEntry: 'dist/bundle.js',
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
      v1GuestEntry: 'dist/bundle.js',
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
        v1GuestEntry: 'dist/bundle.js',
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
        v1GuestEntry: 'dist/bundle.js',
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
        v1GuestEntry: 'dist/bundle.js',
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
      v1GuestEntry: 'dist/bundle.js',
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

const aliased = globalThis.aliased;

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

  it('records the effective route when a known spread overrides it', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

const override = { route: 'private.Route' };

function send() {
  return call({ route: 'allowed.Route', payloadBase64: '', ...override });
}

export default function App() {
  return <View onLayout={() => void send()} />;
}
`);
    expect(manifest.hostModules.service.rpc).toEqual(['private.Route']);
  });

  it('fails closed when an unknown spread can override the service route', async () => {
    await expectBuildFailure(
      `
import { call } from 'host:service';
import { View } from 'keel/guest';

function send(override) {
  return call({ route: 'allowed.Route', payloadBase64: '', ...override });
}

export default function App() {
  return <View onLayout={() => void send({ route: 'private.Route' })} />;
}
`,
      'spread/dynamic key'
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
        v1GuestEntry: 'dist/bundle.js',
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

describe('PR429 review regressions', () => {
  it('rejects a computed key that can override a literal route', async () => {
    await expectBuildFailure(
      `
import { call } from 'host:service';
import { View } from 'keel/guest';

function send(key: string) {
  return call({ route: 'allowed.Route', [key]: 'private.Route', payloadBase64: '' });
}

export default function App() {
  return <View onLayout={() => void send('route')} />;
}
`,
      'host:service'
    );
  });

  it('rejects a component selected by an if/else branch', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

let C = View;
if (globalThis.pickModal) C = Modal;
else C = View;

export default function App() {
  return <C />;
}
`,
      },
      'provenance'
    );
  });

  it('rejects a component selected by a switch branch', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

let C = View;
switch (globalThis.pick) {
  case 1: C = Modal; break;
  default: C = View;
}

export default function App() {
  return <C />;
}
`,
      },
      'provenance'
    );
  });

  it('resolves a multi-hop alias to the SDK component', async () => {
    const manifest = await buildManifest(`
import { View } from 'keel/guest';

const A = View;
const B = A;
const C = B;

export default function App() {
  return <C />;
}
`);
    expect(manifest.components).toEqual(['View']);
  });

  it('resolves component factories reached through an alias', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

const h = createElement;

export default function App() {
  return h(Modal);
}
`,
    });
  });

  it('rejects an object spread that overrides a component binding', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const override = { C: Modal };
const bag = { C: View, ...override };

export default function App() {
  return <bag.C />;
}
`,
    });
  });

  it('rejects a dynamic member write on a component bag', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const bag = { C: View };
bag[globalThis.key] = Modal;

export default function App() {
  return <bag.C />;
}
`,
      },
      'provenance'
    );
  });

  it('rejects a destructuring write to a component member', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const bag = { C: View };
({ C: bag.C } = { C: Modal });

export default function App() {
  return <bag.C />;
}
`,
      },
      'provenance'
    );
  });

  it('records service events behind multi-hop aliases', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const listen = on;
const subscribe = listen;

export default function App() {
  return <View onLayout={() => subscribe('private.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['private.Event']);
  });

  it('records service events behind argument forwarding', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

function wrap(fn) {
  return fn('private.Event', () => {});
}

export default function App() {
  return <View onLayout={() => wrap(on)} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['private.Event']);
  });

  it('records service events behind a wrapper spread', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const base = { on };
const wrapper = { ...base };

export default function App() {
  return <View onLayout={() => wrapper.on('private.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['private.Event']);
  });

  it('records a route that is re-established after a spread', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

const middle = { route: 'middle.Route' };

function send() {
  return call({ route: 'allowed.Route', ...middle, route: 'final.Route', payloadBase64: '' });
}

export default function App() {
  return <View onLayout={() => void send()} />;
}
`);
    expect(manifest.hostModules.service.rpc).toEqual(['final.Route']);
  });

  it('allows React Fragment aliases', async () => {
    const manifest = await buildManifest(`
import { Fragment } from 'react';
import { View } from 'keel/guest';

const F = Fragment;

export default function App() {
  return (
    <F>
      <View />
    </F>
  );
}
`);
    expect(manifest.components).toEqual(['View']);
  });

  it('rejects non-literal dynamic imports with --no-strict', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-dynamic-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';

void import(globalThis.target);

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
          strict: false,
          contract,
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
          v1GuestEntry: 'dist/bundle.js',
        })
      ).rejects.toThrow('non-literal dynamic import');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('rejects an absolute guestEntry', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-entry-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';
export default function App() { return <View />; }
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
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
        })
      ).rejects.toThrow('package-relative guestEntry');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('snapshots a member alias before the source object changes', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const bag = { C: Modal };
const C = bag.C;
bag.C = View;

export default function App() {
  return <C />;
}
`,
    });
  });

  it('keeps a member alias when the source object changes later', async () => {
    const manifest = await buildManifest(`
import { Modal, View } from 'keel/guest';

const bag = { C: View };
const C = bag.C;
bag.C = Modal;

export default function App() {
  return <C />;
}
`);
    expect(manifest.components).toEqual(['View']);
  });

  it('rejects a component chosen through a member alias in a branch', async () => {
    await expectBuildFailure(
      `
import { Modal, View } from 'keel/guest';

const first = { C: View };
const second = { C: Modal };
let C = first.C;
if (globalThis.pick) C = first.C;
else C = second.C;

export default function App() {
  return <C />;
}
`,
      'provenance'
    );
  });

  it('rejects a non-React Fragment member on an arbitrary object', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal } from 'keel/guest';

const bag = { Fragment: Modal };
const F = bag.Fragment;

export default function App() {
  return <F />;
}
`,
    });
  });

  it('resolves a service wrapper declared after the caller', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

export default function App() {
  return <View onLayout={() => wrap(on)} />;
}

function wrap(fn) {
  return fn('hoisted.Event', () => {});
}
`);
    expect(manifest.hostModules.service.events).toEqual(['hoisted.Event']);
  });

  it('resolves a service alias declared after the caller', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

export default function App() {
  return <View onLayout={() => subscribe('late-alias.Event', () => {})} />;
}

const subscribe = on;
`);
    expect(manifest.hostModules.service.events).toEqual(['late-alias.Event']);
  });

  it('resolves a service method assigned through a destructuring member target', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const wrapper = { on: function () {} };
({ on: wrapper.on } = { on });

export default function App() {
  return <View onLayout={() => wrapper.on('destructured.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['destructured.Event']);
  });

  it('normalizes ./ and .. segments in the v1 guestEntry', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-entry-normalize-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';
export default function App() { return <View />; }
`
      );
      const manifestPath = path.join(tempDir, 'dist/capability-manifest.json');
      await build({
        entry: path.join(srcDir, 'entry.tsx'),
        outfile: path.join(tempDir, 'dist/bundle.js'),
        minify: true,
        sourcemap: false,
        watch: false,
        contract,
        v1CapabilityManifest: manifestPath,
        v1GuestEntry: './nested/../dist/bundle.js',
      });
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
      expect(manifest.guestEntry).toBe('dist/bundle.js');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('derives a normalized guestEntry from a relative outfile', async () => {
    const tempDir = fs.mkdtempSync(path.join(process.cwd(), 'tmp-v1-entry-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(path.join(tempDir, 'dist'), { recursive: true });
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';
export default function App() { return <View />; }
`
      );
      const outfile = path.join(tempDir, 'dist/bundle.js');
      const relativeOutfile = `./${path.relative(process.cwd(), outfile)}`;
      const manifestPath = path.join(tempDir, 'dist/capability-manifest.json');
      await build({
        entry: path.join(srcDir, 'entry.tsx'),
        outfile: relativeOutfile,
        minify: true,
        sourcemap: false,
        watch: false,
        contract,
        v1CapabilityManifest: manifestPath,
      });
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
      expect(manifest.guestEntry).toBe(
        path.relative(process.cwd(), outfile).split(path.sep).join('/')
      );
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('rejects a guest-defined import_guest namespace', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-import-guest-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { Modal } from 'keel/guest';

const import_guest = { View: Modal };

export default function App() {
  return <import_guest.View />;
}
`
      );
      await expect(
        build({
          entry: path.join(srcDir, 'entry.tsx'),
          outfile: path.join(tempDir, 'dist/bundle.js'),
          minify: false,
          sourcemap: false,
          watch: false,
          contract,
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
          v1GuestEntry: 'dist/bundle.js',
        })
      ).rejects.toThrow('Modal');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('records a bound service method', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const subscribe = on.bind(undefined);

export default function App() {
  return <View onLayout={() => subscribe('bound.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['bound.Event']);
  });

  it('rejects optional eval calls with --no-strict', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-optional-eval-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';

void eval?.('return 1');
void globalThis.eval?.('return 1');

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
          strict: false,
          contract,
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
          v1GuestEntry: 'dist/bundle.js',
        })
      ).rejects.toThrow('eval call');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('rejects every dynamic code evaluation shape with --no-strict', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-dynamic-eval-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';

void Function('return 1')();
void (0, eval)('return 1');
void globalThis['eval']('return 1');
void new Function('return 1')();
void window.eval('return 1');

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
          strict: false,
          contract,
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
          v1GuestEntry: 'dist/bundle.js',
        })
      ).rejects.toThrow('eval call');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('records events invoked through a .call wrapper', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

function subscribe() {
  return on.call(on, 'call-wrapped.Event', () => {});
}

export default function App() {
  return <View onLayout={() => void subscribe()} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['call-wrapped.Event']);
  });

  it('records events invoked through an .apply wrapper', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

function subscribe() {
  return on.apply(on, ['apply-wrapped.Event', () => {}]);
}

export default function App() {
  return <View onLayout={() => void subscribe()} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['apply-wrapped.Event']);
  });

  it('records events invoked through Reflect.apply', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

function subscribe() {
  return Reflect.apply(on, undefined, ['reflect-wrapped.Event', () => {}]);
}

export default function App() {
  return <View onLayout={() => void subscribe()} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['reflect-wrapped.Event']);
  });

  it('records events behind an on.call.bind(on) rebind', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const subscribe = on.call.bind(on);

export default function App() {
  return <View onLayout={() => subscribe('rebound.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['rebound.Event']);
  });

  it('fails closed for an .apply wrapper without a literal argument array', async () => {
    await expectBuildFailure(
      `
import { on } from 'host:service';
import { View } from 'keel/guest';

const args = ['hidden.Event'];

function subscribe() {
  return on.apply(on, args);
}

export default function App() {
  return <View onLayout={() => void subscribe()} />;
}
`,
      'literal argument array'
    );
  });

  it('rejects components created through a factory .call wrapper', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

export default function App() {
  return createElement.call(createElement, Modal);
}
`,
    });
  });

  it('rejects components created through a factory .apply wrapper', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

export default function App() {
  return createElement.apply(null, [Modal]);
}
`,
    });
  });

  it('rejects components created through Reflect.apply of a factory', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

export default function App() {
  return Reflect.apply(createElement, null, [Modal]);
}
`,
    });
  });

  it('rejects components behind a factory call rebind', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

const h = createElement.call.bind(createElement);

export default function App() {
  return h(Modal);
}
`,
    });
  });

  it('fails closed when Object.assign mutates a tracked component bag', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const bag = { C: View };
Object.assign(bag, { C: Modal });

export default function App() {
  return <bag.C />;
}
`,
      },
      'provenance'
    );
  });

  it('fails closed when Object.defineProperty mutates a tracked component bag', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const bag = { C: View };
Object.defineProperty(bag, 'C', { value: Modal });

export default function App() {
  return <bag.C />;
}
`,
      },
      'provenance'
    );
  });

  // Note: a plain `import * as guest from 'keel/guest'` cannot reach the
  // manifest step in this pipeline — Bun's __toESM helper for namespace
  // imports contains a let/const loop head that the Hermes compat guard
  // rejects — so the namespace-resolution path is covered indirectly by the
  // mutation test below (the analyzer still recognizes the __toESM-wrapped
  // require as the guest namespace).
  it('fails closed when the keel/guest namespace is mutated', async () => {
    await expectBuildFailure(
      `
import * as guest from 'keel/guest';
import { Modal, View } from 'keel/guest';

Object.assign(guest, { View: Modal });

export default function App() {
  return <guest.View />;
}
`,
      'keel/guest namespace'
    );
  });

  it('rejects a component bound through array destructuring', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { Modal } from 'keel/guest';

const [C] = [Modal];

export default function App() {
  return <C />;
}
`,
    });
  });

  it('fails closed for a component bound through a parameter default', async () => {
    await expectBuildFailure(
      `
import { Modal, View } from 'keel/guest';

function render(C = Modal) {
  return <C />;
}

export default function App() {
  return <View onLayout={() => render()} />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('resolves a component from a destructuring default value', async () => {
    const manifest = await buildManifest(`
import { View } from 'keel/guest';

const bag = {};
const { C = View } = bag;

export default function App() {
  return <C />;
}
`);
    expect(manifest.components).toEqual(['View']);
  });

  it('fails closed when a parameter shadows the bundler guest namespace name', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { View } from 'keel/guest';

function render(import_guest) {
  return <import_guest.View />;
}

export default function App() {
  return <View onLayout={() => render(globalThis.anything)} />;
}
`,
      },
      'provenance'
    );
  });

  it('normalizes Windows separators in the v1 guestEntry', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-entry-win-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';
export default function App() { return <View />; }
`
      );
      const manifestPath = path.join(tempDir, 'dist/capability-manifest.json');
      await build({
        entry: path.join(srcDir, 'entry.tsx'),
        outfile: path.join(tempDir, 'dist/bundle.js'),
        minify: true,
        sourcemap: false,
        watch: false,
        contract,
        v1CapabilityManifest: manifestPath,
        v1GuestEntry: 'dist\\bundle.js',
      });
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
      expect(manifest.guestEntry).toBe('dist/bundle.js');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('rejects a guestEntry whose symlink resolves outside the package root', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-entry-symlink-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.mkdirSync(path.join(tempDir, 'dist'), { recursive: true });
      fs.mkdirSync(path.join(tempDir, 'outside'), { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';
export default function App() { return <View />; }
`
      );
      // The outfile path stays inside the temp package, but it is a symlink
      // to a file outside of it, so the realpath-derived entry must not be
      // accepted as package-relative.
      fs.symlinkSync(
        path.join(tempDir, 'outside/bundle.js'),
        path.join(tempDir, 'dist/bundle.js')
      );
      await expect(
        build({
          entry: path.join(srcDir, 'entry.tsx'),
          outfile: path.join(tempDir, 'dist/bundle.js'),
          minify: true,
          sourcemap: false,
          watch: false,
          contract,
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
        })
      ).rejects.toThrow('package-relative guestEntry');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('fails closed for an alias/spread/branch/computed-key component chain', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

const base = { C: View };
let pick = base;
if (globalThis.flag) pick = { C: Modal };
const merged = { ...pick };
const key = 'C';
const C = merged[key];

export default function App() {
  return <C />;
}
`,
      },
      'provenance'
    );
  });

  it('records an event through an alias/spread/branch/computed-key service chain', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const methods = { on };
const wrapper = { ...methods };
const key = 'on';
let handler = wrapper[key];
if (globalThis.flag) handler = methods.on;

export default function App() {
  return <View onLayout={() => handler('combo.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['combo.Event']);
  });

  it('resolves a second keel/guest namespace across modules in minified builds', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { View } from 'keel/guest';
import { Child } from './child';

export default function App() {
  return (
    <View>
      <Child />
    </View>
  );
}
`,
      'src/child.tsx': `
import { Modal as M } from 'keel/guest';

export function Child() {
  return <M />;
}
`,
    });
  });
});

describe('PR429 third-round review regressions', () => {
  it('rejects a custom footer in v1 mode', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-footer-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';
export default function App() { return <View />; }
`
      );
      const footerPath = path.join(tempDir, 'custom-footer.js');
      fs.writeFileSync(footerPath, 'void globalThis.React.createElement(globalThis.Unknown);');
      await expect(
        build({
          entry: path.join(srcDir, 'entry.tsx'),
          outfile: path.join(tempDir, 'dist/bundle.js'),
          minify: true,
          sourcemap: false,
          watch: false,
          contract,
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
          v1GuestEntry: 'dist/bundle.js',
          footer: footerPath,
        })
      ).rejects.toThrow('custom --footer');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('fails closed for a local memo that is not the React wrapper', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

function memo(C) {
  return globalThis.Sneaky;
}

const M = memo(Modal);

export default function App() {
  return (
    <View>
      <M />
    </View>
  );
}
`,
      },
      'provenance'
    );
  });

  it('fails closed for a captured write in a function body that may not run', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

let C = Modal;

function maybeCalled() {
  C = View;
}

// Keep the function in the bundle: the bundler tree-shakes an unreferenced
// declaration (and a bare void reference), which would legitimately reduce
// the analysis to C = Modal.
globalThis.keep = maybeCalled;

export default function App() {
  return <C />;
}
`,
      },
      'provenance'
    );
  });

  it('records events behind a nested member assignment', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const api = { wrapper: {} };
api.wrapper.subscribe = on;

export default function App() {
  return <View onLayout={() => api.wrapper.subscribe('nested.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['nested.Event']);
  });

  it('rejects components created through a factory stored in an object member', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

const factory = { make: createElement };

export default function App() {
  return factory.make(Modal);
}
`,
    });
  });

  it('keeps properties installed before a known object spread', async () => {
    const manifest = await buildManifest(`
import { call, on } from 'host:service';
import { View } from 'keel/guest';

const both = { on, ...{ call } };

export default function App() {
  return <View onLayout={() => both.on('spread-keep.Event', () => {})} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['spread-keep.Event']);
  });

  it('records events behind a bind with pre-bound arguments', async () => {
    const manifest = await buildManifest(`
import { on } from 'host:service';
import { View } from 'keel/guest';

const preset = on.bind(undefined, 'preset.Event');

export default function App() {
  return <View onLayout={() => preset()} />;
}
`);
    expect(manifest.hostModules.service.events).toEqual(['preset.Event']);
  });

  it('rejects components behind a bind with pre-bound arguments', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

const preset = createElement.bind(null, Modal);

export default function App() {
  return preset({});
}
`,
    });
  });

  it('fails closed for a component chosen through a conditional expression', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

let C = Modal;

globalThis.flag ? (C = View) : void 0;

export default function App() {
  return <C />;
}
`,
      },
      'provenance'
    );
  });

  it('fails closed for a component chosen through a short-circuit expression', async () => {
    await expectModalViolation(
      {
        'src/entry.tsx': `
import { Modal, View } from 'keel/guest';

let C = Modal;

globalThis.flag && (C = View);

export default function App() {
  return <C />;
}
`,
      },
      'provenance'
    );
  });

  it('rejects computed globalThis eval access with --no-strict', async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-computed-eval-'));
    try {
      const srcDir = path.join(tempDir, 'src');
      fs.mkdirSync(srcDir, { recursive: true });
      fs.writeFileSync(
        path.join(srcDir, 'entry.tsx'),
        `
import { View } from 'keel/guest';

void globalThis['eval']('return 1');

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
          strict: false,
          contract,
          v1CapabilityManifest: path.join(tempDir, 'dist/capability-manifest.json'),
          v1GuestEntry: 'dist/bundle.js',
        })
      ).rejects.toThrow('eval call');
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it('fails closed for a chained service accessor call', async () => {
    await expectBuildFailure(
      `
import { on } from 'host:service';
import { View } from 'keel/guest';

export default function App() {
  return <View onLayout={() => on.call.call(on, null, 'private.Event', () => {})} />;
}
`,
      'chained accessor call'
    );
  });

  it('fails closed for a destructuring default over an unknown present property', async () => {
    await expectBuildFailure(
      `
import { Modal, View } from 'keel/guest';

function pick() {
  return Modal;
}

const bag = { C: pick() };
const { C = View } = bag;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps the bound argument prefix through a rebind', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

const a = call.bind(null, { route: 'private.Route', payloadBase64: '' });
const b = a.bind(null, { route: 'allowed.Route', payloadBase64: '' });

export default function App() {
  return <View onLayout={() => b()} />;
}
`);
    expect(manifest.hostModules.service.rpc).toEqual(['private.Route']);
  });

  it('keeps the bound argument prefix through a .call wrapper', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

const a = call.bind(null, { route: 'private.Route', payloadBase64: '' });

export default function App() {
  return <View onLayout={() => a.call(null, { route: 'allowed.Route', payloadBase64: '' })} />;
}
`);
    expect(manifest.hostModules.service.rpc).toEqual(['private.Route']);
  });

  it('keeps the bound argument prefix through Reflect.apply', async () => {
    const manifest = await buildManifest(`
import { call } from 'host:service';
import { View } from 'keel/guest';

const a = call.bind(null, { route: 'private.Route', payloadBase64: '' });

export default function App() {
  return <View onLayout={() => Reflect.apply(a, null, [{ route: 'allowed.Route', payloadBase64: '' }])} />;
}
`);
    expect(manifest.hostModules.service.rpc).toEqual(['private.Route']);
  });

  it('fails closed for Reflect.apply of a service accessor', async () => {
    await expectBuildFailure(
      `
import { call } from 'host:service';
import { View } from 'keel/guest';

const a = call.bind(null, { route: 'private.Route', payloadBase64: '' });

export default function App() {
  return <View onLayout={() => Reflect.apply(a.call, null, [])} />;
}
`,
      'Reflect.apply of an accessor'
    );
  });

  it('keeps the bound factory prefix through .call and .apply', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal, View } from 'keel/guest';

const h = createElement.bind(null, Modal, {});

export default function App() {
  return h.call(null, View);
}
`,
    });
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal, View } from 'keel/guest';

const h = createElement.bind(null, Modal, {});

export default function App() {
  return h.apply(null, [View]);
}
`,
    });
  });

  it('rejects a factory bound through multiple bind calls', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal, View } from 'keel/guest';

const h = createElement.bind(null, Modal).bind(null, {});

export default function App() {
  return h(View);
}
`,
    });
  });

  it('fails closed for a chained factory accessor call', async () => {
    await expectBuildFailure(
      `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

export default function App() {
  return createElement.call.call(createElement, null, Modal, {});
}
`,
      'chained JSX factory accessor call'
    );
  });

  it('rejects a computed destructuring key with a default', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

function key() {
  return 'x';
}

const { [key()]: C = View } = { x: Modal };

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('snapshots bound factory arguments at bind time', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal, View } from 'keel/guest';

let C = Modal;
const make = createElement.bind(null, C, {});
C = View;

export default function App() {
  return make();
}
`,
    });
  });

  it('records the factory behind a bound call accessor', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

const make = createElement.call.bind(createElement, null, Modal, {});

export default function App() {
  return make();
}
`,
    });
  });

  it('records the factory behind a bound apply accessor', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal } from 'keel/guest';

const make = createElement.apply.bind(createElement, null, [Modal, {}]);

export default function App() {
  return make();
}
`,
    });
  });

  it('fails closed for a prototype-backed destructuring default', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const { C = View } = { __proto__: { C: Modal } };

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('fails closed after an indexed write to a destructured array', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View];
arr[0] = Modal;
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('fails closed when a bound service argument can change after bind', async () => {
    await expectBuildFailure(
      `
import { call } from 'host:service';
import { View } from 'keel/guest';

let route = { route: 'private.Route', payloadBase64: '' };
const send = call.bind(null, route);
route = { route: 'allowed.Route', payloadBase64: '' };

export default function App() {
  return <View onLayout={() => send()} />;
}
`,
      'bound argument provenance'
    );
  });

  it('keeps array invalidation through branch cloning', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View];
arr[0] = Modal;
if (globalThis.flag) { globalThis.probe = 1; } else { globalThis.probe = 2; }
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps array invalidation through destructuring assignment', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View];
[arr[0]] = [Modal];
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps array invalidation through Object.assign', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View];
Object.assign(arr, { 0: Modal });
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps bind-time snapshots through an accessor rebind', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal, View } from 'keel/guest';

let C = Modal;
const a = createElement.bind(null, C, {});
C = View;
const b = a.call.bind(a, null);

export default function App() {
  return b();
}
`,
    });
  });

  it('keeps the accessor receiver factory state through call.bind', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal, View } from 'keel/guest';

const a = createElement.bind(null, Modal, {});
const b = createElement.call.bind(a, null, View, {});

export default function App() {
  return b();
}
`,
    });
  });

  it('keeps the accessor receiver factory state through apply.bind', async () => {
    await expectModalViolation({
      'src/entry.tsx': `
import { createElement } from 'react';
import { Modal, View } from 'keel/guest';

const a = createElement.bind(null, Modal, {});
const b = createElement.apply.bind(a, null, [View, {}]);

export default function App() {
  return b();
}
`,
    });
  });

  it('keeps array invalidation through in-place methods', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View];
arr.splice(0, 1, Modal);
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View, Modal];
arr.reverse();
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps array invalidation through Array.prototype call', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View];
Array.prototype.splice.call(arr, 0, 1, Modal);
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps the snapshot for non-mutating array methods', async () => {
    const manifest = await buildManifest(`
import { View } from 'keel/guest';

const arr = [View];
arr.map((item) => item);
const [C] = arr;

export default function App() {
  return <C />;
}
`);
    expect(manifest.components).toEqual(['View']);
  });

  it('keeps array invalidation through Reflect.apply', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View, Modal];
Reflect.apply(Array.prototype.reverse, arr, []);
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps array invalidation through a mutator alias', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View, Modal];
const reverse = arr.reverse;
reverse.call(arr);
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps array invalidation through a dynamic method key', async () => {
    await expectBuildFailure(
      `
import { View, Modal } from 'keel/guest';

const arr = [View, Modal];
function key() { return 'reverse'; }
arr[key()]();
const [C] = arr;

export default function App() {
  return <C />;
}
`,
      'Unable to establish component provenance'
    );
  });

  it('keeps the snapshot for non-mutating Array.prototype methods', async () => {
    const manifest = await buildManifest(`
import { View } from 'keel/guest';

const arr = [View];
Array.prototype.map.call(arr, (item) => item);
const [C] = arr;

export default function App() {
  return <C />;
}
`);
    expect(manifest.components).toEqual(['View']);
  });
});
