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

test('closing a disconnected peer reports the closed connection instance', async (t) => {
  const original = globalThis.RTCPeerConnection;
  globalThis.RTCPeerConnection = class {
    constructor() { this.connectionState = 'new'; }
    close() { this.connectionState = 'closed'; }
  };
  t.after(() => { globalThis.RTCPeerConnection = original; });

  const events = [];
  const bridge = {
    onMessage() {},
    sendToSW(message) { events.push(message.payload); },
  };
  const coordinator = new WebRtcCoordinator(bridge, {});
  await coordinator.createPeerConnection('peer');
  const first = coordinator.getPeerInfo('peer');
  first.connection.connectionState = 'disconnected';
  first.connection.onconnectionstatechange();
  const disconnected = events.find(event => event.eventType === 'connection_state_changed');
  assert.equal(disconnected.data.connectionId, first.connectionId);

  await coordinator.handleWebRtcCommand({ action: 'close_peer', peerId: 'peer' });
  const closed = events.find(event => event.eventType === 'peer_connection_closed');
  assert.equal(first.connection.connectionState, 'closed');
  assert.deepEqual(closed.data, { peerId: 'peer', connectionId: first.connectionId });

  await coordinator.createPeerConnection('peer');
  const replacement = coordinator.getPeerInfo('peer');
  assert.notEqual(replacement.connectionId, first.connectionId);
  const previousCount = events.length;
  first.connection.onconnectionstatechange();
  assert.equal(events.length, previousCount, 'stale state events must not affect the replacement');
  coordinator.dispose();
});
