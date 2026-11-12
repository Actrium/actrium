import { afterEach, beforeEach, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { defineKeelContract, rpc } from '../contract';
import { readV1Linkage } from '../v1';
import { build } from './build';

const contract = defineKeelContract({
  version: '1.0.0',
  hostModules: {
    'host:clipboard': { writeText: rpc() },
    'host:service': { call: rpc() },
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
