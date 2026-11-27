import { afterEach, beforeEach, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type React from 'react';
import { ScrollView as NativeScrollView } from 'react-native';
import { act, create } from 'react-test-renderer';
import { defineKeelContract, rpc } from '../contract';
import { Engine } from '../host/engine';
import { waitFor } from '../host/__tests__/e2e/helpers/test-utils';
import { ScrollView } from '../host/preset/components/scroll-view';
import { readV1Linkage } from '../v1';
import { build } from './build';

const contract = defineKeelContract({
  version: '1.0.0',
  hostModules: {
    'host:clipboard': { writeText: rpc() },
    'host:service': { call: rpc(), tell: rpc(), uploadAttachment: rpc() },
  },
  guestExports: {},
});

let root: string;
let cwd: string;

beforeEach(() => {
  cwd = process.cwd();
  root = fs.mkdtempSync(path.join(os.tmpdir(), 'keel-v1-build-'));
  process.chdir(root);
});

afterEach(() => {
  process.chdir(cwd);
  fs.rmSync(root, { recursive: true, force: true });
});

async function buildGuest(source: string, minify: boolean) {
  await Bun.write('guest.tsx', source);
  await build({
    entry: 'guest.tsx',
    outfile: './workload/guest.js',
    v1CapabilityManifest: './workload/capabilities.json',
    contract,
    minify,
  });
}

test.each([false, true])('builds a v1 linkage and manifest (minify=%s)', async (minify) => {
  await buildGuest(
    `import { View, Text } from 'keel/guest';
    import { writeText } from 'host:clipboard';
    import { call as echo } from 'host:service/echo.v1.EchoService.Echo';
    export default function App() {
      const components = [View, Text];
      components.reverse();
      const [Label] = components;
      return <View onPress={() => { writeText({text: 'echo'}); echo({payloadBase64: ''}); }}>
        <Label>Echo</Label>
      </View>;
    }`,
    minify
  );
  const script = await Bun.file('workload/guest.js').text();
  const manifest = await Bun.file('workload/capabilities.json').json();
  expect(readV1Linkage(script).guestEntry).toBe('workload/guest.js');
  expect(manifest).toMatchObject({
    guestEntry: 'workload/guest.js',
    components: ['Text', 'View'],
    hostModules: {
      clipboard: { rpc: ['writeText'] },
      service: { rpc: ['echo.v1.EchoService.Echo'] },
    },
  });
  expect(script).not.toContain('require("host:service/');
});

test('a packaged v1 Guest requests a scroll through ScrollView props', async () => {
  await buildGuest(
    `import { useState } from 'react';
    import { ScrollView } from 'keel/guest';
    export default function App() {
      const [request, setRequest] = useState();
      return <ScrollView testID="conversation" scrollRequest={request} onContentSizeChange={() =>
        setRequest({ id: 1, target: 'end' })} />;
    }`,
    true
  );

  const engine = new Engine({ sandbox: 'node-vm', restrictToRegisteredComponents: true });
  engine.register({ ScrollView });
  const receiver = engine.createReceiver();
  const calls: unknown[] = [];
  let renderer: ReturnType<typeof create> | undefined;
  try {
    await engine.loadBundle(await Bun.file('workload/guest.js').text());
    await waitFor(() => receiver.findByTestId('conversation') !== undefined, 1500);
    const node = receiver.findByTestId('conversation')!;
    await act(async () => {
      renderer = create(receiver.render() as React.ReactElement, {
        createNodeMock: () => ({ scrollToEnd: (options: unknown) => { calls.push(options); } }),
      });
    });
    renderer!.root.findByType(NativeScrollView).props.onContentSizeChange(320, 600);
    await waitFor(() => node.props.scrollRequest?.target === 'end', 1500);
    await act(async () => { renderer!.update(receiver.render() as React.ReactElement); });
    expect(calls).toEqual([{ animated: false }]);
  } finally {
    await act(async () => { renderer?.unmount(); });
    await engine.destroy();
  }
});

test.each([
  'function mutate(a) { a[0] = Modal; } const arr = [View]; mutate(arr); const [C] = arr;',
  'const arr = [View, Modal]; const reverse = arr.reverse.bind(arr); reverse(); const [C] = arr;',
  'const bag = {arr: [View, Modal]}; bag.arr.reverse.call(bag.arr); const [C] = bag.arr;',
])('rejects a disallowed component acquisition through %s', async (body) => {
  for (const minify of [false, true]) {
    await expect(
      buildGuest(
        `import { View, Modal } from 'keel/guest'; ${body}
        export default function App() { return <C />; }`,
        minify
      )
    ).rejects.toThrow(/component_not_allowed.*Modal/);
    expect(await Bun.file('workload/capabilities.json').exists()).toBe(false);
  }
});

test('requires a fixed route for service calls', async () => {
  await expect(
    buildGuest(
      `import { call } from 'host:service';
      export default function App() { call({route: 'echo.v1.EchoService.Echo', payloadBase64: ''}); return null; }`,
      true
    )
  ).rejects.toThrow(/fixed route/);
});

test('builds the fixed uploadAttachment transport import from host:service', async () => {
  await buildGuest(
    `import { uploadAttachment } from 'host:service';
    export default function App() {
      uploadAttachment({ filename: 'report.pdf', base64: 'AAEC' });
      return null;
    }`,
    true
  );
  const script = await Bun.file('workload/guest.js').text();
  const manifest = await Bun.file('workload/capabilities.json').json();
  expect(manifest).toMatchObject({
    guestEntry: 'workload/guest.js',
    hostModules: {
      service: { rpc: ['uploadAttachment'] },
    },
  });
  expect(script).not.toContain('require("host:service")');
});

test('still rejects a raw tell import from host:service', async () => {
  await expect(
    buildGuest(
      `import { tell } from 'host:service';
      export default function App() { tell({route: 'echo.v1.EchoService.Echo', payloadBase64: ''}); return null; }`,
      true
    )
  ).rejects.toThrow(/fixed route or event/);
});

test('normalizes a macOS absolute alias to the same signed entry', async () => {
  // macOS exposes the physical /private/var temporary directory through /var.
  const physical = fs.realpathSync(root);
  const alias = physical.replace(/^\/private\/var\//, '/var/');
  await Bun.write('guest.tsx', "export default function App() { return null; }");
  await build({
    entry: 'guest.tsx',
    outfile: path.join(alias, 'workload', 'guest.js'),
    v1CapabilityManifest: 'workload/capabilities.json',
    contract,
    minify: false,
  });
  expect(readV1Linkage(await Bun.file('workload/guest.js').text()).guestEntry).toBe('workload/guest.js');
});

test('rejects a manifest and a Guest that resolve to the same file', async () => {
  fs.mkdirSync('workload');
  fs.symlinkSync('workload', 'alias');
  await Bun.write('guest.tsx', "export default function App() { return null; }");
  await expect(build({
    entry: 'guest.tsx',
    outfile: 'workload/guest.js',
    v1CapabilityManifest: 'alias/guest.js',
    contract,
    minify: false,
  })).rejects.toThrow(/different files/);
});

test.each(['globalThis.eval?.("1")', 'globalThis["eval"]("1")'])(
  'rejects an explicit dynamic code entry point in an unminified Guest',
  async (expression) => {
    await expect(buildGuest(
      `export default function App() { ${expression}; return null; }`,
      false
    )).rejects.toThrow(/dynamic code/);
  }
);

test('rejects a custom footer for v1 output', async () => {
  await Bun.write('guest.tsx', "export default function App() { return null; }");
  await expect(build({
    entry: 'guest.tsx',
    outfile: 'workload/guest.js',
    v1CapabilityManifest: 'workload/capabilities.json',
    contract,
    footer: 'footer.js',
    minify: false,
  })).rejects.toThrow(/default footer/);
});

test('a rejected rebuild leaves the last validated bundle and manifest intact', async () => {
  await buildGuest("export default function App() { return null; }", false);
  const bundle = await Bun.file('workload/guest.js').text();
  const manifest = await Bun.file('workload/capabilities.json').text();
  await expect(buildGuest(
    "import { Modal } from 'keel/guest'; export default function App() { return <Modal />; }",
    false
  )).rejects.toThrow(/component_not_allowed/);
  expect(await Bun.file('workload/guest.js').text()).toBe(bundle);
  expect(await Bun.file('workload/capabilities.json').text()).toBe(manifest);
});
