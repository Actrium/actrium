const assert = require('node:assert/strict');
const { test } = require('node:test');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const Module = require('node:module');
const ts = require('typescript');
const source = resolve(__dirname, '../src/webrtc-coordinator.ts');
const compiled = ts.transpileModule(readFileSync(source, 'utf8'), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS },
}).outputText;
const loaded = new Module(source, module);
loaded._compile(compiled, source);
const { WebRtcCoordinator } = loaded.exports;

test('credential rotation updates live and future peer configurations', () => {
  let receive;
  const coordinator = new WebRtcCoordinator(
    {
      onMessage(handler) {
        receive = handler;
      },
    },
    {},
    {
      iceServers: [
        { urls: 'stun:stun.example.org' },
        { urls: ['turn:turn.example.org?transport=udp'], username: 'old', credential: 'old' },
      ],
      iceTransportPolicy: 'relay',
    }
  );
  const updated = [];
  const original = {
    iceServers: [
      { urls: 'stun:stun.example.org' },
      {
        urls: 'turn:turn.example.org',
        username: 'old',
        credential: 'old',
        credentialType: 'password',
      },
    ],
    iceTransportPolicy: 'relay',
    bundlePolicy: 'max-bundle',
  };
  coordinator.peers.set('active', {
    connection: {
      signalingState: 'stable',
      getConfiguration() {
        return original;
      },
      setConfiguration(config) {
        updated.push(config);
      },
    },
  });
  coordinator.peers.set('closed', {
    connection: {
      signalingState: 'closed',
      setConfiguration() {
        throw Error('closed connection updated');
      },
    },
  });
  receive({ type: 'update_turn_credential', payload: { username: 'new', password: 'new-secret' } });
  assert.equal(updated.length, 1);
  assert.equal(updated[0].iceServers[1].username, 'new');
  assert.equal(updated[0].iceServers[1].credential, 'new-secret');
  assert.equal(updated[0].iceServers[1].credentialType, 'password');
  assert.deepEqual(updated[0].iceServers[0], original.iceServers[0]);
  assert.equal(updated[0].bundlePolicy, 'max-bundle');
  assert.equal(updated[0].iceTransportPolicy, 'relay');
  assert.equal(
    coordinator.withTurnCredential([{ urls: 'turn:turn.example.org' }])[0].username,
    'new'
  );
});

function pathFixture(entries, getStats) {
  const events = [];
  const coordinator = new WebRtcCoordinator({ onMessage() {}, sendToSW(event) { events.push(event); } }, {});
  const connection = { connectionState: 'connected', getStats: getStats || (async () => new Map(entries.map((entry) => [entry.id, entry]))) };
  coordinator.peers.set('peer', { connection });
  return { coordinator, connection, events };
}

test('routing classification uses the selected pair and ignores unused relay candidates', async () => {
  const f = pathFixture([
    { id: 'transport', type: 'transport', selectedCandidatePairId: 'selected' },
    { id: 'selected', type: 'candidate-pair', localCandidateId: 'local', remoteCandidateId: 'remote' },
    { id: 'local', candidateType: 'host' }, { id: 'remote', candidateType: 'srflx' },
    { id: 'unused', candidateType: 'relay' },
  ]);
  await f.coordinator.observeNetworkPath('peer', f.connection);
  assert.deepEqual(f.events[0].payload, { eventType: 'network_path_changed', data: { peerId: 'peer', relayed: false } });
});

test('a TURN candidate at either selected endpoint marks the path relayed', async () => {
  for (const localRelay of [true, false]) {
    const f = pathFixture([
      { id: 'transport', type: 'transport', selectedCandidatePairId: 'selected' },
      { id: 'selected', localCandidateId: 'local', remoteCandidateId: 'remote' },
      { id: 'local', candidateType: localRelay ? 'relay' : 'host' },
      { id: 'remote', candidateType: localRelay ? 'host' : 'relay' },
    ]);
    await f.coordinator.observeNetworkPath('peer', f.connection);
    assert.equal(f.events[0].payload.data.relayed, true);
  }
});

test('unknown selected path stays unknown and stale stats cannot classify a replacement', async () => {
  const f = pathFixture([]);
  await f.coordinator.observeNetworkPath('peer', f.connection);
  assert.equal(f.events[0].payload.data.relayed, null);
  let finish;
  const g = pathFixture([], () => new Promise((resolve) => { finish = resolve; }));
  const observation = g.coordinator.observeNetworkPath('peer', g.connection);
  g.coordinator.peers.set('peer', { connection: {} });
  finish(new Map()); await observation;
  assert.equal(g.events.length, 0);
});

test('unrecognized selected candidate types do not claim a direct path', async () => {
  const f = pathFixture([
    { id: 'transport', type: 'transport', selectedCandidatePairId: 'selected' },
    { id: 'selected', type: 'candidate-pair', localCandidateId: 'local', remoteCandidateId: 'remote' },
    { id: 'local', candidateType: 'future-type' }, { id: 'remote', candidateType: 'host' },
  ]);
  await f.coordinator.observeNetworkPath('peer', f.connection);
  assert.equal(f.events[0].payload.data.relayed, null);
});
