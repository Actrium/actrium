import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { describe, expect, it, vi } from 'vitest';

const config = {
  realm: 'browser@example.com',
  issuer_endpoint: 'https://provider.example/issuer',
  signaling_url: 'wss://provider.example/signaling',
  client_actr_type: 'apps.example/echo@1.0.0',
  package_url: '/echo.actr',
  runtime_wasm_url: '/actr_sw_host_bg.wasm',
  trust: [],
};
const imported = (keyId) => ({
  ...config,
  realm_client_key: { key_id: keyId, private_key: 'disposable-import-canary' },
});
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
};

// Execute the complete shipped classic worker, including its real message
// handler, bootstrap and cleanup paths. Only the external WASM/package bytes
// and WASM registration boundary are doubles; no worker function is replaced.
function worker(sourceUrl, register, restoredKeyId = 'key-a') {
  const listeners = new Map();
  const windows = new Map();
  const attempts = [];
  let storedKeyId = restoredKeyId;
  const wasm = Object.assign(
    vi.fn(async () => {}),
    {
      init_global: vi.fn(),
      install_dependency_lock: vi.fn(),
      register_guest_workload: vi.fn(),
      verify_and_extract_actr_package: vi.fn(() => ({
        manifest_json: JSON.stringify({
          publisher: 'apps.example',
          name: 'echo',
          version: '1.0.0',
        }),
        glue_js: 'let wasm_bindgen = self.fixtureGuest;',
        binary: new Uint8Array(),
      })),
      register_client: vi.fn(async (id, runtimeConfig) => {
        attempts.push({ id, config: structuredClone(runtimeConfig) });
        storedKeyId = runtimeConfig.realm_client_key?.key_id ?? storedKeyId;
        const keyId = storedKeyId;
        await register(id, runtimeConfig);
        return keyId;
      }),
      unregister_client: vi.fn(async () => {}),
    }
  );
  const context = {
    fixtureWasm: wasm,
    fixtureGuest: Object.assign(async () => {}, { dispatch: async () => ({ Ok: [] }) }),
    console: Object.fromEntries(['info', 'warn', 'error', 'log'].map((level) => [level, vi.fn()])),
    location: { href: 'https://browser.example/actor.sw.js' },
    addEventListener: (type, listener) => listeners.set(type, listener),
    clients: { matchAll: async () => [...windows.values()] },
    setTimeout: () => 1,
    clearTimeout() {},
    WebSocket: class {
      close() {}
    },
    fetch: vi.fn(async (url) => {
      if (url === '/actr_sw_host.js') {
        return { ok: true, text: async () => 'let wasm_bindgen = self.fixtureWasm;' };
      }
      if (url === '/echo.actr') return { ok: true, arrayBuffer: async () => new ArrayBuffer(0) };
      throw new Error(`Unexpected fixture fetch: ${url}`);
    }),
    Uint8Array,
  };
  context.self = context;
  runInNewContext(readFileSync(sourceUrl, 'utf8'), context);
  const message = (data) => listeners.get('message')({ data });
  return {
    wasm,
    attempts,
    fetch: context.fetch,
    state: () =>
      runInNewContext(
        `({
      keyId: RUNTIME_CLIENT_KEY_ID,
      privateKey: RUNTIME_CONFIG?.realm_client_key,
      clients: [...clientPorts.keys()],
      browsers: [...browserToSwClient.values()],
    })`,
        context
      ),
    async init(id, runtimeConfig, browserId = 'browser-tab') {
      const source = { id: browserId, postMessage: vi.fn() };
      windows.set(browserId, source);
      const port = { postMessage: vi.fn(), close: vi.fn(), start: vi.fn() };
      await listeners.get('message')({
        data: { type: 'DOM_PORT_INIT', clientId: id, runtimeConfig, port },
        source,
      });
      return {
        id,
        port,
        messages: () => port.postMessage.mock.calls.map(([data]) => data),
        portMessage: (data) => port.onmessage({ data }),
      };
    },
    unregister: (id) => message({ type: 'CLIENT_UNREGISTER', clientId: id }),
    stale: async (browserId) => {
      windows.delete(browserId);
      await runInNewContext('cleanupStaleClients()', context);
    },
  };
}

async function expectMessage(client, type) {
  await vi.waitFor(() =>
    expect(client.messages().some((message) => message.type === type)).toBe(true)
  );
}

describe.each([
  ['SDK worker', new URL('./actor.sw.js', import.meta.url)],
  [
    'CLI embedded worker',
    new URL('../../../../../crates/actr-cli/assets/web-runtime/actor.sw.js', import.meta.url),
  ],
])('%s registration lifecycle', (_name, source) => {
  it('retries an authorized B after A is rejected and its failed client unregisters', async () => {
    const runtime = worker(source, async (_id, cfg) => {
      if (cfg.realm_client_key?.key_id === 'key-a') throw new Error('client_revoked');
    });
    const a = await runtime.init('failed-a', imported('key-a'));
    await expectMessage(a, 'runtime_error');
    await runtime.unregister(a.id);
    const b = await runtime.init('authorized-b', imported('key-b'));
    await expectMessage(b, 'runtime_ready');
    expect(runtime.attempts.map((attempt) => attempt.config.realm_client_key.key_id)).toEqual([
      'key-a',
      'key-b',
    ]);
    expect(runtime.wasm).toHaveBeenCalledTimes(1); // Same verified workload and worker.
    // A delayed cleanup for A must not clear the replacement's binding.
    await runtime.unregister(a.id);
    expect(runtime.state().keyId).toBe('key-b');
    expect(runtime.state().clients).toEqual(['authorized-b']);
  });

  it('discards rejected import material even if the page never unregisters', async () => {
    const runtime = worker(source, async () => {
      throw new Error('client_revoked');
    });
    const a = await runtime.init('failed-a', imported('key-a'));
    await expectMessage(a, 'runtime_error');
    expect(runtime.state()).toEqual({
      keyId: null,
      privateKey: undefined,
      clients: [],
      browsers: [],
    });
  });

  it('keeps a successful client bound when a peer registration fails', async () => {
    const runtime = worker(source, async (id) => {
      if (id === 'failed-peer') throw new Error('registration_refused');
    });
    const live = await runtime.init('live', imported('key-a'), 'live-tab');
    await expectMessage(live, 'runtime_ready');
    const failed = await runtime.init('failed-peer', imported('key-a'), 'failed-tab');
    await expectMessage(failed, 'runtime_error');
    await runtime.unregister(failed.id);
    const other = await runtime.init('other-key', imported('key-b'), 'other-tab');
    await expectMessage(other, 'runtime_error');
    expect(runtime.attempts.map((attempt) => attempt.id)).toEqual(['live', 'failed-peer']);
    expect(runtime.state().keyId).toBe('key-a');
    expect(runtime.state().clients).toContain('live');
    expect(runtime.wasm.unregister_client).not.toHaveBeenCalledWith('live');
  });

  it('accepts the same imported A after restoring A and still refuses B', async () => {
    const runtime = worker(source, async () => {});
    const live = await runtime.init('stored-key', config, 'live-tab');
    await expectMessage(live, 'runtime_ready');
    expect(runtime.state().keyId).toBe('key-a');
    const same = await runtime.init('same-key', imported('key-a'), 'same-tab');
    await expectMessage(same, 'runtime_ready');
    expect(runtime.state().keyId).toBe('key-a');
    const other = await runtime.init('other-key', imported('key-b'), 'other-tab');
    await expectMessage(other, 'runtime_error');
    expect(runtime.attempts.map((attempt) => attempt.id)).toEqual(['stored-key', 'same-key']);
    expect(runtime.state().clients).toEqual(['stored-key', 'same-key']);
    expect(runtime.wasm.unregister_client).not.toHaveBeenCalled();
  });

  it('keeps the live binding if a later Host restores a different key', async () => {
    const runtime = worker(source, async () => {});
    const live = await runtime.init('stored-key', config, 'live-tab');
    await expectMessage(live, 'runtime_ready');
    runtime.wasm.register_client.mockResolvedValueOnce('key-b');
    const changed = await runtime.init('changed-storage', config, 'changed-tab');
    await expectMessage(changed, 'runtime_error');
    expect(runtime.wasm.unregister_client).toHaveBeenCalledWith('changed-storage');
    expect(runtime.wasm.unregister_client).not.toHaveBeenCalledWith('stored-key');
    expect(runtime.state().keyId).toBe('key-a');
    expect(runtime.state().clients).toEqual(['stored-key']);
  });

  it('allows a replacement key after WASM bootstrap fails', async () => {
    const runtime = worker(source, async () => {});
    runtime.fetch.mockRejectedValueOnce(new Error('network_unavailable'));
    const failed = await runtime.init('failed-bootstrap', imported('key-a'));
    await expectMessage(failed, 'runtime_error');
    await runtime.unregister(failed.id);
    const b = await runtime.init('replacement', imported('key-b'));
    await expectMessage(b, 'runtime_ready');
    expect(runtime.attempts.map((attempt) => attempt.config.realm_client_key.key_id)).toEqual([
      'key-b',
    ]);
  });

  it('does not release a binding while another matching registration is pending', async () => {
    const pending = deferred();
    const runtime = worker(source, async (id) => {
      if (id === 'failed') throw new Error('client_revoked');
      if (id === 'pending') await pending.promise;
    });
    const first = await runtime.init('failed', imported('key-a'), 'failed-tab');
    const second = await runtime.init('pending', imported('key-a'), 'pending-tab');
    await expectMessage(first, 'runtime_error');
    const other = await runtime.init('other-key', imported('key-b'), 'other-tab');
    await expectMessage(other, 'runtime_error');
    expect(runtime.state().keyId).toBe('key-a');
    pending.resolve();
    await expectMessage(second, 'runtime_ready');
    expect(runtime.state().keyId).toBe('key-a');
  });

  it('cleans up a cancelled in-flight registration before allowing a replacement key', async () => {
    const pending = deferred();
    const runtime = worker(source, async (id) => {
      if (id === 'cancelled') await pending.promise;
    });
    const a = await runtime.init('cancelled', imported('key-a'));
    await vi.waitFor(() => expect(runtime.attempts).toHaveLength(1));
    const cancelled = runtime.unregister(a.id);
    pending.resolve();
    await cancelled;
    expect(a.messages().some((message) => message.type === 'runtime_ready')).toBe(false);
    expect(runtime.wasm.unregister_client).toHaveBeenCalledWith(a.id);
    const b = await runtime.init('replacement', imported('key-b'));
    await expectMessage(b, 'runtime_ready');
    expect(runtime.state().clients).toEqual(['replacement']);
  });

  it('does not change the verified workload or Realm when retrying a failed key', async () => {
    const runtime = worker(source, async () => {
      throw new Error('client_revoked');
    });
    const a = await runtime.init('failed-a', imported('key-a'));
    await expectMessage(a, 'runtime_error');
    const other = await runtime.init('other-realm', {
      ...imported('key-b'),
      realm: 'other@example.com',
    });
    await expectMessage(other, 'runtime_error');
    expect(runtime.attempts).toHaveLength(1);
  });

  it('releases the key only after every concurrent registration has failed', async () => {
    const first = deferred(),
      second = deferred();
    const runtime = worker(source, async (id) => {
      if (id === 'first') await first.promise;
      if (id === 'second') await second.promise;
    });
    const a = await runtime.init('first', imported('key-a'), 'first-tab');
    const peer = await runtime.init('second', imported('key-a'), 'second-tab');
    await vi.waitFor(() => expect(runtime.attempts).toHaveLength(2));
    first.reject(new Error('client_revoked'));
    await expectMessage(a, 'runtime_error');
    expect(runtime.state().keyId).toBe('key-a');
    second.reject(new Error('client_revoked'));
    await expectMessage(peer, 'runtime_error');
    expect(runtime.state().keyId).toBe(null);
    expect(runtime.state().privateKey).toBeUndefined();
    const b = await runtime.init('replacement', imported('key-b'));
    await expectMessage(b, 'runtime_ready');
  });

  it.each(['port', 'stale'])(
    'releases the last live client through %s cleanup',
    async (cleanup) => {
      const runtime = worker(source, async () => {});
      const a = await runtime.init('first', imported('key-a'), 'first-tab');
      await expectMessage(a, 'runtime_ready');
      if (cleanup === 'port') a.portMessage({ type: 'unregister_client' });
      else await runtime.stale('first-tab');
      await vi.waitFor(() => expect(runtime.state().clients).toHaveLength(0));
      const b = await runtime.init('replacement', imported('key-b'));
      await expectMessage(b, 'runtime_ready');
      expect(runtime.state().keyId).toBe('key-b');
    }
  );

  it('keeps the binding while failed-registration cleanup is still in flight', async () => {
    const cleanup = deferred();
    const runtime = worker(source, async (id) => {
      if (id === 'failed') throw new Error('client_revoked');
    });
    runtime.wasm.unregister_client.mockImplementationOnce(async () => cleanup.promise);
    const a = await runtime.init('failed', imported('key-a'));
    await vi.waitFor(() => expect(runtime.wasm.unregister_client).toHaveBeenCalledWith(a.id));
    const early = await runtime.init('early-retry', imported('key-b'));
    await expectMessage(early, 'runtime_error');
    expect(runtime.attempts).toHaveLength(1);
    cleanup.resolve();
    await expectMessage(a, 'runtime_error');
    const b = await runtime.init('replacement', imported('key-b'));
    await expectMessage(b, 'runtime_ready');
  });

  it('does not release an unconfirmed runtime when its cleanup fails', async () => {
    const runtime = worker(source, async (id) => {
      if (id === 'failed') throw new Error('client_revoked');
    });
    runtime.wasm.unregister_client.mockRejectedValueOnce(new Error('cleanup_failed'));
    const a = await runtime.init('failed', imported('key-a'));
    await expectMessage(a, 'runtime_error');
    const early = await runtime.init('early-retry', imported('key-b'));
    await expectMessage(early, 'runtime_error');
    expect(runtime.attempts).toHaveLength(1);
    await runtime.unregister(a.id); // Cleanup succeeds on the next attempt.
    const b = await runtime.init('replacement', imported('key-b'));
    await expectMessage(b, 'runtime_ready');
  });

  it('joins simultaneous cleanup requests for the same live registration', async () => {
    const cleanup = deferred();
    const runtime = worker(source, async () => {});
    const a = await runtime.init('live', imported('key-a'));
    await expectMessage(a, 'runtime_ready');
    runtime.wasm.unregister_client.mockImplementationOnce(async () => cleanup.promise);
    const first = runtime.unregister(a.id),
      second = runtime.unregister(a.id);
    await vi.waitFor(() => expect(runtime.wasm.unregister_client).toHaveBeenCalledTimes(1));
    cleanup.resolve();
    await Promise.all([first, second]);
    expect(runtime.wasm.unregister_client).toHaveBeenCalledTimes(1);
    expect(runtime.state().keyId).toBe(null);
  });
});
