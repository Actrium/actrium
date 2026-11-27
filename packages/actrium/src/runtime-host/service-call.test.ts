import { afterEach, describe, expect, spyOn, test } from 'bun:test';

import { callHttpService, callNativeService } from './service-call';

const request = {
  requestId: 'request-1',
  route: 'echo.EchoService.Echo',
  payloadBase64: 'AAEC',
  timeoutMs: 30_000,
};

const url = 'http://localhost/v1/call';
let restoreFetch: (() => void) | undefined;

function mockResponse(body: string, contentType: string, status = 502) {
  const fetch = spyOn(globalThis, 'fetch').mockResolvedValue(
    new Response(body, { status, headers: { 'Content-Type': contentType } })
  );
  restoreFetch = () => fetch.mockRestore();
}

afterEach(() => {
  restoreFetch?.();
  restoreFetch = undefined;
});

describe('HTTP Service calls', () => {
  test('returns the payload from a successful response', async () => {
    mockResponse('{"success":true,"payloadBase64":"AAEC"}', 'application/json', 200);
    expect(await callHttpService(url, request)).toEqual({ success: true, payloadBase64: 'AAEC' });
  });

  test('keeps the entire structured failure, including fields outside error', async () => {
    const body = '{"success":false,"error":{"message":"unavailable","code":"E_ROUTE","details":{"retryAfterMs":25}},"requestId":"upstream-1"}';
    mockResponse(body, 'application/json', 200);
    expect(await callHttpService(url, request)).toEqual({ success: false, message: body });
  });

  test('keeps a multiline failure body verbatim', async () => {
    const body = 'actor offline\nsecond line\n';
    mockResponse(body, 'text/plain');
    expect(await callHttpService(url, request)).toEqual({ success: false, message: body });
  });

  test('keeps a gateway response body even if it is mislabeled as JSON', async () => {
    const body = '<html>gateway unavailable</html>';
    mockResponse(body, 'application/json');
    expect(await callHttpService(url, request)).toEqual({ success: false, message: body });
  });

  test('includes the HTTP status when the error response has no body', async () => {
    mockResponse('', 'application/json');
    expect(await callHttpService(url, request)).toEqual({ success: false, message: 'HTTP 502' });
  });

  test('keeps the code and nested cause on a transport rejection', async () => {
    const error = Object.assign(new Error('fetch failed', { cause: new Error('offline') }), {
      code: 'E_TRANSPORT',
    });
    const fetch = spyOn(globalThis, 'fetch').mockRejectedValue(error);
    restoreFetch = () => fetch.mockRestore();

    const result = await callHttpService(url, request);
    expect(result.success).toBe(false);
    expect(JSON.parse(result.message!)).toMatchObject({
      message: 'fetch failed',
      code: 'E_TRANSPORT',
      cause: { message: 'offline' },
    });
  });
});

test('keeps the native rejection code and upstream details', async () => {
  const userInfo = { code: 'delivery_uncertain', details: { operationId: 'op-1', cause: 'offline' } };
  const error = Object.assign(new Error('delivery uncertain'), {
    code: 'realm_service_call_failed',
    userInfo,
  });
  const result = await callNativeService({ call: async () => { throw error; } }, 'tab-1', request);

  expect(result.success).toBe(false);
  expect(JSON.parse(result.message!)).toMatchObject({
    message: 'delivery uncertain',
    code: 'realm_service_call_failed',
    userInfo,
  });
});

test('carries the native connection binding through a pinned RPC', async () => {
  const id = '88c42cbb-085e-48d3-abbf-efd5621c688a';
  const connection = { action: 'use' as const, id };
  const result = await callNativeService({
    callBound: async (tabId, route, payload, timeout, reloadKey, affinity) => {
      expect([tabId, route, payload, timeout, reloadKey, affinity])
        .toEqual(['tab-1', request.route, request.payloadBase64, request.timeoutMs, 7, connection]);
      return { payloadBase64: 'AAEC', connectionId: id };
    },
  }, 'tab-1', { ...request, connection }, 7);
  expect(result).toEqual({ success: true, payloadBase64: 'AAEC', connectionId: id });
});
