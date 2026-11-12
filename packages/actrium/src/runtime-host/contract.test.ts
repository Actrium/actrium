import { describe, expect, test } from 'bun:test';
import {
  CAPABILITY_TO_HOST_METHOD,
  CAPABILITY_TO_HOST_MODULE,
  FRONT_CAPABILITIES,
} from './capabilities';
import { contract } from './contract';

type InputSchema = {
  schema?: {
    parseInput?: (value: unknown) => unknown;
    parseOutput?: (value: unknown) => unknown;
  };
};

type EventSchema = {
  schema?: {
    parseEvent?: (value: unknown) => unknown;
  };
};

function descriptor(moduleId: string, exportName: string): InputSchema {
  const hostModules = contract.hostModules as Record<
    string,
    Record<string, InputSchema>
  >;
  return hostModules[moduleId][exportName];
}

const service = contract.hostModules['host:service'];
const call = service.call as unknown as InputSchema;
const tell = service.tell as unknown as InputSchema;
const on = service.on as unknown as EventSchema;

describe('host:service contract', () => {
  test('call accepts a valid request and keeps the route opaque', () => {
    const parsed = call.schema?.parseInput?.({
      route: 'echo.EchoService.Echo',
      payloadBase64: 'AAEC',
      timeoutMs: 30000,
    });
    expect(parsed).toEqual({
      route: 'echo.EchoService.Echo',
      payloadBase64: 'AAEC',
      timeoutMs: 30000,
    });
  });

  test('call rejects a missing or empty route', () => {
    expect(() => call.schema?.parseInput?.({ payloadBase64: '' })).toThrow();
    expect(() =>
      call.schema?.parseInput?.({ route: '', payloadBase64: '' })
    ).toThrow();
  });

  test('call rejects invalid base64 payloads', () => {
    expect(() =>
      call.schema?.parseInput?.({ route: 'echo.EchoService.Echo', payloadBase64: 'AAE' })
    ).toThrow();
    expect(() =>
      call.schema?.parseInput?.({ route: 'echo.EchoService.Echo', payloadBase64: '!!!' })
    ).toThrow();
    expect(() =>
      call.schema?.parseInput?.({ route: 'echo.EchoService.Echo', payloadBase64: 'AB=' })
    ).toThrow();
    expect(() =>
      call.schema?.parseInput?.({ route: 'echo.EchoService.Echo', payloadBase64: 'A===' })
    ).toThrow();
  });

  test('call rejects a non-string payload', () => {
    expect(() =>
      call.schema?.parseInput?.({ route: 'echo.EchoService.Echo', payloadBase64: 1 })
    ).toThrow();
  });

  test('call rejects a non-positive timeout', () => {
    expect(() =>
      call.schema?.parseInput?.({
        route: 'echo.EchoService.Echo',
        payloadBase64: '',
        timeoutMs: 0,
      })
    ).toThrow();
  });

  test('call rejects non-finite timeouts', () => {
    expect(() =>
      call.schema?.parseInput?.({
        route: 'echo.EchoService.Echo',
        payloadBase64: '',
        timeoutMs: Number.NaN,
      })
    ).toThrow();
    expect(() =>
      call.schema?.parseInput?.({
        route: 'echo.EchoService.Echo',
        payloadBase64: '',
        timeoutMs: Number.POSITIVE_INFINITY,
      })
    ).toThrow();
  });

  test('call rejects timeouts above the host maximum', () => {
    expect(() =>
      call.schema?.parseInput?.({
        route: 'echo.EchoService.Echo',
        payloadBase64: '',
        timeoutMs: 300_001,
      })
    ).toThrow();
  });

  test('call validates the response payload', () => {
    expect(call.schema?.parseOutput?.({ payloadBase64: 'AAEC' })).toEqual({
      payloadBase64: 'AAEC',
    });
    expect(() => call.schema?.parseOutput?.({ payloadBase64: 1 })).toThrow();
  });

  test('tell rejects invalid base64 payloads', () => {
    expect(() =>
      tell.schema?.parseInput?.({ route: 'echo.EchoService.Echo', payloadBase64: 'AAE' })
    ).toThrow();
  });

  test('on rejects invalid base64 event payloads', () => {
    expect(() =>
      on.schema?.parseEvent?.({ event: 'echo.changed', payloadBase64: 'AAE' })
    ).toThrow();
  });

  test('call output rejects invalid base64 payloads', () => {
    expect(() => call.schema?.parseOutput?.({ payloadBase64: 'AAE' })).toThrow();
  });

  test('tell accepts a valid request and rejects a missing route', () => {
    expect(
      tell.schema?.parseInput?.({ route: 'echo.EchoService.Echo', payloadBase64: '' })
    ).toEqual({ route: 'echo.EchoService.Echo', payloadBase64: '' });
    expect(() => tell.schema?.parseInput?.({ payloadBase64: '' })).toThrow();
  });

  test('on validates the event name and payload', () => {
    expect(on.schema?.parseEvent?.({ event: 'echo.changed', payloadBase64: '' })).toEqual({
      event: 'echo.changed',
      payloadBase64: '',
    });
    expect(() => on.schema?.parseEvent?.({ event: '', payloadBase64: '' })).toThrow();
  });
});

/**
 * The twelve placeholder modules are declared so builds accept their imports
 * and the Host-side boundary schemas exist; the Host still throws on every
 * call (see host-modules.test.ts). Guest-side TypeScript declarations are not
 * generated yet (TODO M2-N6).
 */
const placeholderInputs: ReadonlyArray<
  [moduleId: string, exportName: string, input: unknown]
> = [
  ['host:assets', 'read', { path: 'assets/icon.png' }],
  ['host:clipboard', 'writeText', { text: 'hello' }],
  ['host:notifications', 'show', { title: 'Hi', body: 'There' }],
  ['host:camera', 'capturePhoto', undefined],
  ['host:microphone', 'record', { durationMs: 1000 }],
  ['host:location', 'getCurrentPosition', undefined],
  ['host:external-link', 'open', { url: 'https://example.com' }],
  ['host:navigate', 'navigate', { path: '/settings', mode: 'push' }],
  ['host:file-import', 'pick', { accept: ['.png'], multiple: true }],
  ['host:file-export', 'save', { name: 'notes.txt', base64: 'AAEC' }],
  ['host:share', 'share', { text: 'hello' }],
  ['host:net', 'request', { url: 'https://api.example.com/v1', method: 'GET' }],
  [
    'host:net',
    'request',
    {
      url: 'https://api.example.com/v1',
      method: 'POST',
      bodyBase64: 'AAEC',
    },
  ],
];

describe('placeholder host module contracts', () => {
  test('declares every third-party Front capability as a contract method', () => {
    const hostModules = contract.hostModules as Record<
      string,
      Record<string, { kind?: string }>
    >;

    for (const capability of FRONT_CAPABILITIES) {
      // System capabilities are Host policy data, not Guest-importable
      // contract modules (until a system Host implementation exists).
      if (capability.startsWith('system:')) continue;
      const moduleId = CAPABILITY_TO_HOST_MODULE[capability];
      const method = CAPABILITY_TO_HOST_METHOD[capability];
      const kind = hostModules[moduleId]?.[method]?.kind;
      expect(kind === 'rpc' || kind === 'subscription').toBe(true);
    }
  });

  test('maps the clipboard capability to the writeText contract method', () => {
    expect(CAPABILITY_TO_HOST_MODULE['host:clipboard.write']).toBe(
      'host:clipboard'
    );
    expect(CAPABILITY_TO_HOST_METHOD['host:clipboard.write']).toBe('writeText');
  });

  test('accepts valid input for every placeholder module', () => {
    for (const [moduleId, exportName, input] of placeholderInputs) {
      expect(() =>
        descriptor(moduleId, exportName).schema?.parseInput?.(input)
      ).not.toThrow();
    }
  });

  test('rejects invalid input for every placeholder module', () => {
    const invalidInputs: ReadonlyArray<
      [moduleId: string, exportName: string, input: unknown]
    > = [
      ['host:assets', 'read', { path: '../secret' }],
      ['host:assets', 'read', { path: '/absolute' }],
      ['host:assets', 'read', { path: '..\\secret' }],
      ['host:assets', 'read', { path: 'C:/secret' }],
      ['host:assets', 'read', { path: 'C:\\secret' }],
      ['host:assets', 'read', { path: '\\\\server\\share' }],
      ['host:clipboard', 'writeText', {}],
      ['host:notifications', 'show', { body: 'missing title' }],
      ['host:camera', 'capturePhoto', {}],
      ['host:microphone', 'record', { durationMs: 0 }],
      ['host:location', 'getCurrentPosition', {}],
      ['host:external-link', 'open', { url: 'ftp://example.com' }],
      ['host:external-link', 'open', { url: 'https://' }],
      ['host:external-link', 'open', { url: 'https://exa mple.com' }],
      ['host:external-link', 'open', { url: 'https://user:pass@example.com' }],
      ['host:navigate', 'navigate', { path: 'settings' }],
      ['host:navigate', 'navigate', { path: '/settings', mode: 'reload' }],
      ['host:file-import', 'pick', { accept: 'image/png' }],
      ['host:file-export', 'save', { name: 'notes.txt' }],
      ['host:share', 'share', {}],
      ['host:net', 'request', { url: 'http://example.com' }],
      ['host:net', 'request', { url: 'https://' }],
      ['host:net', 'request', { url: 'https://user@example.com' }],
      ['host:net', 'request', { url: 'https://example.com\\@evil.com' }],
      [
        'host:net',
        'request',
        { url: 'https://example.com', method: 'GET', bodyBase64: 'AAEC' },
      ],
      [
        'host:net',
        'request',
        { url: 'https://example.com', bodyBase64: 'AAEC' },
      ],
    ];

    for (const [moduleId, exportName, input] of invalidInputs) {
      expect(() =>
        descriptor(moduleId, exportName).schema?.parseInput?.(input)
      ).toThrow();
    }
  });

  test('validates outputs declared by placeholder modules', () => {
    const validOutputs: ReadonlyArray<
      [moduleId: string, exportName: string, output: unknown]
    > = [
      ['host:assets', 'read', { base64: 'AAEC' }],
      ['host:camera', 'capturePhoto', { base64: 'AAEC' }],
      ['host:microphone', 'record', { base64: 'AAEC' }],
      ['host:location', 'getCurrentPosition', { latitude: 1, longitude: 2 }],
      [
        'host:file-import',
        'pick',
        { files: [{ name: 'a.txt', base64: 'AAEC' }] },
      ],
      ['host:file-export', 'save', { saved: true }],
      ['host:net', 'request', { status: 200, bodyBase64: '' }],
    ];

    for (const [moduleId, exportName, output] of validOutputs) {
      expect(() =>
        descriptor(moduleId, exportName).schema?.parseOutput?.(output)
      ).not.toThrow();
    }

    const invalidOutputs: ReadonlyArray<
      [moduleId: string, exportName: string, output: unknown]
    > = [
      ['host:assets', 'read', { base64: 'AAE' }],
      ['host:camera', 'capturePhoto', {}],
      ['host:microphone', 'record', { base64: 1 }],
      ['host:location', 'getCurrentPosition', { latitude: 91, longitude: 0 }],
      ['host:file-import', 'pick', { files: [{}] }],
      ['host:file-export', 'save', { saved: 'yes' }],
      ['host:net', 'request', { status: 99, bodyBase64: '' }],
    ];

    for (const [moduleId, exportName, output] of invalidOutputs) {
      expect(() =>
        descriptor(moduleId, exportName).schema?.parseOutput?.(output)
      ).toThrow();
    }
  });
});
