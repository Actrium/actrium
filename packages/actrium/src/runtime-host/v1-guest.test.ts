import { expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHostModuleDispatch } from 'keel/contract';
import { Engine } from 'keel/host';
import { type V1Linkage, createV1CapabilityManifest, v1ServiceDispatchId, writeV1Bundle } from 'keel/v1';
import { contract } from './contract';
import { createHostModules } from './host-modules';
import { bindV1HostModules, verifyV1RuntimeGuest } from './v1-guest';

/** The actual CLI entrypoint, exercised in a separate process. */
const keelCli = fileURLToPath(new URL('../../../keel/src/cli/bin.ts', import.meta.url));

const route = 'echo.v1.EchoService.Echo';
const sourceModule = `host:service/${route}`;
const linkage: V1Linkage = {
  guestEntry: 'workload/guest.js',
  links: [['keel/guest', 'View'], ['host:clipboard', 'writeText'], [sourceModule, 'call']],
};
const manifest = createV1CapabilityManifest(linkage, contract);
const script = writeV1Bundle(linkage, 'module.exports = {};');
const declarations = ['service.call', 'host:clipboard.write'];

test('checks a signed manifest against the linkage before loading a Guest', () => {
  expect(verifyV1RuntimeGuest(script, JSON.stringify(manifest), declarations).ok).toBe(true);
  expect(
    verifyV1RuntimeGuest(script, JSON.stringify({ ...manifest, components: [] }), declarations)
  ).toMatchObject({ ok: false, code: 'capability_mismatch' });
  expect(verifyV1RuntimeGuest('module.exports = {};', JSON.stringify(manifest), declarations).ok).toBe(false);
});

test('exposes only linked Host methods and pins the route on the Host', async () => {
  const calls: string[] = [];
  const grants: string[] = [];
  const clipboard: string[] = [];
  const authorize = async (capability: string) => { grants.push(capability); };
  const hostModules = createHostModules({
    hasServiceCapability: true,
    callActor: async ({ route: sentRoute }) => {
      calls.push(sentRoute);
      return { success: true, payloadBase64: '' };
    },
    authorizeCapability: authorize,
    writeClipboardText: async (text) => { clipboard.push(text); },
  });
  const bound = bindV1HostModules(linkage, hostModules, authorize);
  const dispatch = createHostModuleDispatch(bound.contract, bound.hostModules);
  expect(Object.keys(dispatch).sort()).toEqual(['host:clipboard', v1ServiceDispatchId(sourceModule, 'call')]);
  expect(Object.getPrototypeOf(dispatch)).toBeNull();
  expect(Object.getPrototypeOf(dispatch['host:clipboard'])).toBeNull();
  await dispatch[v1ServiceDispatchId(sourceModule, 'call')].call({
    route: 'unlisted.v1.Route',
    payloadBase64: '',
  });
  await dispatch['host:clipboard'].writeText({ text: 'Echo' });
  expect(calls).toEqual([route]);
  expect(grants).toEqual(['service.call', 'host:clipboard.write']);
  expect(clipboard).toEqual(['Echo']);
});

test('rejects a fixed tell link while tell is unimplemented even on a linked call route', () => {
  const withTell: V1Linkage = { ...linkage, links: [...linkage.links, [sourceModule, 'tell']] };
  expect(verifyV1RuntimeGuest(writeV1Bundle(withTell, ''), JSON.stringify(manifest), declarations)).toMatchObject({
    ok: false,
    code: 'unknown_capability',
  });
});

test.each([false, true])('runs the built Guest through a scoped Engine (minify=%s)', async (minify) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'actrium-v1-guest-'));
  let engine: Engine | undefined;
  try {
    await Bun.write(path.join(root, 'guest.tsx'), `
      import { View } from 'keel/guest';
      import { writeText } from 'host:clipboard';
      import { call as echo } from 'host:service/${route}';
      void echo({ payloadBase64: '' }).then(() => writeText({ text: 'Echo received' }));
      export default function App() { return <><View>Hello</View>{'Standalone text'}</>; }
    `);
    const command = [
      Bun.argv[0], keelCli, 'build', 'guest.tsx',
      '-o', './workload/guest.js',
      '--v1-capability-manifest', './workload/capabilities.json',
      '--contract', fileURLToPath(new URL('./contract.ts', import.meta.url)),
      ...(minify ? [] : ['--no-minify']),
    ];
    const result = Bun.spawnSync(command, { cwd: root, stderr: 'pipe' });
    // Surface the CLI diagnostics if building the integration fixture failed.
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
    const bundle = await Bun.file(path.join(root, 'workload/guest.js')).text();
    const signedProfile = await Bun.file(path.join(root, 'workload/capabilities.json')).text();
    const verified = verifyV1RuntimeGuest(bundle, signedProfile, declarations);
    if (!verified.ok) throw new Error(verified.details.join(' '));
    expect(verified.manifest.components).toEqual(['View']);

    const calls: string[] = [];
    const clipboard = Promise.withResolvers<string>();
    const authorize = async () => {};
    const implementations = createHostModules({
      hasServiceCapability: true,
      callActor: async ({ route: sentRoute }) => {
        calls.push(sentRoute);
        return { success: true, payloadBase64: '' };
      },
      authorizeCapability: authorize,
      writeClipboardText: async (text) => clipboard.resolve(text),
    });
    // Stand in for the Host's native renderer without registering a Text component.
    const ProtocolText = () => null;
    engine = new Engine({
      sandbox: 'node-vm',
      ...bindV1HostModules(verified.linkage, implementations, authorize),
      restrictToRegisteredComponents: true,
      textNodeRenderer: ProtocolText,
      requireWhitelist: verified.manifest.imports.filter((id) => !id.startsWith('host:')),
    });
    engine.register({ View: () => null });
    const receiver = engine.createReceiver();
    await engine.loadBundle(bundle);
    expect(await clipboard.promise).toBe('Echo received');
    expect(calls).toEqual([route]);
    // Check rendered children, since protocol text nodes can exist without a renderer.
    expect(receiver.render()).toMatchObject({
      props: {
        children: [
          { props: { children: { type: ProtocolText, props: { children: 'Hello' } } } },
          { type: ProtocolText, props: { children: 'Standalone text' } },
        ],
      },
    });
    expect(receiver.applyBatch({
      version: 1,
      batchId: 99,
      operations: [
        { op: 'CREATE', id: 998, type: 'Modal', props: {} },
        { op: 'CREATE', id: 999, type: 'Text', props: {} },
      ],
    }).failed).toBe(2);
    expect(receiver.getStats().nodeCount).toBe(3);
  } finally {
    engine?.destroy();
    fs.rmSync(root, { recursive: true, force: true });
  }
});
