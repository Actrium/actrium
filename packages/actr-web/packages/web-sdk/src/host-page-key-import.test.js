import { readFileSync } from 'node:fs';
import { MessageChannel } from 'node:worker_threads';
import { runInNewContext } from 'node:vm';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Execute the bundled CLI page, rather than an SDK-only config constructor.
// The worker double starts with no stored key and acknowledges registration
// only when the test requests it. The WASM key parser/persistence is covered
// separately by sw-host's authorized_realm_key_survives_host_restart_and_is_scoped test.
const html = readFileSync(
  new URL('../../../../../crates/actr-cli/assets/web-runtime/actr-host.html', import.meta.url),
  'utf8',
);
const source = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const config = {
  realm: 'browser@example.com',
  issuer_endpoint: 'https://provider.example/issuer',
  signaling_url: 'wss://provider.example/signaling',
  package: { full_type: 'apps.example/echo@1.0.0' },
  package_url: '/packages/echo.actr',
  runtime_wasm_url: '/packages/actr_sw_host_bg.wasm',
  trust: [{ kind: 'static', pubkey_b64: 'public-verification-material' }],
};
const key = {
  realm: config.realm,
  issuer: config.issuer_endpoint,
  provider_id: 'provider.example',
  key_id: 'disposable-client-key',
  private_key: 'disposable-private-canary',
  public_key: 'disposable-public-canary',
};
const cleanups = [];
afterEach(() => { for (const cleanup of cleanups.splice(0)) cleanup(); });

class Element {
  constructor(id) {
    this.id = id;
    this.disabled = ['startBtn', 'sendBtn'].includes(id);
    this.children = [];
    this.files = [];
    this.listeners = new Map();
    this.textContent = '';
  }
  set value(value) { if (value === '') this.files = []; }
  addEventListener(type, listener) { this.listeners.set(type, listener); }
  async dispatch(type) { return this.listeners.get(type)?.({ target: this }); }
  appendChild(child) { this.children.push(child); }
  removeChild(child) { this.children.splice(this.children.indexOf(child), 1); }
}

async function page() {
  const elements = new Map([...html.matchAll(/id="([^"]+)"/g)].map(([, id]) => [id, new Element(id)]));
  const imports = [];
  const requests = [];
  const logs = [];
  const worker = {
    postMessage(message) {
      if (message.type === 'DOM_PORT_INIT') {
        imports.push({ config: structuredClone(message.runtimeConfig), port: message.port });
      }
    },
  };
  const context = {
    document: { getElementById: id => elements.get(id), createElement: () => new Element('log') },
    window: { addEventListener() {} },
    navigator: { serviceWorker: {
      controller: worker,
      ready: Promise.resolve(),
      register: async () => ({ active: worker, update: async () => {} }),
      addEventListener() {},
    } },
    fetch: async (url, options) => {
      requests.push({ url, options });
      return { ok: true, json: async () => structuredClone(config) };
    },
    MessageChannel: class extends MessageChannel {
      constructor() {
        super();
        cleanups.push(() => { this.port1.close(); this.port2.close(); });
      }
    },
    setTimeout, clearTimeout, setInterval: () => 0,
    console: { log: (...args) => logs.push(args), error: (...args) => logs.push(args) },
    TextEncoder, TextDecoder,
  };
  runInNewContext(source, context);
  await vi.waitFor(() => expect(elements.get('realmKeyFile').listeners.has('change')).toBe(true));
  return {
    elements, imports, requests, logs,
    turnCredential: () => runInNewContext('_coordinator.turnCred', context),
    async choose(text) {
      elements.get('realmKeyFile').files = [{ text: async () => text }];
      await elements.get('realmKeyFile').dispatch('change');
    },
    start: () => elements.get('startBtn').dispatch('click'),
  };
}

describe('bundled actr run --web Realm key provisioning', () => {
  it('imports a local key on first startup and waits for the worker acknowledgement', async () => {
    const host = await page();
    expect(host.imports).toHaveLength(0);
    expect(host.elements.get('startBtn').disabled).toBe(true);
    await host.choose(JSON.stringify(key));
    const started = host.start();
    await vi.waitFor(() => expect(host.imports).toHaveLength(1));
    expect(host.imports[0].config.realm_client_key).toEqual(key);
    expect(host.elements.get('sendBtn').disabled).toBe(true);
    expect(host.elements.get('status').textContent).toContain('Starting');
    host.imports[0].port.postMessage({
      type: 'update_turn_credential', payload: { username: 'test-user', password: 'disposable-turn' },
    });
    host.imports[0].port.postMessage({ type: 'runtime_ready' });
    await started;
    expect(host.turnCredential()).toEqual({ username: 'test-user', credential: 'disposable-turn' });
    expect(host.elements.get('sendBtn').disabled).toBe(false);
    expect(host.elements.get('realmKeyForm').hidden).toBe(true);
    expect(host.elements.get('realmKeyFile').files).toHaveLength(0);
    expect(host.requests).toEqual([{ url: '/actr-runtime-config.json', options: undefined }]);
    expect(JSON.stringify(host.logs)).not.toContain(key.private_key);
  });

  it.each([
    ['invalid JSON', '{"private_key":"disposable-private-canary"'],
    ['Publisher keychain', JSON.stringify({ private_key: key.private_key })],
    ['another Realm', JSON.stringify({ ...key, realm: 'other@example.com' })],
    ['another Issuer', JSON.stringify({ ...key, issuer: 'https://other.example/issuer' })],
  ])('refuses %s before connecting the worker', async (_name, text) => {
    const host = await page();
    await host.choose(text);
    await host.start();
    expect(host.imports).toHaveLength(0);
    expect(host.elements.get('status').className).toBe('status error');
    expect(host.elements.get('sendBtn').disabled).toBe(true);
    expect(JSON.stringify(host.logs)).not.toContain(key.private_key);
    // A rejected file does not prevent selecting the correct key.
    await host.choose(JSON.stringify(key));
    expect(host.elements.get('startBtn').disabled).toBe(false);
  });

  it('keeps the Actor unavailable when the worker rejects the key', async () => {
    const host = await page();
    await host.choose(JSON.stringify(key));
    const started = host.start();
    await vi.waitFor(() => expect(host.imports).toHaveLength(1));
    host.imports[0].port.postMessage({ type: 'runtime_error', error: 'client_revoked' });
    await started;
    expect(host.elements.get('status').textContent).toContain('client_revoked');
    expect(host.elements.get('sendBtn').disabled).toBe(true);
    expect(host.elements.get('realmKeyForm').hidden).not.toBe(true);
  });
});
