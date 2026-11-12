import { describe, expect, test } from 'bun:test';
import { createHostModuleDispatch } from 'keel/contract';

import { contract } from './contract';
import type { FrontCapability } from './capabilities';
import { createHostModules, type HostModules } from './host-modules';

const input = { route: 'echo.EchoService.Echo', payloadBase64: 'AAEC' };

function createTestHostModules(options?: {
  hasServiceCapability?: boolean;
  authorizeCapability?: (capability: FrontCapability) => Promise<void>;
  writeClipboardText?: (text: string) => Promise<void>;
}): HostModules {
  return createHostModules({
    hasServiceCapability: options?.hasServiceCapability ?? true,
    callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
    authorizeCapability: options?.authorizeCapability ?? (async () => {}),
    writeClipboardText: options?.writeClipboardText ?? (async () => {}),
  });
}

describe('host:service implementation', () => {
  test('rejects calls when the service capability is not declared', async () => {
    let called = false;
    const hostModules = createHostModules({
      hasServiceCapability: false,
      callActor: async () => {
        called = true;
        return { success: true, payloadBase64: 'AAEC' };
      },
      authorizeCapability: async () => {},
      writeClipboardText: async () => {},
    });

    await expect(hostModules['host:service'].call(input)).rejects.toThrow(
      'service capability'
    );
    expect(called).toBe(false);
  });

  test('returns the response payload and forwards the route', async () => {
    let captured: {
      route: string;
      payloadBase64: string;
      timeoutMs: number;
    } | null = null;
    const hostModules = createHostModules({
      hasServiceCapability: true,
      callActor: async (request) => {
        captured = request;
        return { success: true, payloadBase64: 'AAEC' };
      },
      authorizeCapability: async () => {},
      writeClipboardText: async () => {},
    });

    await expect(hostModules['host:service'].call(input)).resolves.toEqual({
      payloadBase64: 'AAEC',
    });
    expect(captured).toMatchObject({
      route: 'echo.EchoService.Echo',
      timeoutMs: 30_000,
    });
  });

  test('propagates gateway errors', async () => {
    const hostModules = createHostModules({
      hasServiceCapability: true,
      callActor: async () => ({ success: false, message: 'gateway down' }),
      authorizeCapability: async () => {},
      writeClipboardText: async () => {},
    });

    await expect(hostModules['host:service'].call(input)).rejects.toThrow(
      'gateway down'
    );
  });

  test('times out a call that never resolves', async () => {
    const hostModules = createHostModules({
      hasServiceCapability: true,
      callActor: () => new Promise(() => {}),
      authorizeCapability: async () => {},
      writeClipboardText: async () => {},
    });

    await expect(
      hostModules['host:service'].call({ ...input, timeoutMs: 10 })
    ).rejects.toThrow('timed out');
  });

  test('rejects a success response without a payload', async () => {
    const hostModules = createHostModules({
      hasServiceCapability: true,
      callActor: async () => ({ success: true }),
      authorizeCapability: async () => {},
      writeClipboardText: async () => {},
    });

    await expect(hostModules['host:service'].call(input)).rejects.toThrow(
      'Service call failed'
    );
  });

  test('fails closed for tell and on until implemented', async () => {
    const hostModules = createTestHostModules();

    await expect(hostModules['host:service'].tell(input)).rejects.toThrow(
      'not implemented'
    );
    expect(() => hostModules['host:service'].on(() => {})).toThrow(
      'not implemented'
    );
  });
});

describe('host:clipboard implementation', () => {
  test('writes text through the host adapter', async () => {
    let written = '';
    const hostModules = createTestHostModules({
      writeClipboardText: async (text) => {
        written = text;
      },
    });

    await hostModules['host:clipboard'].writeText({ text: 'hello' });
    expect(written).toBe('hello');
  });

  test('rejects writes when authorization fails', async () => {
    let called = false;
    const hostModules = createTestHostModules({
      authorizeCapability: async () => {
        throw new Error('permission_required: host:clipboard.write');
      },
      writeClipboardText: async () => {
        called = true;
      },
    });

    await expect(
      hostModules['host:clipboard'].writeText({ text: 'hello' })
    ).rejects.toThrow('permission_required');
    expect(called).toBe(false);
  });

  test('dispatches through the contract boundary', async () => {
    let written = '';
    const dispatch = createHostModuleDispatch(
      contract,
      createTestHostModules({
        writeClipboardText: async (text) => {
          written = text;
        },
      })
    );

    await dispatch['host:clipboard'].writeText({ text: 'hello' });
    expect(written).toBe('hello');
    expect(() => dispatch['host:clipboard'].writeText({ text: 1 })).toThrow(
      'Boundary input validation failed'
    );
  });
});

describe('placeholder host modules', () => {
  const placeholderCalls: ReadonlyArray<{
    capability: string;
    call: (hostModules: HostModules) => unknown;
  }> = [
    {
      capability: 'host:assets.read',
      call: (hostModules) =>
        hostModules['host:assets'].read({ path: 'assets/a.png' }),
    },
    {
      capability: 'host:notifications.show',
      call: (hostModules) =>
        hostModules['host:notifications'].show({ title: 'Hi' }),
    },
    {
      capability: 'host:camera.capturePhoto',
      call: (hostModules) => hostModules['host:camera'].capturePhoto(),
    },
    {
      capability: 'host:microphone.record',
      call: (hostModules) =>
        hostModules['host:microphone'].record({ durationMs: 1000 }),
    },
    {
      capability: 'host:location.getCurrentPosition',
      call: (hostModules) =>
        hostModules['host:location'].getCurrentPosition(),
    },
    {
      capability: 'host:external-link.open',
      call: (hostModules) =>
        hostModules['host:external-link'].open({
          url: 'https://example.com',
        }),
    },
    {
      capability: 'host:navigate.navigate',
      call: (hostModules) =>
        hostModules['host:navigate'].navigate({ path: '/settings' }),
    },
    {
      capability: 'host:file-import.pick',
      call: (hostModules) => hostModules['host:file-import'].pick({}),
    },
    {
      capability: 'host:file-export.save',
      call: (hostModules) =>
        hostModules['host:file-export'].save({
          name: 'notes.txt',
          base64: 'AAEC',
        }),
    },
    {
      capability: 'host:share.share',
      call: (hostModules) => hostModules['host:share'].share({ text: 'hello' }),
    },
    {
      capability: 'host:net.request',
      call: (hostModules) =>
        hostModules['host:net'].request({ url: 'https://example.com' }),
    },
  ];

  test('the implementation map matches the active contract', () => {
    expect(() =>
      createHostModuleDispatch(contract, createTestHostModules())
    ).not.toThrow();
  });

  for (const { capability, call } of placeholderCalls) {
    test(`${capability} throws instead of resolving`, async () => {
      const hostModules = createTestHostModules();
      // The stubs reject asynchronously; wrapping also lets the type stay
      // honest for implementations that are allowed to return synchronously.
      await expect(Promise.resolve().then(() => call(hostModules))).rejects.toThrow(
        `${capability} is not implemented`
      );
    });
  }
});
