import assert from 'node:assert/strict';
import { test } from 'vitest';
import { ActorRef } from './actor-ref';

test('logical peer events survive relay-to-direct promotion and single-path failure', () => {
  let receive;
  const ref = new ActorRef({ getSWBridge: () => ({ onMessage(handler) { receive = handler; } }) });
  const events = [];
  ref.on('peerConnected', (event) => events.push(['connected', event.peerId]));
  ref.on('peerDisconnected', (event) => events.push(['disconnected', event.peerId]));
  const send = (eventType, data) => receive({ type: 'webrtc_event', payload: { eventType, data: { peerId: 'peer', ...data } } });
  send('relay_state_changed', { connected: true });
  send('datachannel_open', { channelId: 0 });
  send('datachannel_open', { channelId: 1 });
  send('connection_state_changed', { state: 'failed' });
  send('datachannel_close', { channelId: 0 });
  assert.deepEqual(events, [['connected', 'peer']]);
  send('relay_state_changed', { connected: false });
  assert.deepEqual(events, [['connected', 'peer'], ['disconnected', 'peer']]);
  ref.dispose();
});

test('a physical RTC failure does not reject pending operations or other peers', () => {
  let receive;
  const ref = new ActorRef({ getSWBridge: () => ({ onMessage(handler) { receive = handler; } }) });
  let rejected = 0;
  ref.pendingRequests.set('inflight', { reject: () => rejected++, resolve() {}, timeout: 0 });
  for (const state of ['failed', 'closed']) {
    receive({ type: 'webrtc_event', payload: { eventType: 'connection_state_changed', data: { peerId: 'other-peer', state } } });
  }
  assert.equal(rejected, 0);
  assert.equal(ref.pendingRequests.size, 1);
  ref.dispose(); assert.equal(rejected, 1);
});

test('RTC recovery restores an already open RPC channel without another open event', () => {
  let receive;
  const ref = new ActorRef({ getSWBridge: () => ({ onMessage(handler) { receive = handler; } }) });
  const events = [];
  ref.on('peerConnected', () => events.push('connected'));
  ref.on('peerDisconnected', () => events.push('disconnected'));
  const send = (eventType, data) => receive({ type: 'webrtc_event', payload: { eventType, data: { peerId: 'peer', ...data } } });
  send('connection_state_changed', { state: 'connected' });
  assert.deepEqual(events, [], 'ICE alone does not make RPC usable');
  send('datachannel_open', { channelId: 0 });
  send('connection_state_changed', { state: 'disconnected' });
  send('connection_state_changed', { state: 'connected' });
  assert.deepEqual(events, ['connected', 'disconnected', 'connected']);
  send('datachannel_close', { channelId: 0 });
  send('connection_state_changed', { state: 'connected' });
  assert.deepEqual(events, ['connected', 'disconnected', 'connected', 'disconnected']);
  ref.dispose();
});

test('relay loss after RTC recovery keeps the logical peer connected', () => {
  let receive;
  const ref = new ActorRef({ getSWBridge: () => ({ onMessage(handler) { receive = handler; } }) });
  const events = [];
  ref.on('peerConnected', () => events.push('connected'));
  ref.on('peerDisconnected', () => events.push('disconnected'));
  const send = (eventType, data) => receive({ type: 'webrtc_event', payload: { eventType, data: { peerId: 'peer', ...data } } });
  send('relay_state_changed', { connected: true });
  send('datachannel_open', { channelId: 0 });
  send('connection_state_changed', { state: 'disconnected' });
  send('connection_state_changed', { state: 'connected' });
  send('relay_state_changed', { connected: false });
  assert.deepEqual(events, ['connected']);
  for (const state of ['failed', 'closed']) {
    send('connection_state_changed', { state });
    send('connection_state_changed', { state: 'connected' });
  }
  assert.deepEqual(events, ['connected', 'disconnected'], 'terminal failures require a new channel');
  ref.dispose();
});
