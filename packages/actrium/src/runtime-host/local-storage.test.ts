import { expect, test } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHostModuleDispatch } from 'keel/contract';
import { Engine } from 'keel/host';
import { createV1CapabilityManifest, type V1Linkage, writeV1Bundle } from 'keel/v1';
import { contract } from './contract';
import { createHostModules } from './host-modules';
import { bindNativeLocalStorage } from './local-storage';
import { bindV1HostModules, verifyV1RuntimeGuest } from './v1-guest';

const moduleId = 'host:local-storage';
const declarations = ['host:local-storage.getItem'];

test('native storage JSON framing preserves NUL and Unicode strings', async () => {
  const key = 'theme\0\u00e9';
  const value = 'dark\0\u00e9\u{1f332}';
  const data = new Map<string, string>();
  const call = bindNativeLocalStorage({
    perform: async (tabId, documentId, operation, json) => {
      expect([tabId, documentId]).toEqual(['tab', 'document']);
      expect(json).not.toContain('\0');
      const input = JSON.parse(json) as { key: string; value?: string };
      if (operation === 'setItem' && input.value !== undefined) data.set(input.key, input.value);
      if (operation === 'removeItem') data.delete(input.key);
      return JSON.stringify(operation === 'getItem' ? data.get(input.key) ?? null : null);
    },
  }, 'tab', 'document');
  if (!call) throw new Error('The test bridge did not bind.');
  await call('setItem', { key: 'theme', value: 'plain' });
  await call('setItem', { key, value });
  expect(await call('getItem', { key })).toBe(value);
  expect(await call('getItem', { key: 'theme' })).toBe('plain');
  await call('removeItem', { key });
  expect(await call('getItem', { key })).toBeNull();
});

test('native storage rejects a response with an invalid JSON value type', async () => {
  const call = bindNativeLocalStorage({ perform: async () => '42' }, 'tab', 'document');
  if (!call) throw new Error('The test bridge did not bind.');
  await expect(call('getItem', { key: 'theme' })).rejects.toThrow('Invalid local storage result.');
});

test('a read-only package can only bind the declared storage method', async () => {
  const linkage: V1Linkage = {
    guestEntry: 'workload/guest.js',
    links: [[moduleId, 'getItem']],
  };
  const manifest = JSON.stringify(createV1CapabilityManifest(linkage, contract));
  const bundle = writeV1Bundle(linkage, 'module.exports = {};');
  expect(verifyV1RuntimeGuest(bundle, manifest, [])).toMatchObject({ ok: false });
  expect(verifyV1RuntimeGuest(bundle, manifest, declarations)).toMatchObject({ ok: true });

  const checked: string[] = [];
  const authorizeCapability = async (name: string) => { checked.push(name); };
  const implementations = createHostModules({
    hasServiceCapability: false, hasServiceOnCapability: false,
    callActor: async () => ({ success: false }),
    authorizeCapability,
    writeClipboardText: async () => {},
    localStorage: async (operation, input) => {
      expect(operation).toBe('getItem');
      expect(input).toEqual({ key: 'theme' });
      return null;
    },
  });
  const bound = bindV1HostModules(linkage, implementations, authorizeCapability);
  const dispatch = createHostModuleDispatch(bound.contract, bound.hostModules);
  expect(Object.keys(dispatch[moduleId])).toEqual(['getItem']);
  await expect(dispatch[moduleId].getItem({ key: 'theme' })).resolves.toBeNull();
  expect(checked).toEqual(declarations);
});

test('storage validates inputs and authorizes each method before calling the backend', async () => {
  const sent: string[] = [];
  const implementations = createHostModules({
    hasServiceCapability: false, hasServiceOnCapability: false,
    callActor: async () => ({ success: false }),
    writeClipboardText: async () => {},
    authorizeCapability: async (name) => {
      if (name === 'host:local-storage.removeItem') throw new Error('denied');
    },
    localStorage: async (operation) => {
      sent.push(operation);
      return null;
    },
  });
  const methods = createHostModuleDispatch(contract, implementations)[moduleId];
  expect(() => methods.setItem({ key: 'theme', value: 42 })).toThrow('value');
  await expect(methods.removeItem({ key: 'theme' })).rejects.toThrow('denied');
  await methods.setItem({ key: '', value: '' });
  expect(sent).toEqual(['setItem']);
});

test.each([
  ['ASCII', 'a', 1],
  ['two-byte characters', '\u00e9', 2],
  ['three-byte characters', '\u96ea', 3],
  ['emoji', '\u{1f332}', 4],
] as const)('storage enforces UTF-8 byte limits for %s', async (_, character, bytes) => {
  const methods = createHostModuleDispatch(contract, createHostModules({
    hasServiceCapability: false, hasServiceOnCapability: false,
    callActor: async () => ({ success: false }),
    writeClipboardText: async () => {},
    authorizeCapability: async () => {},
    localStorage: async () => null,
  }))[moduleId];
  const key = character.repeat(Math.floor(1024 / bytes)) + 'a'.repeat(1024 % bytes);
  const value = character.repeat(Math.floor(65536 / bytes)) + 'a'.repeat(65536 % bytes);

  await expect(methods.getItem({ key })).resolves.toBeNull();
  await methods.removeItem({ key });
  await methods.setItem({ key, value });
  expect(() => methods.getItem({ key: key + 'a' })).toThrow('key');
  expect(() => methods.removeItem({ key: key + 'a' })).toThrow('key');
  expect(() => methods.setItem({ key: key + 'a', value: '' })).toThrow('key');
  expect(() => methods.setItem({ key, value: value + 'a' })).toThrow('value');
});

test.each(['\ud800', '\udc00'])('storage rejects unpaired UTF-16 surrogates', (surrogate) => {
  const methods = createHostModuleDispatch(contract, createHostModules({
    hasServiceCapability: false, hasServiceOnCapability: false,
    callActor: async () => ({ success: false }),
    writeClipboardText: async () => {},
    authorizeCapability: async () => {},
    localStorage: async () => { throw new Error('Invalid input reached storage.'); },
  }))[moduleId];
  expect(() => methods.getItem({ key: surrogate })).toThrow('valid Unicode');
  expect(() => methods.removeItem({ key: surrogate })).toThrow('valid Unicode');
  expect(() => methods.setItem({ key: surrogate, value: 'value' })).toThrow('valid Unicode');
  expect(() => methods.setItem({ key: 'key', value: 'prefix' + surrogate })).toThrow('valid Unicode');
});

test('the v1 CLI and Guest Engine round-trip all three named storage methods', async () => {
  const root = mkdtempSync(join(tmpdir(), 'actrium-local-storage-'));
  let engine: Engine | undefined;
  try {
    await Bun.write(join(root, 'guest.tsx'), [
      "import { getItem, setItem, removeItem } from 'host:local-storage';",
      'void (async () => {',
      "  await setItem({ key: 'theme', value: 'dark' });",
      "  const theme = await getItem({ key: 'theme' });",
      "  await removeItem({ key: 'theme' });",
      "  const missing = await getItem({ key: 'theme' });",
      "  await setItem({ key: 'report', value: JSON.stringify([theme, missing]) });",
      '})();',
      'export default function App() { return null; }',
    ].join('\n'));
    const command = [
      Bun.argv[0], fileURLToPath(new URL('../../../keel/src/cli/bin.ts', import.meta.url)),
      'build', 'guest.tsx', '-o', './workload/guest.js',
      '--v1-capability-manifest', './workload/capabilities.json',
      '--contract', fileURLToPath(new URL('./contract.ts', import.meta.url)),
    ];
    const build = Bun.spawnSync(command, { cwd: root, stderr: 'pipe' });
    if (build.exitCode !== 0) throw new Error(build.stderr.toString());
    const bundle = await Bun.file(join(root, 'workload/guest.js')).text();
    const manifest = await Bun.file(join(root, 'workload/capabilities.json')).text();
    const allowed = [...declarations, 'host:local-storage.setItem', 'host:local-storage.removeItem'];
    const verified = verifyV1RuntimeGuest(bundle, manifest, allowed);
    if (!verified.ok) throw new Error(verified.details.join(' '));

    const data = new Map<string, string>();
    const report = Promise.withResolvers<string>();
    const authorizeCapability = async () => {};
    const implementations = createHostModules({
      hasServiceCapability: false, hasServiceOnCapability: false,
      callActor: async () => ({ success: false }),
      writeClipboardText: async () => {},
      authorizeCapability,
      localStorage: async (operation, input) => {
        if (operation === 'getItem') return data.get(input.key) ?? null;
        if (operation === 'removeItem') data.delete(input.key);
        if (operation === 'setItem' && 'value' in input) {
          data.set(input.key, input.value);
          if (input.key === 'report') report.resolve(input.value);
        }
        return null;
      },
    });
    engine = new Engine({
      sandbox: 'node-vm',
      ...bindV1HostModules(verified.linkage, implementations, authorizeCapability),
    });
    engine.createReceiver();
    await engine.loadBundle(bundle);
    expect(await report.promise).toBe('["dark",null]');
  } finally {
    engine?.destroy();
    rmSync(root, { recursive: true, force: true });
  }
});
