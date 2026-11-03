// Run compiled wasm-bindgen tests in a real browser. --interop additionally
// starts the native fixture and exposes the canonical worker relay adapter.
// Requires the matching wasm-bindgen-test-runner and @playwright/test.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createServer, request as httpRequest } from 'node:http';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';

const wasm = process.argv[2];
assert.ok(wasm, 'Pass the compiled test .wasm path');
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
const children = [];
let browser;
let fixtureServer;
let fixture;
let runnerOrigin;

function firstLine(child, match, timeout = 30_000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Child readiness deadline exceeded')), timeout);
    const lines = createInterface({ input: child.stdout });
    const finish = (error, value) => {
      clearTimeout(timer);
      lines.close();
      child.off('exit', onExit);
      child.off('error', onError);
      if (error) reject(error); else resolve(value);
    };
    const onExit = (code) => finish(new Error(`Child exited before readiness: ${code}`));
    const onError = (error) => finish(error);
    child.on('exit', onExit);
    child.on('error', onError);
    lines.on('line', (line) => {
      const value = match(line);
      if (value) finish(null, value);
    });
  });
}

try {
  let fixtureOrigin;
  if (process.argv.includes('--interop')) {
    assert.ok(process.env.ACTR_RELAY_FIXTURE, 'Set ACTR_RELAY_FIXTURE to error-fixture');
    fixture = spawn(process.env.ACTR_RELAY_FIXTURE, ['--relay-test', '--pause-rtc'], {
      stdio: ['pipe', 'pipe', 'inherit'],
    });
    children.push(fixture);
    const info = await firstLine(fixture, (line) => {
      try { const value = JSON.parse(line); return value.port ? value : null; }
      catch { return null; }
    });
    const worker = readFileSync(new URL('../../packages/web-sdk/src/actor.sw.js', import.meta.url), 'utf8');
    function section(name) {
      const begin = worker.indexOf(`// @${name} begin`);
      const end = worker.indexOf(`// @${name} end`, begin);
      assert.ok(begin >= 0 && end > begin);
      return worker.slice(begin, end);
    }
    const adapter = `
      let wasm_bindgen;
      const clientPorts = new Map();
      const emitSwLog = (...args) => console.debug(...args);
      const observed = { sentRecords: 0, receivedRecords: 0, scopedHellos: 0, invalidHellos: 0, plaintextLeaks: 0, invalidRecords: 0 };
      function inspectRecord(data, direction) {
        const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
        observed[direction]++;
        if (new TextDecoder().decode(bytes.subarray(8,16)) !== 'ACTRE2E1') observed.invalidRecords++;
        // These known application markers span multiple encrypted fragments.
        for (const marker of [0xA5, 0xB6]) {
          let run = 0;
          for (const byte of bytes) { run = byte === marker ? run + 1 : 0; if (run === 32) { observed.plaintextLeaks++; break; } }
        }
      }
      // Observe exactly the bytes visible to relay, outside SDK encryption and
      // inside hop TLS. No production transport hook or payload logging added.
      const WebSocket = class extends globalThis.WebSocket {
        constructor(...args) {
          super(...args);
          this.addEventListener('message', event => { if (typeof event.data !== 'string') inspectRecord(event.data, 'receivedRecords'); });
        }
        send(data) {
          if (typeof data !== 'string') inspectRecord(data, 'sentRecords');
          else {
            const control = JSON.parse(data);
            if (control.type === 'hello') {
              try {
                const { certificate } = JSON.parse(atob(control.token));
                const claims = JSON.parse(new TextDecoder().decode(new Uint8Array(certificate.claims)));
                if (claims.domain !== 'ACTR-RELAY-IDENTITY-V2' || claims.endpoint !== control.endpoint) throw new Error('Wrong certificate scope');
                observed.scopedHellos++;
              } catch (_) { observed.invalidHellos++; }
            }
          }
          super.send(data);
        }
      };
      ${section('relay-client')}
      ${section('relay-adapter')}
      export function start(runtime, client, port) {
        wasm_bindgen = runtime;
        clientPorts.set(client, port);
        return startRelayForClient(client);
      }
      export function stop(client) { globalThis.__actrRelayStop(client); }
      export function security_observations() { return { ...observed }; }
    `;
    fixtureServer = createServer((request, response) => {
      response.setHeader('Access-Control-Allow-Origin', '*');
      response.setHeader('Cache-Control', 'no-store');
      if (request.url === '/fixture') {
        response.setHeader('Content-Type', 'application/json');
        response.end(JSON.stringify(info));
      } else if (request.url === '/adapter.js') {
        response.setHeader('Content-Type', 'text/javascript');
        response.end(adapter);
      } else if (request.url === '/disconnect' && request.method === 'POST') {
        fixture.stdin.write('disconnect-relay\n');
        response.end('ok');
      } else {
        if (!runnerOrigin) { response.writeHead(503).end(); return; }
        const target = new URL(request.url, runnerOrigin);
        const upstream = httpRequest(target, {
          method: request.method,
          headers: { ...request.headers, host: target.host },
        }, (incoming) => {
          response.writeHead(incoming.statusCode, incoming.headers);
          incoming.pipe(response);
        });
        upstream.on('error', () => response.writeHead(502).end());
        request.pipe(upstream);
      }
    });
    await new Promise((resolve) => fixtureServer.listen(0, '127.0.0.1', resolve));
    fixtureOrigin = `http://127.0.0.1:${fixtureServer.address().port}`;
  }
  const runnerArgs = process.argv.includes('--interop') ? [wasm, '--nocapture'] : [wasm];
  const runner = spawn(process.env.WASM_BINDGEN_TEST_RUNNER || 'wasm-bindgen-test-runner', runnerArgs, {
    env: { ...process.env, NO_HEADLESS: '1', WASM_BINDGEN_TEST_ADDRESS: '127.0.0.1:0' },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  children.push(runner);
  const origin = await firstLine(runner, (line) => line.match(/available at (http:\/\/\S+)/)?.[1]);
  runnerOrigin = origin;
  // One origin serves the runner and fixture endpoints so the same test can
  // execute inside a Service Worker, whose location has no page query string.
  const target = new URL(fixtureOrigin || origin);
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.TEST_BROWSER_PATH || undefined,
    args: ['--no-sandbox'],
  });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => {
    if (message.type() === 'error') console.error(message.text());
    else if (process.argv.includes('--interop') && /relay_|First peer-acknowledged/.test(message.text())) console.log(message.text());
  });
  await page.goto(target.href);
  await page.waitForFunction(() => document.body.textContent.includes('test result:'), null, { timeout: 120_000 });
  const output = await page.locator('body').innerText();
  console.log(output);
  assert.match(output, /test result: ok\. [1-9]\d* passed; 0 failed;/);
  assert.deepEqual(errors, [], 'Browser must not report uncaught exceptions');
} finally {
  if (browser) await browser.close();
  if (fixtureServer) await new Promise((resolve) => fixtureServer.close(resolve));
  if (fixture) fixture.stdin.end();
  for (const child of children) {
    if (child.exitCode === null) child.kill('SIGTERM');
  }
  await Promise.race([
    Promise.all(children.map((child) => child.exitCode !== null ? undefined : new Promise((resolve) => child.once('exit', resolve)))),
    delay(2000),
  ]);
  for (const child of children) {
    if (child.exitCode === null) child.kill('SIGKILL');
  }
}
