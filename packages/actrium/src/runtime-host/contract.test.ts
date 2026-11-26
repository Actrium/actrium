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
const uploadAttachment = service.uploadAttachment as unknown as InputSchema;

const cachedFileId = '824be89f-48d0-451e-b016-ef133fde55c2';

test('cached picks return metadata and uploads accept the opaque ID only', () => {
  const pick = descriptor('host:file-import', 'pick').schema!;
  expect(pick.parseInput?.({ cache: true })).toMatchObject({ cache: true });
  expect(pick.parseOutput?.({ files: [{ fileId: cachedFileId, name: 'original.png', size: 10 }] }))
    .toEqual({ files: [{ fileId: cachedFileId, name: 'original.png', size: 10 }] });
  const image = { fileId: cachedFileId, name: 'original.png', size: 10, canPreview: true };
  expect(pick.parseOutput?.({ files: [image] })).toEqual({ files: [image] });
  expect(() => pick.parseOutput?.({ files: [{ ...image, canPreview: 'yes' }] })).toThrow();
  expect(uploadAttachment.schema?.parseInput?.({ fileId: cachedFileId, type: 'image' }))
    .toMatchObject({ fileId: cachedFileId, type: 'image' });
  expect(() => uploadAttachment.schema?.parseInput?.({ fileId: cachedFileId, filename: 'override.png', base64: 'AAEC' })).toThrow();
  expect(() => pick.parseOutput?.({ files: [{ fileId: 'file:///private/image.png', name: 'image.png', size: 10 }] })).toThrow();
});

test('an undefined fileId preserves the legacy Base64 upload', () => {
  const legacy = { filename: 'original.png', base64: 'AAEC' };
  const parse = uploadAttachment.schema?.parseInput;
  expect(parse?.({ ...legacy, fileId: undefined })).toEqual(parse?.(legacy));
});

test('Service DataStream validates the stream ID and preserves the full sequence', () => {
  const streams = contract.hostModules['host:service-stream'];
  const outbound = { route: 'example.Service.BeginUpload', streamId: 'upload-1', sequence: '1', payloadBase64: 'YQ==' };
  expect(streams.send.schema?.parseInput?.(outbound)).toEqual(outbound);
  expect(() => streams.send.schema?.parseInput?.({ ...outbound, sequence: '0' })).toThrow();
  expect(streams.register.schema?.parseInput?.({ streamId: 'reply-abc_123' }))
    .toEqual({ streamId: 'reply-abc_123' });
  expect(() => streams.unregister.schema?.parseInput?.({ streamId: 'bad stream' })).toThrow();
  const chunk = { streamId: 'reply-abc_123', sequence: '18446744073709551615', payloadBase64: 'AAEC' };
  expect(streams.onChunk.schema?.parseEvent?.(chunk)).toEqual(chunk);
  expect(() => streams.onChunk.schema?.parseEvent?.({ ...chunk, sequence: '18446744073709551616' }))
    .toThrow();
});

test('Service DataStream accepts a full file-sized chunk and rejects invalid Base64', () => {
  const parse = contract.hostModules['host:service-stream'].send.schema!.parseInput!;
  const chunk = {
    route: 'example.Service.BeginUpload', streamId: 'upload-1', sequence: '1',
    payloadBase64: Buffer.alloc(7 * 1024 * 1024, 255).toString('base64'),
  };
  expect(parse(chunk).payloadBase64).toBe(chunk.payloadBase64);
  expect(() => parse({ ...chunk, payloadBase64: 'abc\n' })).toThrow();
});

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

  test('uploadAttachment accepts a valid request and defaults the type', () => {
    expect(
      uploadAttachment.schema?.parseInput?.({
        filename: 'report.pdf',
        base64: 'AAEC',
      })
    ).toEqual({ filename: 'report.pdf', base64: 'AAEC', type: 'document', id: undefined });
    expect(
      uploadAttachment.schema?.parseInput?.({
        filename: 'photo.png',
        base64: 'AAEC',
        type: 'image',
        id: 'fixed-id',
      })
    ).toEqual({ filename: 'photo.png', base64: 'AAEC', type: 'image', id: 'fixed-id' });
  });

  test('uploadAttachment rejects invalid input', () => {
    const reject = (input: unknown) =>
      expect(() => uploadAttachment.schema?.parseInput?.(input)).toThrow();
    reject({ base64: 'AAEC' }); // missing filename
    reject({ filename: '', base64: 'AAEC' });
    reject({ filename: 'a.txt', base64: 'AAE' }); // unpadded base64
    reject({ filename: 'a.txt', base64: '!!!' });
    reject({ filename: 'a.txt', base64: 'AAEC', type: 'archive' });
    reject({ filename: 'a.txt', base64: 'AAEC', id: '' });
    reject({ filename: 'a.txt', base64: 'AAEC', id: 1 });
    // 7 MiB + 1 byte, base64-encoded.
    const oversized = Buffer.alloc(7 * 1024 * 1024 + 1).toString('base64');
    reject({ filename: 'big.bin', base64: oversized });
  });

  test('uploadAttachment validates the output attachment id', () => {
    expect(uploadAttachment.schema?.parseOutput?.({ attachmentId: 'a-1' })).toEqual({
      attachmentId: 'a-1',
    });
    expect(() => uploadAttachment.schema?.parseOutput?.({})).toThrow();
    expect(() =>
      uploadAttachment.schema?.parseOutput?.({ attachmentId: 1 })
    ).toThrow();
  });
});

describe('host:tab.setTitle contract', () => {
  const parse = descriptor('host:tab', 'setTitle').schema?.parseInput;

  test('accepts a valid title', () => {
    expect(parse?.({ title: 'Inbox' })).toEqual({ title: 'Inbox' });
  });

  test('enforces the UTF-8 byte limit on the ASCII boundary', () => {
    expect(parse?.({ title: 'a'.repeat(256) })).toEqual({ title: 'a'.repeat(256) });
    expect(() => parse?.({ title: 'a'.repeat(257) })).toThrow();
  });

  test('counts multibyte characters as UTF-8 bytes, not characters', () => {
    const twoByteCharacter = String.fromCharCode(0xfc);
    const fourByteCharacter = String.fromCodePoint(0x1f642);
    // The two-byte character makes 128 chars = 256 bytes and 129 = 258 bytes.
    expect(parse?.({ title: twoByteCharacter.repeat(128) })).toEqual({
      title: twoByteCharacter.repeat(128),
    });
    expect(() => parse?.({ title: twoByteCharacter.repeat(129) })).toThrow();
    // Mixed boundary: 254 ASCII bytes + one 2-byte char = 256 bytes exactly.
    expect(parse?.({ title: `${'a'.repeat(254)}${twoByteCharacter}` })).toEqual({
      title: `${'a'.repeat(254)}${twoByteCharacter}`,
    });
    expect(() => parse?.({ title: `${'a'.repeat(255)}${twoByteCharacter}` })).toThrow();
    // The emoji is a surrogate pair in UTF-16 and four bytes in UTF-8.
    expect(parse?.({ title: fourByteCharacter.repeat(64) })).toEqual({
      title: fourByteCharacter.repeat(64),
    });
    expect(() => parse?.({ title: fourByteCharacter.repeat(65) })).toThrow();
  });

  test('rejects a missing or non-string title', () => {
    expect(() => parse?.({})).toThrow();
    expect(() => parse?.({ title: 1 })).toThrow();
    // Empty strings stay allowed (matching clipboard.writeText): the shell
    // renders its own fallback for a blank title.
    expect(parse?.({ title: '' })).toEqual({ title: '' });
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
  ['host:file-import', 'pick', { source: 'photos', multiple: true }],
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
      ['host:file-import', 'pick', { source: 'camera' }],
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

  test('rejects a truncated file picker thumbnail', () => {
    const parse = descriptor('host:file-import', 'pick').schema?.parseOutput;
    const file = { name: 'photo.jpg', base64: 'AAEC', thumbnailBase64: '/9j/4AAQSkZJRg==' };
    expect(() => parse?.({ files: [file] })).toThrow('JPEG thumbnail');
  });
});
