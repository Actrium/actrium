/* Actor-RTC Service Worker entry — wasm-bindgen guest path.
 *
 * Loads a wasm-bindgen guest bundle (produced by `tools/actr/wit-compile-web` →
 * `actr-web-abi` + `wasm-pack --target no-modules`) and bridges it into the
 * sw-host runtime. This is the only browser dispatch path post Option U
 * Phase 8 (the previous Component Model + jco variant was deleted; see
 * `packages/actr-web/docs/option-u-wit-compile-web.zh.md` §11).
 *
 * Required `runtimeConfig` fields:
 *   package_url       - URL of the `.actr` workload package
 *   runtime_wasm_url  - URL of the SW runtime WASM (sw-host wasm-pack output)
 *   trust             - array of `TrustAnchor` entries
 *
 * The signed package must contain a wasm-bindgen `*_bg.wasm` binary and its
 * generated JS glue as an authenticated resource. No executable sibling
 * artefacts are loaded outside the verified package.
 */

/* global wasm_bindgen */

let SW_BROADCAST = null;

// ── Console interception: forward WASM logs to main page ──
//
// Identical to actor.sw.js; the WBG path still emits Rust `log::info!` from
// sw-host, so this broadcaster stays the same.
(function () {
  const _origInfo = console.info;
  const _origWarn = console.warn;
  const _origError = console.error;
  const _origLog = console.log;

  function extractMessage(args) {
    return Array.from(args)
      .filter(
        (a) => typeof a === 'string' && !/^\s*(color|background|font-weight|padding)\s*:/.test(a)
      )
      .join(' ')
      .replace(/%c/g, '')
      .trim();
  }

  function broadcast(data) {
    self.clients
      .matchAll({ type: 'window' })
      .then((clients) => {
        for (const client of clients) {
          client.postMessage(data);
        }
      })
      .catch(() => {
        /* ignore */
      });
  }
  SW_BROADCAST = broadcast;

  console.info = function (...args) {
    _origInfo.apply(console, args);
    const msg = extractMessage(args);
    if (msg.includes('📨') && msg.includes('Echo request')) {
      const m = msg.match(/message='([^']*)'/);
      broadcast({ type: 'echo_event', event: 'request', detail: m ? m[1] : '', ts: Date.now() });
    } else if (msg.includes('📤') && msg.includes('Echo response')) {
      const m = msg.match(/reply='([^']*)'/);
      broadcast({ type: 'echo_event', event: 'response', detail: m ? m[1] : '', ts: Date.now() });
    }
    if (
      msg.includes('[SW]') ||
      msg.includes('[WBG]') ||
      msg.includes('EchoService') ||
      msg.includes('Echo') ||
      msg.includes('Registering') ||
      msg.includes('SendEcho') ||
      msg.includes('Scheduler') ||
      msg.includes('Dispatcher') ||
      msg.includes('HostGate') ||
      msg.includes('PeerGate')
    ) {
      broadcast({ type: 'sw_log', level: 'info', message: msg, ts: Date.now() });
    }
  };

  console.warn = function (...args) {
    _origWarn.apply(console, args);
    const msg = extractMessage(args);
    if (msg.length > 0) {
      broadcast({ type: 'sw_log', level: 'warn', message: msg, ts: Date.now() });
    }
  };

  console.error = function (...args) {
    _origError.apply(console, args);
    const msg = extractMessage(args);
    broadcast({ type: 'sw_log', level: 'error', message: msg, ts: Date.now() });
    if (msg.includes('Echo') || msg.includes('handle_request') || msg.includes('service')) {
      broadcast({ type: 'echo_event', event: 'error', detail: msg, ts: Date.now() });
    }
  };

  console.log = function (...args) {
    _origLog.apply(console, args);
    const msg = extractMessage(args);
    if (
      msg.includes('[EchoService]') ||
      msg.includes('[SW]') ||
      msg.includes('[WBG]') ||
      msg.includes('[SendEcho]') ||
      msg.includes('[WebRTC]')
    ) {
      broadcast({ type: 'sw_log', level: 'info', message: msg, ts: Date.now() });
    }
  };
})();

/** @type {import('@actrium/actr-web').SwRuntimeConfig | null} */
let RUNTIME_CONFIG = null;
let RUNTIME_CLIENT_KEY_ID = null;

function canonicalRuntimeConfig(value) {
  if (Array.isArray(value)) {
    return value.map(canonicalRuntimeConfig);
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalRuntimeConfig(value[key])])
    );
  }
  return value;
}

function runtimeConfigsEqual(left, right, rightKeyId = right.realm_client_key?.key_id) {
  const { realm_client_key: leftKey, ...leftConfig } = left;
  const { realm_client_key: rightKey, ...rightConfig } = right;
  return (
    (!leftKey || leftKey.key_id === rightKeyId) &&
    JSON.stringify(canonicalRuntimeConfig(leftConfig)) ===
    JSON.stringify(canonicalRuntimeConfig(rightConfig))
  );
}

function validateRuntimeConfig(config) {
  if (!config || typeof config !== 'object') {
    throw new Error('[SW] runtimeConfig must be an object');
  }
  for (const field of ['client_actr_type', 'package_url', 'runtime_wasm_url']) {
    if (typeof config[field] !== 'string' || config[field].trim() === '') {
      throw new Error(`[SW] runtimeConfig.${field} must be a non-empty string`);
    }
  }
  if (!Array.isArray(config.trust)) {
    throw new Error('[SW] runtimeConfig.trust must be an array');
  }
  return config;
}

function verifiedPackageActrType(extracted) {
  let manifest;
  try {
    manifest = JSON.parse(extracted.manifest_json);
  } catch (error) {
    throw new Error('[SW] Verified package manifest JSON is invalid: ' + String(error));
  }
  const fields = ['publisher', 'name', 'version'];
  for (const field of fields) {
    if (typeof manifest[field] !== 'string' || manifest[field].trim() === '') {
      throw new Error(`[SW] Verified package manifest is missing ${field}`);
    }
  }
  return `${manifest.publisher}/${manifest.name}@${manifest.version}`;
}

let wasmReady = false;
let wasmInitPromise = null;
let wsProbeDone = false;

// @relay-client begin
// Generic loran-relay.v1 client. Identity and application framing are injected
// by the worker adapter below; this class does not know business identities.
const MAX_DESIRED_PEERS = 1024;
class LoranRelayClient {
  constructor({ credentials, discover, prepare, onError = () => {}, WebSocketImpl = WebSocket }) {
    this.credentials = credentials;
    this.discover = discover;
    this.prepare = prepare;
    this.onError = onError;
    this.WebSocketImpl = WebSocketImpl;
    this.channels = new Map();
    this.peers = new Map();
    this.pending = new Map();
    this.dials = new Map();
    this.desired = new Set();
    this.waiters = new Set();
    this.request = 1;
    this.generation = 0;
    this.stopped = false;
    this.disabled = false;
    this.authenticated = false;
    this.socket = null;
  }

  start() {
    if (!this.loop) this.loop = this.run();
    return this.loop;
  }

  async run() {
    let backoff = 250;
    while (!this.stopped) {
      try {
        const url = await this.discover();
        if (!url) {
          this.disabled = true;
          this.wake();
        } else {
          this.disabled = false;
          const identity = await this.credentials();
          if (this.stopped) break;
          const lifetime = await this.cycle(url, identity);
          if (lifetime > 10000) backoff = 250;
        }
      } catch (error) {
        if (!this.stopped) this.onError(error);
      }
      if (this.stopped) break;
      await new Promise((resolve) => {
        this.resumeDelay = resolve;
        this.delayTimer = setTimeout(resolve, backoff + Math.random() * backoff / 4);
      });
      this.resumeDelay = null;
      backoff = Math.min(backoff * 2, 30000);
    }
  }

  wake() {
    for (const wake of this.waiters) wake();
    this.waiters.clear();
  }

  async waitSocket() {
    const deadline = Date.now() + 15000;
    let retriedDisabled = false;
    this.start();
    while (!this.stopped) {
      if (this.disabled) {
        if (retriedDisabled) throw new Error('Relay service unavailable');
        retriedDisabled = true;
        this.disabled = false;
        if (this.resumeDelay) {
          clearTimeout(this.delayTimer);
          this.resumeDelay();
        }
      }
      if (this.authenticated && this.socket && this.socket.readyState === 1) return;
      const remaining = deadline - Date.now();
      if (remaining <= 0) throw new Error('Relay connection deadline exceeded');
      await new Promise((resolve, reject) => {
        const timer = setTimeout(() => { this.waiters.delete(wake); reject(new Error('Relay connection deadline exceeded')); }, remaining);
        const wake = () => { clearTimeout(timer); resolve(); };
        this.waiters.add(wake);
      });
    }
    throw new Error('Relay service unavailable');
  }

  cycle(url, identity) {
    return new Promise((resolve, reject) => {
      const socket = new this.WebSocketImpl(url, ['loran-relay.v1']);
      socket.binaryType = 'arraybuffer';
      this.socket = socket;
      this.authenticated = false;
      const generation = ++this.generation;
      const started = Date.now();
      let chain = Promise.resolve();
      let queuedBytes = 0;
      let finished = false;
      let identityTimer;
      const helloTimer = setTimeout(() => finish(new Error('Relay authentication deadline exceeded')), 10000);
      const finish = (error) => {
        if (finished) return;
        finished = true;
        clearTimeout(helloTimer);
        clearInterval(identityTimer);
        socket.onopen = socket.onmessage = socket.onclose = socket.onerror = null;
        if (this.socket === socket) {
          this.socket = null;
          this.authenticated = false;
          for (const id of [...this.channels.keys()]) this.removeChannel(id);
          for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(new Error('Relay disconnected')); }
          this.pending.clear();
          this.wake();
        }
        try { socket.close(); } catch (_) { /* already closed */ }
        if (error) reject(error); else resolve(Date.now() - started);
      };
      this.finishCycle = finish;
      socket.onopen = () => {
        try {
          if (socket.protocol !== 'loran-relay.v1') throw new Error('Relay subprotocol not negotiated');
          const hello = JSON.stringify({ type: 'hello', endpoint: identity.endpoint, token: identity.token });
          if (new TextEncoder().encode(hello).length > 32768) throw new Error('Relay credentials exceed protocol limit');
          socket.send(hello);
        } catch (error) { finish(error); }
      };
      socket.onmessage = (event) => {
        const size = typeof event.data === 'string' ? new TextEncoder().encode(event.data).length : event.data.byteLength;
        if (!Number.isFinite(size) || size > 8 * 1024 * 1024 + 64 || queuedBytes + size > 16 * 1024 * 1024) {
          finish(new Error('Relay receive budget exhausted')); return;
        }
        queuedBytes += size;
        chain = chain.then(async () => {
          if (finished || this.socket !== socket) return;
          await this.handleFrame(event.data, generation);
          if (this.authenticated) clearTimeout(helloTimer);
        }).catch(finish).finally(() => { queuedBytes -= size; });
      };
      socket.onclose = () => finish();
      socket.onerror = () => finish(new Error('Relay WebSocket failed'));
      identityTimer = setInterval(async () => {
        try {
          const current = await this.credentials();
          if (finished) return;
          if (current.endpoint !== identity.endpoint || current.securityEpoch !== identity.securityEpoch || (identity.expiresAt && identity.expiresAt - Date.now() < 30000 && current.token !== identity.token)) finish();
          else if (this.authenticated) this.warmPeers();
        } catch (error) { finish(error); }
      }, 5000);
    });
  }

  control(message) {
    if (!this.socket || this.socket.readyState !== 1 || !this.authenticated) throw new Error('Relay not connected');
    const text = JSON.stringify(message);
    if (this.socket.bufferedAmount + new TextEncoder().encode(text).length > 16 * 1024 * 1024) throw new Error('Relay send budget exhausted');
    this.socket.send(text);
  }

  validId(id) { return Number.isSafeInteger(id) && id > 0; }
  validEndpoint(peer) { return typeof peer === 'string' && peer.length > 0 && new TextEncoder().encode(peer).length <= 512 && !/\p{Cc}/u.test(peer); }

  warmPeers() {
    const peers = [...this.desired];
    if (!peers.length) return;
    const offset = (this.warmOffset || 0) % peers.length;
    const count = Math.min(peers.length, 16);
    this.warmOffset = offset + count;
    for (let i = 0; i < count; i++) this.ensurePeer(peers[(offset + i) % peers.length], true).catch(() => {});
  }

  rememberPeer(peer) {
    if (this.desired.has(peer)) {
      this.desired.delete(peer);
    } else if (this.desired.size >= MAX_DESIRED_PEERS) {
      const stale = [...this.desired].find((candidate) => !this.peers.has(candidate) && !this.dials.has(candidate));
      if (stale === undefined) return false;
      this.desired.delete(stale);
    }
    this.desired.add(peer);
    return true;
  }

  async prepareChannel(channel, peer, generation) {
    if (!this.validId(channel) || !this.validEndpoint(peer)) throw new Error('Invalid relay channel');
    const existing = this.channels.get(channel);
    if (existing) {
      if (existing.peer !== peer) throw new Error('Relay channel identity changed');
      return existing;
    }
    const key = `${generation}:${channel}`;
    const handler = await this.prepare(channel, peer, key);
    if (this.stopped || this.generation !== generation || !this.socket) { if (handler.closed) await handler.closed(); throw new Error('Stale relay channel'); }
    const entry = { channel, peer, key, handler, ready: false, active: false };
    this.channels.set(channel, entry);
    return entry;
  }

  async handleFrame(frame, generation) {
    if (typeof frame !== 'string') {
      if (!this.authenticated) throw new Error('Unauthenticated relay data');
      const data = new Uint8Array(frame);
      if (data.byteLength < 8) throw new Error('Invalid relay data frame');
      const id = new DataView(data.buffer, data.byteOffset, data.byteLength).getBigUint64(0, false);
      if (id === 0n || id > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('Invalid relay channel');
      const entry = this.channels.get(Number(id));
      if (entry && entry.active) {
        try { if (await entry.handler.receive(data.subarray(8)) === true) this.completeChannel(entry); }
        catch (error) { this.closeChannel(entry.channel); this.onError(error); }
      }
      return;
    }
    if (new TextEncoder().encode(frame).length > 32768) throw new Error('Relay control exceeds protocol limit');
    const message = JSON.parse(frame);
    if (message.type === 'welcome') {
      if (this.authenticated || !this.validId(message.session)) throw new Error('Invalid relay welcome');
      this.authenticated = true;
      this.wake();
      this.warmPeers();
      return;
    }
    if (!this.authenticated) throw new Error('Relay authentication required');
    switch (message.type) {
      case 'incoming':
        try {
          await this.prepareChannel(message.channel, message.peer, generation);
          this.control({ type: 'accept', channel: message.channel });
        } catch (_) {
          if (this.validId(message.channel)) this.control({ type: 'reject', channel: message.channel });
          else throw new Error('Invalid relay incoming channel');
        }
        break;
      case 'connected': {
        const entry = await this.prepareChannel(message.channel, message.peer, generation);
        if (message.request != null) entry.request = message.request;
        if (!entry.active) {
          entry.active = true;
          entry.handshakeTimer = setTimeout(() => {
            if (this.channels.get(entry.channel) === entry && !entry.ready) this.closeChannel(entry.channel);
          }, 10000);
          if (await entry.handler.ready() !== false) this.completeChannel(entry);
        } else if (entry.ready) this.completeChannel(entry);
        break;
      }
      case 'closed':
        if (!this.validId(message.channel)) throw new Error('Invalid relay closed channel');
        if (this.pending.has(message.request)) {
          const pending = this.pending.get(message.request);
          clearTimeout(pending.timer); this.pending.delete(message.request);
          pending.reject(new Error(`Relay peer closed: ${message.reason}`));
        }
        this.removeChannel(message.channel);
        break;
      case 'error': {
        const pending = this.pending.get(message.request);
        if (pending) { clearTimeout(pending.timer); this.pending.delete(message.request); pending.reject(new Error(`Relay connect rejected: ${message.code}`)); }
        break;
      }
      default: throw new Error('Unknown relay control');
    }
  }

  completeChannel(entry) {
    if (this.channels.get(entry.channel) !== entry || !entry.active) return;
    clearTimeout(entry.handshakeTimer);
    entry.ready = true;
    this.peers.set(entry.peer, entry.channel);
    this.rememberPeer(entry.peer);
    const pending = this.pending.get(entry.request);
    if (pending) {
      if (pending.peer !== entry.peer) throw new Error('Relay response identity mismatch');
      clearTimeout(pending.timer); this.pending.delete(entry.request); pending.resolve(entry);
    }
  }

  ensurePeer(peer, warm = false) {
    if (!this.validEndpoint(peer)) return Promise.reject(new Error('Invalid relay endpoint'));
    const id = this.peers.get(peer);
    const existing = this.channels.get(id);
    if (existing && existing.ready) {
      if (!warm) this.rememberPeer(peer);
      return Promise.resolve(existing);
    }
    if (this.dials.has(peer)) return this.dials.get(peer);
    if (warm && !this.desired.has(peer)) return Promise.reject(new Error('Relay peer no longer desired'));
    if (!warm && !this.rememberPeer(peer)) return Promise.reject(new Error('Relay peer capacity exhausted'));
    const dial = (async () => {
      await this.waitSocket();
      const request = this.request++;
      return await new Promise((resolve, reject) => {
        const timer = setTimeout(() => { this.pending.delete(request); reject(new Error('Relay peer deadline exceeded')); }, 15000);
        this.pending.set(request, { peer, resolve, reject, timer });
        try { this.control({ type: 'connect', request, peer }); }
        catch (error) { clearTimeout(timer); this.pending.delete(request); reject(error); }
      });
    })();
    this.dials.set(peer, dial);
    dial.finally(() => { if (this.dials.get(peer) === dial) this.dials.delete(peer); }).catch(() => {});
    return dial;
  }

  send(channel, payload) {
    const entry = this.channels.get(channel);
    if (!entry || !entry.active || !this.authenticated || !this.socket || this.socket.readyState !== 1) throw new Error('Relay peer not ready');
    const bytes = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
    if (bytes.byteLength + 8 > 8 * 1024 * 1024 + 64) throw new Error('Relay payload too large');
    const frame = new Uint8Array(8 + bytes.byteLength);
    new DataView(frame.buffer).setBigUint64(0, BigInt(channel), false);
    frame.set(bytes, 8);
    if (this.socket.bufferedAmount + frame.byteLength > 16 * 1024 * 1024) { this.closeChannel(channel); throw new Error('Relay send budget exhausted'); }
    // A submitted message is never replayed on reconnect; its delivery may
    // already have happened even if the socket closes immediately afterwards.
    this.socket.send(frame);
  }

  removeChannel(channel) {
    const entry = this.channels.get(channel);
    if (!entry) return;
    clearTimeout(entry.handshakeTimer);
    const pending = this.pending.get(entry.request);
    if (pending) { clearTimeout(pending.timer); this.pending.delete(entry.request); pending.reject(new Error('Relay encrypted handshake closed')); }
    this.channels.delete(channel);
    if (this.peers.get(entry.peer) === channel) this.peers.delete(entry.peer);
    entry.ready = false;
    entry.active = false;
    if (entry.handler.closed) Promise.resolve(entry.handler.closed()).catch(this.onError);
  }

  closeChannel(channel) {
    this.removeChannel(channel);
    try { this.control({ type: 'close', channel }); }
    catch (_) {
      // A close that cannot enter the bounded queue would leave an accepted
      // server channel behind. Retire this socket and release its channels.
      if (this.finishCycle) this.finishCycle();
    }
  }

  stop() {
    this.stopped = true;
    this.desired.clear();
    if (this.finishCycle) this.finishCycle();
    clearTimeout(this.delayTimer);
    if (this.resumeDelay) this.resumeDelay();
    this.wake();
  }
}
// @relay-client end

// @relay-adapter begin
// Trusted AIS I/O stays in the worker; proof bodies never cross the DOM bridge.
globalThis.__actrRelayTrustedRequest = async function(base, path, body, signaling, proof) {
  const url = new URL(base);
  const secure = new URL(signaling).protocol === 'wss:';
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash || (secure && url.protocol !== 'https:')) throw new Error('Relay encryption requires trusted HTTPS AIS');
  url.pathname = url.pathname.replace(/\/$/, '') + '/' + path;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 5000);
  try {
    const response = await fetch(url, {method: body === null ? 'GET' : 'POST', body: body === null ? undefined : body, headers: body === null ? {} : {'Content-Type':'application/json', 'x-actr-client-proof':proof}, redirect:'error', signal:controller.signal});
    if (!response.ok || !response.body) throw new Error('Trusted AIS request rejected');
    const reader = response.body.getReader();
    const parts = []; let size = 0;
    try {
      while (true) {
        const {value,done} = await reader.read(); if (done) break;
        size += value.byteLength; if (size > 32768) throw new Error('Trusted AIS response exceeds limit');
        parts.push(value);
      }
    } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
    const bytes = new Uint8Array(size); let offset = 0;
    for (const part of parts) { bytes.set(part,offset); offset += part.byteLength; }
    return new TextDecoder('utf-8',{fatal:true}).decode(bytes);
  } finally { clearTimeout(timer); }
};

const relayClients = new Map();

function startRelayForClient(clientId) {
  if (relayClients.has(clientId)) return relayClients.get(clientId);
  const relay = new LoranRelayClient({
    credentials: () => wasm_bindgen.relay_connection_info(clientId),
    discover: async () => {
      const info = await wasm_bindgen.relay_connection_info(clientId);
      const url = new URL(info.signalingUrl);
      if (!['ws:', 'wss:'].includes(url.protocol)) throw new Error('Invalid signaling URL');
      const secure = url.protocol === 'wss:';
      url.protocol = secure ? 'https:' : 'http:';
      url.pathname = '/relay/config'; url.search = ''; url.hash = '';
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 5000);
      try {
        const response = await fetch(url, { signal: controller.signal, redirect: 'error' });
        if (response.status === 404) return null;
        if (!response.ok) throw new Error('Relay discovery failed');
        if (!response.body) throw new Error('Empty relay discovery');
        const reader = response.body.getReader();
        const chunks = [];
        let size = 0;
        try {
          while (true) {
            const { value, done } = await reader.read();
            if (done) break;
            size += value.byteLength;
            if (size > 32768) throw new Error('Relay discovery exceeds size limit');
            chunks.push(value);
          }
        } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
        const bytes = new Uint8Array(size);
        let offset = 0;
        for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
        const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
        const config = JSON.parse(text);
        if (config.version !== 1) throw new Error('Unsupported relay discovery version');
        if (!config.enabled && !config.url) return null;
        url.protocol = secure ? 'wss:' : 'ws:'; url.pathname = '/relay/ws';
        const endpoint = config.url ? new URL(config.url) : url;
        if (!['ws:', 'wss:'].includes(endpoint.protocol) || (secure && endpoint.protocol !== 'wss:') || endpoint.username || endpoint.password || endpoint.search || endpoint.hash) throw new Error('Invalid advertised relay URL');
        return endpoint.href;
      } finally { clearTimeout(timer); }
    },
    prepare: async (channel, endpoint, key) => {
      const rawSend = (payload) => {
        if (relay.channels.get(channel)?.key !== key) throw new Error('Stale encrypted relay channel');
        if (payload === null) relay.closeChannel(channel);
        else relay.send(channel, payload);
      };
      const peer = await wasm_bindgen.prepare_relay_connection(clientId, endpoint, key, rawSend);
      let announced = false;
      const notify = (connected) => {
        try {
          clientPorts.get(clientId)?.postMessage({ type: 'webrtc_event', payload: {
            eventType: 'relay_state_changed', data: { peerId: peer, connected },
          } });
        } catch (_) { /* DOM detached; the worker still owns the connection. */ }
      };
      return {
        ready: () => wasm_bindgen.start_relay_handshake(clientId, key),
        receive: async (payload) => {
          const ready = await wasm_bindgen.handle_relay_record(clientId, key, payload);
          if (ready && !announced) { announced = true; notify(true); }
          return ready;
        },
        closed: async () => {
          await wasm_bindgen.relay_channel_closed(clientId, peer, key);
          if (!relay.peers.has(endpoint)) notify(false);
        },
      };
    },
    onError: (error) => emitSwLog('debug', 'relay_connection_event', String(error)),
  });
  relayClients.set(clientId, relay);
  relay.start().catch((error) => emitSwLog('warn', 'relay_start_failed', String(error)));
  return relay;
}

globalThis.__actrRelayEnsure = (clientId, peer) => {
  startRelayForClient(clientId).ensurePeer(peer).catch((error) => emitSwLog('debug', 'relay_peer_pending', String(error)));
};
globalThis.__actrRelayStop = (clientId) => {
  const relay = relayClients.get(clientId);
  relayClients.delete(clientId);
  if (relay) relay.stop();
};
// @relay-adapter end

const clientPorts = new Map();
const browserToSwClient = new Map();
const clientRegistrations = new Map();
const pendingRegistrations = new Set();
let staleCleanupTimer = null;

function releaseUnusedKeyBinding() {
  if (clientRegistrations.size || pendingRegistrations.size) return;
  // Keep the public configuration: the verified workload is still loaded.
  // Only its unused key binding may be replaced by a subsequent import.
  if (RUNTIME_CONFIG) delete RUNTIME_CONFIG.realm_client_key;
  RUNTIME_CLIENT_KEY_ID = null;
}

function detachClient(registration) {
  const { clientId, port } = registration;
  if (clientRegistrations.get(clientId) !== registration) return;
  clientRegistrations.delete(clientId);
  if (clientPorts.get(clientId) === port) clientPorts.delete(clientId);
  for (const [browserId, swClientId] of browserToSwClient.entries()) {
    if (swClientId === clientId) browserToSwClient.delete(browserId);
  }
}

async function unregisterClient(clientId, expected) {
  const registration = clientRegistrations.get(clientId);
  if (!registration || (expected && registration !== expected)) return;
  if (registration.cleanup) return registration.cleanup;
  registration.cancelled = true;
  // A registration can finish after its page closes. Its completion path
  // must unregister that runtime instead of publishing a late runtime_ready.
  registration.cleanup = (async () => {
    await registration.done;
    if (clientRegistrations.get(clientId) !== registration) return;
    if (wasmReady) await wasm_bindgen.unregister_client(clientId);
    detachClient(registration);
    registration.port.close();
    releaseUnusedKeyBinding();
  })();
  try {
    await registration.cleanup;
  } catch (error) {
    registration.cleanup = null;
    throw error;
  }
}

async function registerClient(registration) {
  const { clientId, config, port } = registration;
  let failure;
  try {
    await ensureWasmReady();
    if (registration.cancelled) return;
    const clientKeyId = await wasm_bindgen.register_client(clientId, config, port);
    if (registration.cancelled) return;
    if (typeof clientKeyId !== 'string' || !clientKeyId) {
      throw new Error('[SW] Host did not return a Realm client key ID');
    }
    if (RUNTIME_CLIENT_KEY_ID !== null && RUNTIME_CLIENT_KEY_ID !== clientKeyId) {
      throw new Error('[SW] Host registered a different Realm client key');
    }
    // Imported and IndexedDB-restored clients must share the same binding.
    // Use the validated Host result rather than guessing from the input.
    RUNTIME_CLIENT_KEY_ID = clientKeyId;
    registration.registered = true;
    startRelayForClient(clientId);
    delete config.realm_client_key;
    console.log('[SW] Client registered (WBG):', clientId);
    emitSwLog('info', 'client_registered', { clientId, variant: 'wbg' });
    port.postMessage({ type: 'runtime_ready' });
  } catch (error) {
    failure = error && error.message ? error.message : String(error);
    console.error('[SW] register_client failed:', error);
    emitSwLog('error', 'register_client_failed', { clientId, error: String(error) });
  } finally {
    if (!registration.registered || registration.cancelled) {
      try {
        if (wasmReady) await wasm_bindgen.unregister_client(clientId);
        detachClient(registration);
      } catch (error) {
        // Retain the binding if cleanup cannot confirm the runtime is gone.
        console.error('[SW] failed registration cleanup:', error);
      }
    }
    pendingRegistrations.delete(registration);
    releaseUnusedKeyBinding();
    if (registration.cancelled) port.close();
  }
  if (failure !== undefined && !registration.cancelled) {
    try {
      port.postMessage({ type: 'runtime_error', error: failure });
      port.close();
    } catch (_) {
      /* page may already be gone */
    }
  }
}

async function cleanupStaleClients() {
  if (!wasmReady) return;
  try {
    const activeWindows = await self.clients.matchAll({ type: 'window' });
    const activeIds = new Set(activeWindows.map((c) => c.id));
    for (const [browserId, swClientId] of browserToSwClient.entries()) {
      if (!activeIds.has(browserId)) {
        console.log('[SW] Cleaning up stale client:', swClientId, '(browser:', browserId, ')');
        try {
          await unregisterClient(swClientId);
        } catch (e) {
          console.warn('[SW] unregister_client error for', swClientId, ':', e);
        }
      }
    }
  } catch (e) {
    console.warn('[SW] cleanupStaleClients error:', e);
  }
}

function scheduleStaleClientCleanup(delayMs = 0) {
  if (staleCleanupTimer) {
    clearTimeout(staleCleanupTimer);
  }
  staleCleanupTimer = setTimeout(() => {
    staleCleanupTimer = null;
    cleanupStaleClients();
  }, delayMs);
}

function emitSwLog(level, message, detail) {
  for (const port of clientPorts.values()) {
    try {
      port.postMessage({
        type: 'webrtc_event',
        payload: {
          eventType: 'sw_log',
          data: { level, message, detail },
        },
      });
    } catch (_) {
      /* port may be closed */
    }
  }
}

// ─────────────────────────────────────────────────────────────────────────
// Schema adapters between sw-host (camelCase) and actr-web-abi guest
// (kebab-case, from serde with `#[serde(rename = "...")]`).
//
// sw-host's `actr_id_to_js` emits `{ realm, serialNumber }` (camelCase, the
// WBG guest's `serde-wasm-bindgen` shape; `realm` is the Realm name).
// actr-web-abi `ActrId` deserialises from `{ realm, "serial-number" }`.
// ─────────────────────────────────────────────────────────────────────────

function actrIdCamelToKebab(id) {
  if (id == null) return id;
  return {
    realm: id.realm,
    'serial-number': id.serialNumber,
  };
}

function actrIdKebabToCamel(id) {
  if (id == null) return id;
  return {
    realm: id.realm,
    serialNumber: id['serial-number'],
  };
}

// ─────────────────────────────────────────────────────────────────────────
// Install the `actrHost*` globals the wasm-bindgen guest imports.
//
// The guest's `.wasm` imports these by bare global name (wasm-pack
// `--target no-modules` resolves them from the enclosing global scope —
// see `echo_*_guest_wbg.js` `__wbg_actrHostCallRaw_*` entries).
//
// Each one is a thin proxy onto `wasm_bindgen.host_*_async` etc. from
// `actr_sw_host.js`, with argument/result reshaping to bridge the
// serde-wasm-bindgen (kebab) <-> hand-written Reflect (camel) gap.
// ─────────────────────────────────────────────────────────────────────────

function installActrHostGlobals() {
  // γ-unified (Option U Phase 6 §3.4/§3.6): every `actrHost*` global now
  // accepts `requestId` as the first argument, threading it into the
  // sw-host `host_*_async` wasm-bindgen imports so the `DISPATCH_CTXS`
  // HashMap can look up the per-dispatch `RuntimeContext` keyed on that
  // id. This lets multiple concurrent dispatches share the single-threaded
  // JS bridge without clobbering each other's runtime context (TD-003).

  self.actrHostCall = async function (requestId, target, routeKey, payload, timeoutMs) {
    // actr-web-abi `call` passes `Dest` variant as `{ peer: {...} }` or
    // `"host"`/`"workload"`; sw-host's `host_call_async` reads `{ tag, val }`.
    // The WBG path is only exercised by the echo client which uses
    // `call_raw`, but keep the shape future-proof.
    let destCamel;
    if (target === 'host' || target === 'workload') {
      destCamel = { tag: target };
    } else if (target && target.peer) {
      destCamel = { tag: 'peer', val: actrIdKebabToCamel(target.peer) };
    } else {
      throw new Error('[WBG] actrHostCall: unknown dest shape ' + JSON.stringify(target));
    }
    const u8 = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
    try {
      const reply = await wasm_bindgen.host_call_async(
        requestId,
        destCamel,
        routeKey,
        u8,
        timeoutMs,
      );
      return { Ok: reply };
    } catch (e) {
      // Host rejects with Error carrying `actrErrorTag`; fold into the
      // guest-expected `Result::Err(ActrError)` shape.
      return { Err: mapHostErrorToActr(e) };
    }
  };

  self.actrHostCallRaw = async function (requestId, target, routeKey, payload, timeoutMs) {
    // `target` is an `ActrId` (kebab) from the guest.
    const targetCamel = actrIdKebabToCamel(target);
    const u8 = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
    try {
      const reply = await wasm_bindgen.host_call_raw_async(
        requestId,
        targetCamel,
        routeKey,
        u8,
        timeoutMs,
      );
      // `reply` is a `Uint8Array` from Rust; guest expects Vec<u8> —
      // serde-wasm-bindgen deserialises Array / Uint8Array / numbers
      // buffer — Uint8Array works directly, but the outer Result
      // needs the `Ok` tag matching `#[serde(rename = "ok")]`?
      // Actually generated `ActrError` variants use kebab tags but
      // `Result<T, E>` is serde's default which emits `{ Ok: ... }`
      // (capitalized). Keep capitalization exact.
      return { Ok: reply };
    } catch (e) {
      return { Err: mapHostErrorToActr(e) };
    }
  };

  self.actrHostDiscover = async function (requestId, targetType) {
    // `targetType` is `ActrType { publisher, name, version }` —
    // identical shape on both sides.
    try {
      const id = await wasm_bindgen.host_discover_async(requestId, targetType);
      return { Ok: actrIdCamelToKebab(id) };
    } catch (e) {
      return { Err: mapHostErrorToActr(e) };
    }
  };

  self.actrHostResolveDependency = async function (requestId, alias) {
    // Resolver access is scoped to a live invocation so stale guests cannot
    // query the dependency lock installed for a later workload.
    try {
      const actrType = await wasm_bindgen.host_resolve_dependency(requestId, alias);
      return { Ok: actrType };
    } catch (e) {
      return { Err: mapHostErrorToActr(e) };
    }
  };

  self.actrHostGetCallerId = async function (requestId) {
    try {
      const id = wasm_bindgen.host_get_caller_id(requestId);
      if (id == null || id === undefined) return null;
      return actrIdCamelToKebab(id);
    } catch (e) {
      console.warn('[WBG] host_get_caller_id threw:', e);
      return null;
    }
  };

  self.actrHostGetRequestId = async function (requestId) {
    try {
      return wasm_bindgen.host_get_request_id(requestId);
    } catch (e) {
      console.warn('[WBG] host_get_request_id threw:', e);
      return '';
    }
  };

  self.actrHostGetSelfId = async function (requestId) {
    const id = wasm_bindgen.host_get_self_id(requestId);
    return actrIdCamelToKebab(id);
  };

  self.actrHostLogMessage = async function (requestId, level, message) {
    wasm_bindgen.host_log_message(requestId, level, message);
  };

  self.actrHostTell = async function (requestId, target, routeKey, payload) {
    let destCamel;
    if (target === 'host' || target === 'workload') {
      destCamel = { tag: target };
    } else if (target && target.peer) {
      destCamel = { tag: 'peer', val: actrIdKebabToCamel(target.peer) };
    } else {
      throw new Error('[WBG] actrHostTell: unknown dest shape ' + JSON.stringify(target));
    }
    const u8 = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
    try {
      await wasm_bindgen.host_tell_async(requestId, destCamel, routeKey, u8);
      return { Ok: null };
    } catch (e) {
      return { Err: mapHostErrorToActr(e) };
    }
  };

  console.log('[SW][WBG] actrHost* globals installed (γ-unified, request_id-threaded)');
}

// @host-error-map begin
// Extraction markers for web-sdk/src/host-error-map.test.js: this script is a
// standalone classic service worker, so its tests load this exact function
// body from the shipped bytes. The CLI-embedded runtime copy under
// crates/actr-cli/assets/web-runtime must stay byte-identical (checked by
// sync-cli-assets.sh --check).
function mapHostErrorToActr(e) {
  // sw-host's `actr_error_to_js` attaches `actrErrorTag` + message.
  // actr-web-abi `ActrError` variant serde tags are kebab-case; we
  // return the matching `{ "kebab-tag": payload }` shape.
  const tag = (e && e.actrErrorTag) || 'internal';
  const msg = (e && e.actrMessage) || (e && e.message) || String(e);
  // `timed-out` is a unit variant; serde emits it as a bare string.
  if (tag === 'timed-out') return 'timed-out';
  if (tag === 'connection-not-ready') {
    const retryAfterMs = Number.isFinite(e && e.actrRetryAfterMs) ? e.actrRetryAfterMs : null;
    return { 'connection-not-ready': { 'retry-after-ms': retryAfterMs } };
  }
  if (tag === 'dependency-not-found') {
    return {
      'dependency-not-found': {
        'service-name': (e && e.actrServiceName) || '',
        message: msg,
      },
    };
  }
  if (tag === 'delivery-uncertain') {
    return {
      'delivery-uncertain': {
        'operation-id': (e && e.actrOperationId) || '',
        cause: msg,
      },
    };
  }
  return { [tag]: msg };
}
// @host-error-map end

// ─────────────────────────────────────────────────────────────────────────
// Main bootstrap: load sw-host, load WBG guest, bridge them.
// ─────────────────────────────────────────────────────────────────────────

async function loadWithGuestBridge(packageUrl, runtimeWasmUrl) {
  emitSwLog('info', 'guest_bridge_start', { packageUrl, runtimeWasmUrl });

  // ── 1. Load sw-host WASM + JS glue (unchanged from CM path) ──
  const jsUrl = runtimeWasmUrl.replace(/_bg\.wasm$/, '.js');
  const jsResp = await fetch(jsUrl, { cache: 'no-store' });
  if (!jsResp.ok) {
    throw new Error('[SW] Failed to fetch runtime JS glue: ' + jsResp.status);
  }
  const jsText = await jsResp.text();
  const patchedText = jsText.replace('let wasm_bindgen =', 'self.wasm_bindgen =');
  (0, eval)(patchedText);
  emitSwLog('info', 'guest_bridge_runtime_js_loaded', jsText.length);

  await wasm_bindgen({ module_or_path: runtimeWasmUrl });
  wasm_bindgen.init_global();
  // A replacement package starts from an empty dependency snapshot. Failed
  // verification or lock parsing must not retain aliases from an old package.
  wasm_bindgen.install_dependency_lock(undefined);
  emitSwLog('info', 'guest_bridge_runtime_ready', null);

  // ── 2. Install actrHost* globals BEFORE guest instantiation ──
  // The guest wasm resolves imports at instantiate time; globals must
  // exist up-front.
  installActrHostGlobals();

  // ── 3. Verify + extract .actr (keeps the mandatory-verify contract) ──
  const resp = await fetch(packageUrl, { cache: 'no-store' });
  if (!resp.ok) {
    throw new Error('[SW] Failed to fetch .actr package: ' + resp.status);
  }
  const buffer = await resp.arrayBuffer();
  emitSwLog('info', 'guest_bridge_actr_size', buffer.byteLength);

  const trustJson = JSON.stringify(
    RUNTIME_CONFIG && Array.isArray(RUNTIME_CONFIG.trust) ? RUNTIME_CONFIG.trust : []
  );
  let extracted;
  try {
    extracted = wasm_bindgen.verify_and_extract_actr_package(
      new Uint8Array(buffer),
      trustJson
    );
    const verifiedActrType = verifiedPackageActrType(extracted);
    if (!RUNTIME_CONFIG || RUNTIME_CONFIG.client_actr_type !== verifiedActrType) {
      throw new Error(
        `[SW] Verified package identity ${verifiedActrType} does not match configured runtime identity ` +
          `${RUNTIME_CONFIG && RUNTIME_CONFIG.client_actr_type}`
      );
    }
    wasm_bindgen.install_dependency_lock(extracted.lock_file_toml);
    emitSwLog('info', 'guest_bridge_verify_ok', null);
  } catch (verifyError) {
    emitSwLog('error', 'guest_bridge_verify_failed', String(verifyError));
    throw verifyError;
  }

  // ── 4. Evaluate the authenticated wasm-bindgen glue from the package.
  const wbgSrc = extracted.glue_js;
  if (typeof wbgSrc !== 'string' || wbgSrc.length === 0) {
    throw new Error('[SW] Verified web package does not contain resources/glue.js');
  }
  // wasm-pack `--target no-modules` emits `let wasm_bindgen = (function(exports) {...})({});`.
  // Rewrite to `self.actrGuestBindgen = ...` so the glue coexists with
  // sw-host (also keyed to `self.wasm_bindgen`).
  const patchedGuestSrc = wbgSrc.replace(/^\s*let\s+wasm_bindgen\s*=/m, 'self.actrGuestBindgen =');
  if (patchedGuestSrc === wbgSrc) {
    throw new Error(
      '[SW] WBG guest JS does not match expected `let wasm_bindgen =` preamble; refusing to eval'
    );
  }
  (0, eval)(patchedGuestSrc);
  if (typeof self.actrGuestBindgen !== 'function') {
    throw new Error('[SW] actrGuestBindgen init not installed after eval');
  }

  // ── 5. Instantiate the authenticated package binary directly.
  await self.actrGuestBindgen({ module_or_path: extracted.binary });
  emitSwLog('info', 'guest_bridge_guest_instantiated', Object.keys(self.actrGuestBindgen));

  // ── 7. Build dispatchFn that adapts sw-host envelope (camelCase) to
  // the actr-web-abi guest (kebab-case + Vec<u8>).
  const dispatchFn = async (envelope) => {
    // sw-host builds `{ requestId, routeKey, payload: Uint8Array, timeoutMs: bigint }`.
    // actr-web-abi `RpcEnvelope` uses kebab-case field names.
    const kebabEnv = {
      'request-id': envelope.requestId,
      'route-key': envelope.routeKey,
      // serde-wasm-bindgen reads Vec<u8> from Uint8Array fine, but
      // also accepts plain arrays. Uint8Array is the natural form.
      payload: envelope.payload,
      'timeout-ms': envelope.timeoutMs,
    };
    const result = await self.actrGuestBindgen.dispatch(kebabEnv);
    // actr-web-abi `dispatch` returns `Result<Vec<u8>, ActrError>` —
    // serde-wasm-bindgen encodes `Ok(bytes)` as `{ Ok: [...] }`.
    if (result && Object.prototype.hasOwnProperty.call(result, 'Ok')) {
      const ok = result.Ok;
      if (ok instanceof Uint8Array) return ok;
      if (Array.isArray(ok)) return new Uint8Array(ok);
      throw new Error('[WBG] dispatch Ok was not bytes-like: ' + typeof ok);
    }
    if (result && Object.prototype.hasOwnProperty.call(result, 'Err')) {
      throw new Error('[WBG] guest dispatch returned Err: ' + JSON.stringify(result.Err));
    }
    // Older serde shapes or direct Uint8Array — accept as last resort.
    if (result instanceof Uint8Array) return result;
    throw new Error('[WBG] guest dispatch returned unexpected shape: ' + JSON.stringify(result));
  };

  wasm_bindgen.register_guest_workload(dispatchFn);
  emitSwLog('info', 'guest_bridge_ready', 'WBG workload registered');
}

async function ensureWasmReady() {
  if (wasmReady) return;
  if (wasmInitPromise) return wasmInitPromise;

  wasmInitPromise = initializeWasm();
  try {
    await wasmInitPromise;
  } catch (error) {
    // A failed first configuration must not permanently poison this worker.
    // No workload has been registered successfully at this point.
    RUNTIME_CONFIG = null;
    RUNTIME_CLIENT_KEY_ID = null;
    wasmInitPromise = null;
    throw error;
  }
}

async function initializeWasm() {

  if (!RUNTIME_CONFIG) {
    throw new Error('[SW] Cannot load WASM: RUNTIME_CONFIG not yet received');
  }

  const packageUrl = RUNTIME_CONFIG.package_url;
  const runtimeWasmUrl = RUNTIME_CONFIG.runtime_wasm_url;

  try {
    if (!wsProbeDone) {
      wsProbeDone = true;
      try {
        emitSwLog('info', 'ws_probe_start', RUNTIME_CONFIG.signaling_url);
        const probe = new WebSocket(RUNTIME_CONFIG.signaling_url);
        probe.binaryType = 'arraybuffer';
        probe.onopen = () => {
          emitSwLog('info', 'ws_probe_open', null);
          probe.close();
        };
        probe.onerror = () => {
          emitSwLog('error', 'ws_probe_error', null);
        };
        probe.onclose = (event) => {
          emitSwLog('info', 'ws_probe_close', {
            code: event.code,
            reason: event.reason,
            wasClean: event.wasClean,
          });
        };
      } catch (error) {
        emitSwLog('error', 'ws_probe_throw', String(error));
      }
    }

    if (!runtimeWasmUrl || !packageUrl) {
      throw new Error('[SW] RUNTIME_CONFIG requires both `runtime_wasm_url` and `package_url`');
    }
    await loadWithGuestBridge(packageUrl, runtimeWasmUrl);

    wasmReady = true;
    emitSwLog('info', 'wasm_ready', null);
  } catch (error) {
    console.error(
      '[SW] WASM init failed:',
      error && error.message ? error.message : String(error),
      'name=' + (error && error.name),
      'stack=' + (error && error.stack)
    );
    emitSwLog('error', 'wasm_init_failed', {
      error: error && error.message ? error.message : String(error),
      name: error && error.name ? error.name : undefined,
      stack: error && error.stack ? error.stack : undefined,
      packageUrl: packageUrl || null,
      runtimeWasmUrl: runtimeWasmUrl || null,
    });
    throw error;
  }
}

self.addEventListener('install', (event) => {
  console.log('[SW] installing (WBG variant)...');
  event.waitUntil(self.skipWaiting());
});

self.addEventListener('activate', (event) => {
  console.log('[SW] activated (WBG variant)');
  event.waitUntil(self.clients.claim());
});

self.addEventListener('message', async (event) => {
  if (event.data && event.data.type === 'PING') {
    if (event.source && event.source.postMessage) {
      event.source.postMessage({ type: 'PONG' });
    }
    return;
  }

  if (event.data && event.data.type === 'CLIENT_UNREGISTER') {
    const clientId = event.data.clientId;
    if (!clientId) return;
    try {
      await unregisterClient(clientId);
    } catch (e) {
      console.warn('[SW] top-level unregister_client error for', clientId, ':', e);
    }
    emitSwLog('info', 'client_unregistered', { clientId, variant: 'wbg', source: 'top-level' });
    return;
  }

  if (!event.data || event.data.type !== 'DOM_PORT_INIT') {
    return;
  }

  const port = event.data.port;
  const clientId = event.data.clientId;
  if (!port || !clientId) return;
  if (clientRegistrations.has(clientId)) {
    port.postMessage({ type: 'runtime_error', error: '[SW] Client ID is already in use' });
    port.close();
    return;
  }

  if (event.data.runtimeConfig) {
    let candidateConfig;
    try {
      candidateConfig = validateRuntimeConfig(event.data.runtimeConfig);
    } catch (error) {
      try {
        port.postMessage({
          type: 'runtime_error',
          error: error && error.message ? error.message : String(error),
        });
        port.close();
      } catch (_) {
        /* ignore */
      }
      return;
    }
    if (!RUNTIME_CONFIG) {
      RUNTIME_CONFIG = candidateConfig;
      RUNTIME_CLIENT_KEY_ID = candidateConfig.realm_client_key?.key_id ?? null;
    } else if (
      !clientRegistrations.size &&
      !pendingRegistrations.size &&
      runtimeConfigsEqual({ ...candidateConfig, realm_client_key: undefined }, RUNTIME_CONFIG)
    ) {
      // A rejected/cancelled import has no remaining users. Rebind only the
      // key; a different workload, Realm or Issuer still requires a new worker.
      RUNTIME_CONFIG = candidateConfig;
      RUNTIME_CLIENT_KEY_ID = candidateConfig.realm_client_key?.key_id ?? null;
    } else if (!runtimeConfigsEqual(candidateConfig, RUNTIME_CONFIG, RUNTIME_CLIENT_KEY_ID)) {
      const message =
        '[SW] This Service Worker is already bound to a different runtime/package configuration';
      emitSwLog('error', 'runtime_config_mismatch', { clientId, message });
      try {
        port.postMessage({ type: 'runtime_error', error: message });
        port.close();
      } catch (_) {
        /* ignore */
      }
      return;
    }
  }

  let startRegistration;
  const registration = { clientId, port, config: RUNTIME_CONFIG, cancelled: false, registered: false };
  registration.done = new Promise((resolve) => {
    startRegistration = resolve;
  }).then(() => registerClient(registration));
  clientRegistrations.set(clientId, registration);
  pendingRegistrations.add(registration);

  const browserId = event.source && event.source.id;
  if (browserId) {
    const previousClientId = browserToSwClient.get(browserId);
    if (previousClientId && previousClientId !== clientId) {
      console.log(
        '[SW] browser client remapped, unregistering previous client:',
        previousClientId,
        'browser:',
        browserId
      );
      try {
        await unregisterClient(previousClientId);
      } catch (e) {
        console.warn('[SW] remap unregister_client error for', previousClientId, ':', e);
      }
    }
  }

  clientPorts.set(clientId, port);
  if (browserId) {
    browserToSwClient.set(browserId, clientId);
  }

  cleanupStaleClients();
  scheduleStaleClientCleanup(1500);

  console.log('[SW] port initialized (WBG) for client:', clientId, 'total:', clientPorts.size);

  if (event.source && event.source.postMessage) {
    event.source.postMessage({ type: 'sw_ack', message: 'port_ready' });
  }

  emitSwLog('info', 'sw_env', {
    clientId,
    variant: 'wbg',
    location: self.location ? self.location.href : null,
    totalClients: clientPorts.size,
  });

  let portMessageChain = Promise.resolve();

  async function processPortMessage(message) {
    if (message && message.type === 'unregister_client') {
      try {
        await unregisterClient(clientId, registration);
        emitSwLog('info', 'client_unregistered', { clientId, variant: 'wbg' });
      } catch (error) {
        console.error('[SW] unregister_client failed:', error);
        emitSwLog('error', 'unregister_client_failed', { clientId, error: String(error) });
      }
      return;
    }
    try {
      await ensureWasmReady();
    } catch (error) {
      console.error('[SW] WASM not ready:', error);
      return;
    }

    if (!message || !message.type) return;

    switch (message.type) {
      case 'control':
        try {
          await wasm_bindgen.handle_dom_control(clientId, message.payload);
        } catch (error) {
          console.error('[SW] handle_dom_control failed:', error);
          emitSwLog('error', 'handle_dom_control_failed', String(error));
        }
        break;

      case 'webrtc_event':
        try {
          await wasm_bindgen.handle_dom_webrtc_event(clientId, message.payload);
        } catch (error) {
          console.error('[SW] handle_dom_webrtc_event failed:', error);
          emitSwLog('error', 'handle_dom_webrtc_event_failed', String(error));
        }
        break;

      case 'fast_path_data':
        try {
          await wasm_bindgen.handle_dom_fast_path(clientId, message.payload);
        } catch (error) {
          console.error('[SW] handle_dom_fast_path failed:', error);
          emitSwLog('error', 'handle_dom_fast_path_failed', String(error));
        }
        break;

      case 'register_datachannel_port':
        try {
          const dcPort = message.payload.port;
          const dcPeerId = message.payload.peerId;
          const dcConnectionId = message.payload.connectionId;
          if (dcPort && dcPeerId && dcConnectionId) {
            await wasm_bindgen.register_datachannel_port(clientId, dcPeerId, dcConnectionId, dcPort);
          } else {
            console.warn('[SW] register_datachannel_port: missing port, peerId, or connectionId');
          }
        } catch (error) {
          console.error('[SW] register_datachannel_port failed:', error);
          emitSwLog('error', 'register_datachannel_port_failed', String(error));
        }
        break;

      default:
        console.log('[SW] unknown message type:', message.type);
        break;
    }
  }

  port.onmessage = (portEvent) => {
    const message = portEvent.data;
    portMessageChain = portMessageChain
      .then(() => processPortMessage(message))
      .catch((error) => {
        console.error('[SW] port message pipeline failed:', error);
        emitSwLog('error', 'port_message_pipeline_failed', String(error));
      });
  };

  port.start();

  startRegistration();
});
