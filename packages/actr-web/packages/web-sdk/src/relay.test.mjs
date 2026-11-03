import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { setImmediate as nextTurn } from 'node:timers/promises';
import { test } from 'vitest';
import assert from 'node:assert/strict';

const worker = readFileSync(new URL('./actor.sw.js', import.meta.url), 'utf8');
const begin = worker.indexOf('// @relay-client begin');
const end = worker.indexOf('// @relay-client end', begin);
assert.ok(begin >= 0 && end > begin);
const source = worker.slice(begin, end) + '\nglobalThis.Client = LoranRelayClient;';

class Socket {
  static instances = [];
  constructor(url, protocols) {
    this.url = url; this.protocols = protocols; this.readyState = 0;
    this.protocol = 'loran-relay.v1'; this.bufferedAmount = 0; this.sent = [];
    Socket.instances.push(this);
  }
  open() { this.readyState = 1; this.onopen?.(); }
  send(frame) { this.sent.push(typeof frame === 'string' ? JSON.parse(frame) : new Uint8Array(frame)); }
  deliver(frame) { this.onmessage?.({ data: typeof frame === 'object' && !(frame instanceof ArrayBuffer) ? JSON.stringify(frame) : frame }); }
  close() { this.readyState = 3; this.onclose?.(); }
}

async function until(predicate) {
  const deadline = Date.now() + 2500;
  while (!predicate()) { assert.ok(Date.now() < deadline, 'condition deadline exceeded'); await nextTurn(); }
}

function fixture(prepare, options = {}) {
  Socket.instances = [];
  const intervals = new Set();
  const context = { setTimeout, clearTimeout, TextEncoder, Uint8Array, DataView, ArrayBuffer, Date, Math, BigInt,
    setInterval: (fn) => { intervals.add(fn); return fn; }, clearInterval: (fn) => intervals.delete(fn) };
  runInNewContext(source, context);
  let identity = { endpoint: 'local', token: 'opaque-secret', expiresAt: Date.now() + 3600000 };
  const events = [];
  const client = new context.Client({ WebSocketImpl: Socket, credentials: async () => identity,
    discover: async () => 'wss://relay.example/ws',
    prepare: prepare ?? (async (id, peer, key) => ({ ready: () => events.push(['ready', id, peer, key]), receive: (bytes) => events.push(['data', id, [...bytes]]), closed: () => events.push(['closed', id]) })),
    onError: (error) => events.push(['error', error.message]), ...options });
  return { client, events, intervals, identity: () => identity, replaceIdentity: (value) => { identity = value; } };
}

test('historical failed targets do not permanently exhaust relay capacity', async () => {
  const { client } = fixture();
  client.waitSocket = async () => { throw new Error('offline'); };
  for (let i = 0; i < 1024; i++) {
    await assert.rejects(client.ensurePeer(`peer-${i}`), /offline/);
  }
  await nextTurn();
  assert.equal(client.desired.size, 1024);
  assert.equal(client.channels.size, 0);
  await assert.rejects(client.ensurePeer('new-peer'), /offline/);
  assert.equal(client.desired.size, 1024);
  assert.equal(client.desired.has('peer-0'), false);
  assert.equal(client.desired.has('new-peer'), true);
  await assert.rejects(client.ensurePeer('peer-0', true), /no longer desired/);
  assert.equal(client.desired.has('peer-0'), false);
});

test('capacity eviction keeps active and in-flight peers', () => {
  const { client } = fixture();
  for (let i = 0; i < 1024; i++) client.rememberPeer(`peer-${i}`);
  client.peers.set('peer-0', 42);
  client.dials.set('peer-1', Promise.resolve());
  assert.equal(client.rememberPeer('new-peer'), true);
  assert.equal(client.desired.has('peer-0'), true);
  assert.equal(client.desired.has('peer-1'), true);
  assert.equal(client.desired.has('peer-2'), false);
});

async function login(client) {
  client.start();
  await until(() => Socket.instances.length > 0);
  const socket = Socket.instances.at(-1);
  socket.open();
  socket.deliver({ type: 'welcome', session: 1 });
  await until(() => client.authenticated);
  return socket;
}

test('socket open does not mark a peer ready; authenticated handshake precedes connect', async () => {
  const { client } = fixture();
  const dial = client.ensurePeer('remote'); dial.catch(() => {});
  await until(() => Socket.instances.length === 1);
  const socket = Socket.instances[0]; socket.open();
  assert.equal(socket.url, 'wss://relay.example/ws');
  assert.deepEqual(socket.sent, [{ type: 'hello', endpoint: 'local', token: 'opaque-secret' }]);
  assert.equal(client.authenticated, false);
  socket.deliver({ type: 'welcome', session: 1 });
  await until(() => socket.sent.some((frame) => frame.type === 'connect'));
  let completed = false; dial.then(() => { completed = true; }).catch(() => {});
  await nextTurn(); assert.equal(completed, false);
  socket.deliver({ type: 'connected', channel: 17, request: 1, peer: 'remote', metadata: '' });
  const entry = await dial;
  assert.equal(entry.channel, 17); assert.equal(entry.ready, true);
  client.stop(); await client.loop;
});

test('incoming is accepted only after receive installation; connected gates sends', async () => {
  let installed;
  const installedPromise = new Promise((resolve) => { installed = resolve; });
  const events = [];
  const { client } = fixture(async () => { await installedPromise; events.push('installed'); return { ready: () => events.push('ready'), receive: () => {}, closed: () => {} }; });
  const socket = await login(client);
  socket.deliver({ type: 'incoming', channel: 21, peer: 'remote', metadata: '' });
  await nextTurn();
  assert.equal(socket.sent.some((frame) => frame.type === 'accept'), false);
  installed(); await until(() => socket.sent.some((frame) => frame.type === 'accept'));
  assert.throws(() => client.send(21, new Uint8Array([1])), /not ready/);
  socket.deliver({ type: 'connected', channel: 21, request: null, peer: 'remote', metadata: '' });
  await until(() => events.includes('ready'));
  client.send(21, new Uint8Array([0, 255, 12]));
  const frame = socket.sent.at(-1);
  assert.equal(new DataView(frame.buffer).getBigUint64(0, false), 21n);
  assert.deepEqual([...frame.subarray(8)], [0, 255, 12]);
  client.stop(); await client.loop;
});

test('a pending dial resolves after local wire registration completes', async () => {
  let ready;
  const readyPromise = new Promise((resolve) => { ready = resolve; });
  const { client } = fixture(async () => ({ ready: () => readyPromise, receive: () => {}, closed: () => {} }));
  const socket = await login(client);
  let completed = false;
  const dial = client.ensurePeer('remote'); dial.then(() => { completed = true; });
  await until(() => socket.sent.some((frame) => frame.type === 'connect'));
  socket.deliver({ type: 'connected', channel: 22, request: 1, peer: 'remote', metadata: '' });
  await nextTurn(); assert.equal(completed, false);
  ready(); await dial; assert.equal(completed, true);
  client.stop(); await client.loop;
});

test('encrypted peers become available only after mutual confirmation and failures close them', async () => {
  let accept = true;
  const f = fixture(async () => ({
    ready: () => false,
    receive: async () => { if (!accept) throw new Error('Ciphertext rejected'); return true; },
    closed: () => {},
  }));
  const socket = await login(f.client);
  const dial = f.client.ensurePeer('remote');
  await until(() => socket.sent.some((frame) => frame.type === 'connect'));
  socket.deliver({ type: 'connected', channel: 28, request: 1, peer: 'remote', metadata: '' });
  await until(() => f.client.channels.get(28)?.active);
  assert.equal(f.client.peers.has('remote'), false);
  let complete = false; dial.then(() => { complete = true; });
  await nextTurn(); assert.equal(complete, false);
  const frame = new Uint8Array(9); new DataView(frame.buffer).setBigUint64(0, 28n, false);
  socket.deliver(frame.buffer);
  await dial;
  assert.equal(f.client.peers.get('remote'), 28);
  accept = false;
  socket.deliver(frame.buffer);
  await until(() => !f.client.channels.has(28));
  assert.equal(f.client.peers.has('remote'), false);
  f.client.stop(); await f.client.loop;
});

test('certificate refresh reconnects even when the relay bearer is unchanged', async () => {
  const f = fixture();
  f.identity().securityEpoch = 'old-certified-key';
  const socket = await login(f.client);
  f.replaceIdentity({ ...f.identity(), securityEpoch: 'new-certified-key' });
  await [...f.intervals][0]();
  assert.equal(socket.readyState, 3);
  f.client.stop(); await f.client.loop;
});

test('binary delivery preserves payload bytes and stale close cannot remove a replacement', async () => {
  const { client, events } = fixture();
  const socket = await login(client);
  socket.deliver({ type: 'connected', channel: 23, request: null, peer: 'remote', metadata: '' });
  await until(() => client.peers.get('remote') === 23);
  socket.deliver({ type: 'connected', channel: 24, request: null, peer: 'remote', metadata: '' });
  await until(() => client.peers.get('remote') === 24);
  socket.deliver({ type: 'closed', channel: 23, reason: 'peer_left' });
  const frame = new Uint8Array(11); new DataView(frame.buffer).setBigUint64(0, 24n, false); frame.set([0, 255, 1], 8);
  socket.deliver(frame.buffer);
  await until(() => events.some((event) => event[0] === 'data'));
  assert.equal(client.peers.get('remote'), 24);
  assert.deepEqual(events.find((event) => event[0] === 'data'), ['data', 24, [0, 255, 1]]);
  client.stop(); await client.loop;
});

test('send backpressure retires the affected channel and does not replay submitted data', async () => {
  const { client } = fixture();
  const socket = await login(client);
  socket.deliver({ type: 'connected', channel: 25, request: null, peer: 'remote', metadata: '' });
  await until(() => client.peers.has('remote'));
  client.send(25, new Uint8Array([1, 2, 3]));
  assert.equal(socket.sent.filter((frame) => frame instanceof Uint8Array).length, 1);
  socket.bufferedAmount = 16 * 1024 * 1024;
  assert.throws(() => client.send(25, new Uint8Array([9])), /budget/);
  assert.equal(client.channels.has(25), false);
  assert.equal(socket.sent.filter((frame) => frame instanceof Uint8Array).length, 1);
  client.stop(); await client.loop;
});

test('credential renewal reconnects near expiry and identity changes retire the old socket', async () => {
  const f = fixture();
  const socket = await login(f.client);
  f.identity().expiresAt = Date.now() + 1000;
  f.replaceIdentity({ endpoint: 'local', token: 'renewed-secret', expiresAt: Date.now() + 3600000 });
  await [...f.intervals][0]();
  assert.equal(socket.readyState, 3);
  f.client.stop(); await f.client.loop;
  const g = fixture(); const other = await login(g.client);
  g.replaceIdentity({ endpoint: 'new-identity', token: 'new-secret' });
  await [...g.intervals][0](); assert.equal(other.readyState, 3);
  g.client.stop(); await g.client.loop;
});

test('malformed frames close the connection; disabled discovery does not open sockets', async () => {
  const f = fixture(); const socket = await login(f.client);
  socket.deliver(new Uint8Array(7).buffer);
  await until(() => socket.readyState === 3);
  f.client.stop(); await f.client.loop;
  const g = fixture(undefined, { discover: async () => null });
  await assert.rejects(g.client.ensurePeer('remote'), /unavailable/);
  assert.equal(Socket.instances.length, 0);
  g.client.stop(); await g.client.loop;
});

test('relay discovery resumes after the service becomes enabled', async () => {
  let enabled = false;
  let discoveries = 0;
  const f = fixture(undefined, { discover: async () => {
    discoveries++;
    return enabled ? 'wss://relay.example/ws' : null;
  } });
  f.client.start();
  await until(() => discoveries === 1);
  enabled = true;
  const dial = f.client.ensurePeer('remote');
  dial.catch(() => {});
  try {
    await until(() => Socket.instances.length === 1);
    const socket = Socket.instances[0];
    socket.open();
    socket.deliver({ type: 'welcome', session: 1 });
    await until(() => socket.sent.some((frame) => frame.type === 'connect'));
    const request = socket.sent.find((frame) => frame.type === 'connect').request;
    socket.deliver({ type: 'connected', channel: 73, request, peer: 'remote', metadata: '' });
    assert.equal((await dial).channel, 73);
    assert.ok(discoveries >= 2);
  } finally {
    f.client.stop();
    await f.client.loop;
  }
});

test('relay discovery recovers after a connected service is disabled and re-enabled', async () => {
  let enabled = true;
  const f = fixture(undefined, { discover: async () => enabled ? 'wss://relay.example/ws' : null });
  const first = await login(f.client);
  enabled = false;
  first.close();
  await until(() => f.client.disabled);
  enabled = true;
  await until(() => Socket.instances.length === 2);
  const second = Socket.instances[1];
  second.open();
  second.deliver({ type: 'welcome', session: 2 });
  await until(() => f.client.authenticated);
  f.client.stop();
  await f.client.loop;
});

test('rejected unfinished dials fail immediately using request correlation', async () => {
  const { client } = fixture(); const socket = await login(client);
  const dial = client.ensurePeer('remote');
  await until(() => socket.sent.some((frame) => frame.type === 'connect'));
  socket.deliver({ type: 'closed', channel: 51, request: 1, reason: 'rejected' });
  await assert.rejects(dial, /rejected/);
  assert.equal(client.pending.size, 0);
  client.stop(); await client.loop;
});

test('reconnect reopens desired peers with new local generations and does not replay bytes', async () => {
  const { client, events } = fixture(); const first = await login(client);
  const initial = client.ensurePeer('remote');
  await until(() => first.sent.some((frame) => frame.type === 'connect'));
  first.deliver({ type: 'connected', channel: 17, request: 1, peer: 'remote', metadata: '' });
  const old = await initial; client.send(17, new Uint8Array([7]));
  first.close(); await until(() => Socket.instances.length === 2);
  const second = Socket.instances[1]; second.open(); second.deliver({ type: 'welcome', session: 1 });
  await until(() => second.sent.some((frame) => frame.type === 'connect'));
  const request = second.sent.find((frame) => frame.type === 'connect').request;
  second.deliver({ type: 'connected', channel: 17, request, peer: 'remote', metadata: '' });
  await until(() => client.peers.has('remote'));
  assert.notEqual(client.channels.get(17).key, old.key);
  assert.equal(second.sent.some((frame) => frame instanceof Uint8Array), false);
  second.deliver({ type: 'connected', channel: 17, request: null, peer: 'remote', metadata: '' });
  await nextTurn();
  assert.equal(events.filter((event) => event[0] === 'ready').length, 2);
  client.stop(); await client.loop;
});

test('warm recovery retries an offline peer without another application send', async () => {
  const f = fixture(); const socket = await login(f.client);
  const dial = f.client.ensurePeer('remote');
  await until(() => socket.sent.some((frame) => frame.type === 'connect'));
  socket.deliver({ type: 'error', request: 1, code: 'peer_unavailable' });
  await assert.rejects(dial, /peer_unavailable/);
  await nextTurn();
  await [...f.intervals][0]();
  await until(() => socket.sent.filter((frame) => frame.type === 'connect').length === 2);
  socket.deliver({ type: 'connected', channel: 61, request: 2, peer: 'remote', metadata: '' });
  await until(() => f.client.peers.get('remote') === 61);
  f.client.stop(); await f.client.loop;
});

const adapterSource = worker.slice(begin, worker.indexOf('// @relay-adapter end', end))
  + '\nLoranRelayClient.prototype.start = function() { return Promise.resolve(); }; globalThis.startAdapter = startRelayForClient;';

function adapterFixture(overrides = {}) {
  const events = [];
  let send;
  const context = { setTimeout, clearTimeout, setInterval, clearInterval, URL, AbortController,
    TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer, Date, Math, BigInt, WebSocket: Socket,
    clientPorts: new Map([['client', { postMessage(event) { events.push(event); } }]]), emitSwLog() {},
    fetch: async () => new Response(JSON.stringify({ version: 1, enabled: true })),
    wasm_bindgen: {
      relay_connection_info: async () => ({ signalingUrl: 'wss://loran.example/signaling', endpoint: 'local', token: 'private-token' }),
      prepare_relay_connection: async (_, endpoint, key, callback) => { send = callback; events.push({ endpoint, key }); return endpoint; },
      start_relay_handshake: () => false,
      handle_relay_record: async () => true,
      relay_channel_closed: async () => {},
    }, ...overrides };
  runInNewContext(adapterSource, context);
  const client = context.startAdapter('client');
  return { client, context, events, sender: () => send };
}

test('worker adapter delegates encrypted records to WASM and awaits authenticated admission', async () => {
  let release;
  const blocked = new Promise((resolve) => { release = resolve; });
  const f = adapterFixture();
  f.context.wasm_bindgen.handle_relay_record = async (_, key, record) => {
    assert.equal(key, '1:71');
    assert.deepEqual([...record], [9, 8, 7]);
    await blocked;
    return true;
  };
  const handler = await f.client.prepare(71, 'remote', '1:71');
  const sent = [];
  f.client.send = (channel, payload) => sent.push([channel, [...payload]]);
  f.client.channels.set(71, { key: '1:71' });
  assert.equal(await handler.ready(), false);
  f.sender()(new Uint8Array([5, 6, 7]));
  assert.deepEqual(sent, [[71, [5, 6, 7]]]);
  let consumed = false;
  const pending = handler.receive(new Uint8Array([9, 8, 7])).then((ready) => { consumed = ready; });
  await nextTurn(); assert.equal(consumed, false);
  assert.equal(f.events.some((event) => event.payload?.data?.connected), false);
  release(); await pending; assert.equal(consumed, true);
  assert.equal(f.events.filter((event) => event.payload?.data?.connected).length, 1);
  f.client.channels.set(71, { key: '2:71' });
  assert.throws(() => f.sender()(new Uint8Array([5])), /Stale encrypted relay channel/);
  assert.equal(JSON.stringify(f.events).includes('private-token'), false);
});

test('worker discovery uses the signaling origin, forbids downgrade and bounds reads', async () => {
  const f = adapterFixture();
  f.context.fetch = async (url, options) => {
    assert.equal(url.href, 'https://loran.example/relay/config');
    assert.equal(options.redirect, 'error');
    return new Response(JSON.stringify({ version: 1, enabled: true }));
  };
  assert.equal(await f.client.discover(), 'wss://loran.example/relay/ws');
  f.context.fetch = async () => new Response(JSON.stringify({ version: 1, enabled: true, url: 'ws://insecure.example/ws' }));
  await assert.rejects(f.client.discover(), /advertised relay URL/);
  let read = 0; let canceled = false;
  f.context.fetch = async () => ({ ok: true, body: { getReader: () => ({
    read: async () => { read++; return { value: new Uint8Array(32769), done: false }; },
    cancel: async () => { canceled = true; }, releaseLock() {},
  }) } });
  await assert.rejects(f.client.discover(), /size limit/);
  assert.equal(read, 1); assert.equal(canceled, true);
});

test('trusted AIS requests normalize TLS schemes, forbid redirects and bound proof responses', async () => {
  const f = adapterFixture();
  const request = f.context.__actrRelayTrustedRequest;
  let calls = 0;
  f.context.fetch = async (url, options) => {
    calls++;
    assert.equal(url.href, 'https://ais.example/ais/relay-identity');
    assert.equal(options.redirect, 'error');
    assert.equal(options.method, 'POST');
    assert.equal(options.body, '{"private":"proof"}');
    return new Response('{"certificate":true}');
  };
  assert.equal(await request('https://ais.example/ais/', 'relay-identity', '{"private":"proof"}', 'WSS://loran.example/ws'), '{"certificate":true}');
  await assert.rejects(request('http://ais.example/ais', 'relay-identity', 'secret', 'WSS://loran.example/ws'), /HTTPS AIS/);
  await assert.rejects(request('https://user:secret@ais.example/ais', 'relay-identity', 'secret', 'wss://loran.example/ws'), /HTTPS AIS/);
  assert.equal(calls, 1);
  let canceled = false;
  f.context.fetch = async () => ({ ok: true, body: { getReader: () => ({
    read: async () => ({ value: new Uint8Array(32769), done: false }),
    cancel: async () => { canceled = true; }, releaseLock() {},
  }) } });
  await assert.rejects(request('https://ais.example/ais', 'relay-identity', 'secret', 'wss://loran.example/ws'), /exceeds limit/);
  assert.equal(canceled, true);
  assert.equal(JSON.stringify(f.events).includes('proof'), false);
});

test('CLI worker template matches the SDK worker', () => {
  const template = readFileSync(new URL('../../../../../crates/actr-cli/assets/web-runtime/actor.sw.js', import.meta.url), 'utf8');
  assert.equal(template, worker);
});
