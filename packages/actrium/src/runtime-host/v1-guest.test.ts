import { expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHostModuleDispatch } from 'keel/contract';
import { Engine } from 'keel/host';
import { type V1Linkage, createV1CapabilityManifest, v1ServiceDispatchId, writeV1Bundle } from 'keel/v1';
import { contract } from './contract';
import { GuestTTS } from './guest-tts';
import { createHostModules } from './host-modules';
import { createTestServiceEventSource } from './service-events.test-utils';
import type { AttachmentUploadManager } from './attachment-upload';
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

test('requires the package declaration for a linked file import picker', () => {
  const linked: V1Linkage = { ...linkage, links: [['host:file-import', 'pick']] };
  const profile = JSON.stringify(createV1CapabilityManifest(linked, contract));
  const bundle = writeV1Bundle(linked, 'module.exports = {};');
  expect(verifyV1RuntimeGuest(bundle, profile, ['host:file-import.pick']).ok).toBe(true);
  expect(verifyV1RuntimeGuest(bundle, profile, []).ok).toBe(false);
});

test('requires the package declaration for linked speech recognition', () => {
  const linked: V1Linkage = { ...linkage, links: [['host:speech', 'recognize']] };
  const profile = JSON.stringify(createV1CapabilityManifest(linked, contract));
  const bundle = writeV1Bundle(linked, 'module.exports = {};');
  expect(verifyV1RuntimeGuest(bundle, profile, ['host:speech.recognize']).ok).toBe(true);
  expect(verifyV1RuntimeGuest(bundle, profile, ['host:microphone.record']).ok).toBe(false);
});

test('verifies and binds the fixed uploadAttachment transport', async () => {
  const linked: V1Linkage = {
    ...linkage,
    links: [...linkage.links, ['host:service', 'uploadAttachment']],
  };
  const profile = JSON.stringify(createV1CapabilityManifest(linked, contract));
  const bundle = writeV1Bundle(linked, 'module.exports = {};');
  // The manifest records the fixed transport next to the business route.
  expect(verifyV1RuntimeGuest(bundle, profile, [
    'service.call',
    'service.attachmentUpload',
    'host:clipboard.write',
  ]).ok).toBe(true);
  // The transport needs its own declaration, not just service.call.
  expect(
    verifyV1RuntimeGuest(bundle, profile, ['service.call', 'host:clipboard.write']).ok
  ).toBe(false);

  const uploads: unknown[] = [];
  const grants: string[] = [];
  const authorize = async (capability: string) => {
    grants.push(capability);
  };
  const uploader = {
    available: true,
    upload: async (input: unknown) => {
      uploads.push(input);
      return { attachmentId: 'a-1' };
    },
    dispose: () => {},
  };
  const implementations = createHostModules({
    hasServiceCapability: true,
    hasServiceOnCapability: true,
    callActor: async () => ({ success: true, payloadBase64: '' }),
    authorizeCapability: authorize,
    writeClipboardText: async () => {},
    attachmentUploader: uploader as unknown as AttachmentUploadManager,
  });
  const bound = bindV1HostModules(linked, implementations, authorize);
  const dispatch = createHostModuleDispatch(bound.contract, bound.hostModules);
  expect(Object.keys(dispatch).sort()).toEqual([
    'host:clipboard',
    'host:service',
    v1ServiceDispatchId(sourceModule, 'call'),
  ]);
  await dispatch['host:service'].uploadAttachment({
    filename: 'a.txt',
    base64: 'AAEC',
    type: 'image',
  });
  expect(grants).toContain('service.attachmentUpload');
  expect(uploads).toEqual([{ filename: 'a.txt', base64: 'AAEC', type: 'image' }]);
});

test('exposes only linked Host methods and pins the route on the Host', async () => {
  const calls: string[] = [];
  const grants: string[] = [];
  const clipboard: string[] = [];
  const authorize = async (capability: string) => { grants.push(capability); };
  const hostModules = createHostModules({
    hasServiceCapability: true,
    hasServiceOnCapability: true,
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

test('verifies and binds a fixed event subscription', async () => {
  const eventLinkage: V1Linkage = {
    guestEntry: 'workload/guest.js',
    links: [[sourceModule, 'on']],
  };
  const eventManifest = createV1CapabilityManifest(eventLinkage, contract);
  const bundle = writeV1Bundle(eventLinkage, 'module.exports = {};');
  const verified = verifyV1RuntimeGuest(
    bundle,
    JSON.stringify(eventManifest),
    ['service.on']
  );
  expect(verified).toMatchObject({ ok: true });
  if (!verified.ok) throw new Error(verified.details.join(' '));

  const { source, emit, subscriberCount } = createTestServiceEventSource([
    route,
    'echo.v1.EchoService.Other',
  ]);
  const authorize = async () => {};
  const implementations = createHostModules({
    hasServiceCapability: true,
    hasServiceOnCapability: true,
    callActor: async () => ({ success: true, payloadBase64: '' }),
    serviceEventSource: source,
    authorizeCapability: authorize,
    writeClipboardText: async () => {},
  });
  const bound = bindV1HostModules(verified.linkage, implementations, authorize);
  const dispatch = createHostModuleDispatch(bound.contract, bound.hostModules);
  const id = v1ServiceDispatchId(sourceModule, 'on');
  const received: { event: string; payloadBase64: string }[] = [];
  const on = dispatch[id].on as (
    handler: (event: { event: string; payloadBase64: string }) => void
  ) => () => void;
  const unsubscribe = on((event) => received.push(event));
  await Promise.resolve();

  expect(subscriberCount()).toBe(1);
  emit({ event: 'echo.v1.EchoService.Other', payloadBase64: 'BAUG' });
  emit({ event: route, payloadBase64: 'AAEC' });
  expect(received).toEqual([{ event: route, payloadBase64: 'AAEC' }]);

  unsubscribe?.();
  expect(subscriberCount()).toBe(0);
});

test('verifies and binds a call and on link with the same business name', () => {
  const sharedLinkage: V1Linkage = {
    guestEntry: 'workload/guest.js',
    links: [
      [sourceModule, 'call'],
      [sourceModule, 'on'],
    ],
  };
  const sharedManifest = createV1CapabilityManifest(sharedLinkage, contract);
  const bundle = writeV1Bundle(sharedLinkage, 'module.exports = {};');
  const verified = verifyV1RuntimeGuest(
    bundle,
    JSON.stringify(sharedManifest),
    ['service.call', 'service.on']
  );
  expect(verified).toMatchObject({ ok: true });
  if (!verified.ok) throw new Error(verified.details.join(' '));

  const authorize = async () => {};
  const bound = bindV1HostModules(
    verified.linkage,
    createHostModules({
      hasServiceCapability: true,
      hasServiceOnCapability: true,
      callActor: async () => ({ success: true, payloadBase64: '' }),
      serviceEventSource: createTestServiceEventSource([route]).source,
      authorizeCapability: authorize,
      writeClipboardText: async () => {},
    }),
    authorize
  );
  const id = v1ServiceDispatchId(sourceModule, 'call');
  expect(bound.contract.hostModules[id]?.call).toBeDefined();
  expect(bound.contract.hostModules[id]?.on).toBeDefined();
  expect(bound.hostModules[id].call).toBeDefined();
  expect(bound.hostModules[id].on).toBeDefined();
});

test('rejects a fixed tell link while tell is unimplemented even on a linked call route', () => {
  const withTell: V1Linkage = { ...linkage, links: [...linkage.links, [sourceModule, 'tell']] };
  expect(verifyV1RuntimeGuest(writeV1Bundle(withTell, ''), JSON.stringify(manifest), declarations)).toMatchObject({
    ok: false,
    code: 'unknown_capability',
  });
});

test.each([false, true])('runs the built Guest through a scoped Engine (minify=%s)', async (minify) => {
  const mandarinText = '\u4f60\u597d';
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'actrium-v1-guest-'));
  let engine: Engine | undefined;
  try {
    await Bun.write(path.join(root, 'guest.tsx'), `
      import { Menu, Select, Supramark, Svg, View } from 'keel/guest';
      import { writeText } from 'host:clipboard';
      import { call as echo } from 'host:service/${route}';
      import { speak } from 'host:tts';
      void echo({ payloadBase64: '' })
        .then(() => speak({ text: ${JSON.stringify(mandarinText)}, locale: 'zh-CN', voice: 'male' }))
        .then(({ status }) => writeText({ text: 'Echo received, speech ' + status }));
      export default function App() {
        return <>
          <View onMouseEnter={() => writeText({ text: 'hover-in' })} onMouseLeave={() => writeText({ text: 'hover-out' })}>
            Hello
          </View>
          {'Standalone text'}<Supramark markdown="# Hello" />
          <Select value="zh" options={[{ value: 'zh', label: 'Chinese' }]} onValueChange={(text) => writeText({ text })} />
          <Menu title="More" items={[{ value: 'rename', label: 'Rename' }]} onSelect={(text) => writeText({ text })} />
          <Svg width={48} height={32} viewBox="0 0 24 16" paths={[{ d: 'M2 2 L22 14', stroke: '#123456' }]} />
        </>;
      }
    `);
    const command = [
      Bun.argv[0], keelCli, 'build', 'guest.tsx',
      '-o', './workload/guest.js',
      '--v1-capability-manifest', './workload/capabilities.json',
      '--contract', fileURLToPath(new URL('./contract.ts', import.meta.url)),
      ...(minify ? [] : ['--no-minify']),
    ];
    const result = Bun.spawnSync(command, { cwd: root, stdout: 'pipe', stderr: 'pipe' });
    // Surface the CLI diagnostics if building the integration fixture failed.
    if (result.exitCode !== 0) throw new Error(result.stderr.toString() || result.stdout.toString());
    const bundle = await Bun.file(path.join(root, 'workload/guest.js')).text();
    const signedProfile = await Bun.file(path.join(root, 'workload/capabilities.json')).text();
    const guestDeclarations = [...declarations, 'host:tts.speak'];
    const verified = verifyV1RuntimeGuest(bundle, signedProfile, guestDeclarations);
    if (!verified.ok) throw new Error(verified.details.join(' '));
    expect(verified.manifest.components).toEqual(['Menu', 'Select', 'Supramark', 'Svg', 'View']);
    const profileWithoutMenus = JSON.stringify({ ...verified.manifest, components: ['Supramark', 'View'] });
    expect(verifyV1RuntimeGuest(bundle, profileWithoutMenus, guestDeclarations)).toMatchObject({
      ok: false, code: 'capability_mismatch',
    });
    const profileWithoutSvg = JSON.stringify({ ...verified.manifest, components: ['Menu', 'Select', 'Supramark', 'View'] });
    expect(verifyV1RuntimeGuest(bundle, profileWithoutSvg, guestDeclarations)).toMatchObject({
      ok: false, code: 'capability_mismatch',
    });

    const calls: string[] = [];
    const spoken: unknown[] = [];
    const clipboard = Promise.withResolvers<string>();
    const selections: string[] = [];
    const authorize = async () => {};
    const implementations = createHostModules({
      hasServiceCapability: true,
      hasServiceOnCapability: true,
      callActor: async ({ route: sentRoute }) => {
        calls.push(sentRoute);
        return { success: true, payloadBase64: '' };
      },
      authorizeCapability: authorize,
      writeClipboardText: async (text) => {
        selections.push(text);
        clipboard.resolve(text);
      },
      tts: new GuestTTS({
        authorize,
        onRevoked: () => () => {},
        onCleanupError: (error) => { throw error; },
        bridge: {
          speak: async (_sessionId, text, locale, voice) => {
            spoken.push({ text, locale, voice });
            return { status: 'finished' };
          },
          stop: async () => {},
        },
      }),
    });
    // Stand in for the Host's native renderer without registering a Text component.
    const ProtocolText = () => null;
    const HostSupramark = () => null;
    const HostSelect = () => null;
    const HostMenu = () => null;
    const HostSvg = () => null;
    engine = new Engine({
      sandbox: 'node-vm',
      ...bindV1HostModules(verified.linkage, implementations, authorize),
      restrictToRegisteredComponents: true,
      textNodeRenderer: ProtocolText,
      requireWhitelist: verified.manifest.imports.filter((id) => !id.startsWith('host:')),
    });
    engine.register({ Supramark: HostSupramark, Select: HostSelect, Menu: HostMenu, Svg: HostSvg, View: () => null });
    const receiver = engine.createReceiver();
    await engine.loadBundle(bundle);
    expect(await clipboard.promise).toBe('Echo received, speech finished');
    expect(spoken).toEqual([{ text: mandarinText, locale: 'zh-CN', voice: 'male' }]);
    expect(calls).toEqual([route]);
    // Check rendered children, since protocol text nodes can exist without a renderer.
    type GuestCallbacks = {
      onMouseEnter?: () => Promise<void>;
      onMouseLeave?: () => Promise<void>;
      onValueChange?: (value: string) => Promise<void>;
      onSelect?: (value: string) => Promise<void>;
    };
    const rendered = receiver.render() as unknown as { props: { children: Array<{ props: GuestCallbacks }> } };
    expect(rendered).toMatchObject({
      props: {
        children: [
          { props: { children: { type: ProtocolText, props: { children: 'Hello' } } } },
          { type: ProtocolText, props: { children: 'Standalone text' } },
          { type: HostSupramark, props: { markdown: '# Hello' } },
          { type: HostSelect, props: { value: 'zh', options: [{ value: 'zh', label: 'Chinese' }] } },
          { type: HostMenu, props: { title: 'More', items: [{ value: 'rename', label: 'Rename' }] } },
          { type: HostSvg, props: { width: 48, height: 32, viewBox: '0 0 24 16', paths: [{ d: 'M2 2 L22 14', stroke: '#123456' }] } },
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
    expect(receiver.getStats().nodeCount).toBe(7);
    const select = rendered.props.children[3]?.props.onValueChange;
    const choose = rendered.props.children[4]?.props.onSelect;
    expect(select).toBeFunction();
    expect(choose).toBeFunction();
    await select?.('zh');
    await choose?.('rename');
    const hover = rendered.props.children[0]?.props;
    expect(hover?.onMouseEnter).toBeFunction();
    expect(hover?.onMouseLeave).toBeFunction();
    await hover?.onMouseEnter?.();
    await hover?.onMouseLeave?.();
    expect(selections).toEqual(['Echo received, speech finished', 'zh', 'rename', 'hover-in', 'hover-out']);
  } finally {
    engine?.destroy();
    fs.rmSync(root, { recursive: true, force: true });
  }
});
