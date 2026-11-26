import { describe, expect, test } from 'bun:test';
import { createHostModuleDispatch } from 'keel/contract';

import { contract } from './contract';
import type { FileImportPickInput, FileImportPickOutput } from './contract';
import type { FrontCapability } from './capabilities';
import type { AttachmentUploadManager } from './attachment-upload';
import { createHostModules, type HostModules } from './host-modules';
import { createTestServiceEventSource } from './service-events.test-utils';

const input = { route: 'echo.EchoService.Echo', payloadBase64: 'AAEC' };

function createTestHostModules(options?: {
  hasServiceCapability?: boolean;
  hasServiceOnCapability?: boolean;
  serviceEventSource?: ReturnType<typeof createTestServiceEventSource>['source'];
  authorizeCapability?: (capability: FrontCapability) => Promise<void>;
  onCapabilityRevoked?: (capability: FrontCapability, listener: () => void) => () => void;
  trackServiceEventSubscription?: (cleanup: () => void) => () => void;
  writeClipboardText?: (text: string) => Promise<void>;
  setTabTitle?: (title: string) => void;
  pickFiles?: (input: FileImportPickInput) => Promise<FileImportPickOutput>;
  saveFile?: HostModules['host:file-export']['save'];
  recognizeSpeech?: Parameters<typeof createHostModules>[0]['recognizeSpeech'];
}): HostModules {
  return createHostModules({
    hasServiceCapability: options?.hasServiceCapability ?? true,
    hasServiceOnCapability: options?.hasServiceOnCapability ?? true,
    callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
    serviceEventSource: options?.serviceEventSource,
    authorizeCapability: options?.authorizeCapability ?? (async () => {}),
    onCapabilityRevoked: options?.onCapabilityRevoked,
    trackServiceEventSubscription: options?.trackServiceEventSubscription,
    writeClipboardText: options?.writeClipboardText ?? (async () => {}),
    setTabTitle: options?.setTabTitle,
    pickFiles: options?.pickFiles,
    saveFile: options?.saveFile,
    recognizeSpeech: options?.recognizeSpeech,
  });
}

test('file export authorizes the save and preserves cancellation', async () => {
  const calls: unknown[] = [];
  const modules = createTestHostModules({
    authorizeCapability: async (name) => { calls.push(name); },
    saveFile: async (input) => { calls.push(input); return { saved: false }; },
  });
  const file = { name: 'history.zip', base64: 'AAEC' };
  await expect(modules['host:file-export'].save(file)).resolves.toEqual({ saved: false });
  expect(calls).toEqual(['host:file-export.save', file]);
});

describe('host:service implementation', () => {
  test('rejects calls when the service capability is not declared', async () => {
    let called = false;
    const hostModules = createHostModules({
      hasServiceCapability: false,
      hasServiceOnCapability: false,
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
      hasServiceOnCapability: true,
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
      hasServiceOnCapability: true,
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
      hasServiceOnCapability: true,
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
      hasServiceOnCapability: true,
      callActor: async () => ({ success: true }),
      authorizeCapability: async () => {},
      writeClipboardText: async () => {},
    });

    await expect(hostModules['host:service'].call(input)).rejects.toThrow(
      'Service call failed'
    );
  });

  test('fails closed for tell until implemented', async () => {
    const hostModules = createTestHostModules();

    await expect(hostModules['host:service'].tell(input)).rejects.toThrow(
      'not implemented'
    );
  });

  test('rejects service.on without the service.on capability', () => {
    const hostModules = createTestHostModules({
      hasServiceOnCapability: false,
      serviceEventSource: createTestServiceEventSource(['notes.v1.changed'])
        .source,
    });

    expect(() => hostModules['host:service'].on(() => {})).toThrow(
      'service.on capability'
    );
  });

  test('rejects service.on when no event source is available', () => {
    const hostModules = createTestHostModules({
      serviceEventSource: undefined,
    });

    expect(() => hostModules['host:service'].on(() => {})).toThrow(
      'Service event source is not available'
    );
  });

  test('delivers service events through the event source after authorization', async () => {
    const { source, emit, subscriberCount } = createTestServiceEventSource([
      'notes.v1.changed',
    ]);
    const hostModules = createTestHostModules({ serviceEventSource: source });

    const received: { event: string; payloadBase64: string }[] = [];
    const unsubscribe = hostModules['host:service'].on((event) => {
      received.push(event);
    });

    // Authorization resolves on the next microtask; events before that are dropped.
    emit({ event: 'notes.v1.changed', payloadBase64: 'AAEC' });
    expect(received).toEqual([]);

    await Promise.resolve();
    emit({ event: 'notes.v1.changed', payloadBase64: 'AAEC' });
    expect(received).toEqual([
      { event: 'notes.v1.changed', payloadBase64: 'AAEC' },
    ]);

    unsubscribe?.();
    expect(subscriberCount()).toBe(0);
    emit({ event: 'notes.v1.changed', payloadBase64: 'AAEC' });
    expect(received).toHaveLength(1);
  });

  test('unsubscribes the transport when service.on is revoked', async () => {
    let subscriptions = 0;
    let revoke!: () => void;
    const hostModules = createTestHostModules({
      serviceEventSource: {
        subscribe: () => {
          subscriptions += 1;
          return () => { subscriptions -= 1; };
        },
      },
      onCapabilityRevoked: (_capability, listener) => {
        revoke = listener;
        return () => {};
      },
    });
    hostModules['host:service'].on(() => {});
    await Promise.resolve();

    expect(subscriptions).toBe(1);
    revoke();
    expect(subscriptions).toBe(0);
  });

  test('engine teardown removes both transport and revocation listeners', async () => {
    let transportSubscriptions = 0;
    const revocationListeners = new Set<() => void>();
    const trackedCleanups = new Set<() => void>();
    const hostModules = createTestHostModules({
      serviceEventSource: {
        subscribe: () => {
          transportSubscriptions += 1;
          return () => { transportSubscriptions -= 1; };
        },
      },
      onCapabilityRevoked: (_capability, listener) => {
        revocationListeners.add(listener);
        return () => { revocationListeners.delete(listener); };
      },
      trackServiceEventSubscription: (cleanup) => {
        trackedCleanups.add(cleanup);
        return () => { trackedCleanups.delete(cleanup); };
      },
    });
    const unsubscribe = hostModules['host:service'].on(() => {});
    await Promise.resolve();
    expect(transportSubscriptions).toBe(1);
    expect(revocationListeners.size).toBe(1);
    expect(trackedCleanups.size).toBe(1);

    for (const cleanup of [...trackedCleanups]) cleanup();
    expect(transportSubscriptions).toBe(0);
    expect(revocationListeners.size).toBe(0);
    expect(trackedCleanups.size).toBe(0);

    unsubscribe?.();
    expect(transportSubscriptions).toBe(0);
  });

  test('unsubscribes when service.on authorization is rejected', async () => {
    const { source, emit, subscriberCount } = createTestServiceEventSource([
      'notes.v1.changed',
    ]);
    const hostModules = createTestHostModules({
      serviceEventSource: source,
      authorizeCapability: async () => {
        throw new Error('permission_denied');
      },
    });

    const received: { event: string; payloadBase64: string }[] = [];
    hostModules['host:service'].on((event) => {
      received.push(event);
    });

    // Denial must not briefly open and then close an unauthorized transport.
    expect(subscriberCount()).toBe(0);
    await Promise.resolve();
    expect(subscriberCount()).toBe(0);
    emit({ event: 'notes.v1.changed', payloadBase64: 'AAEC' });
    expect(received).toEqual([]);
  });

  test('cancelling pending service.on authorization never opens the transport', async () => {
    const { source, subscriberCount } = createTestServiceEventSource([
      'notes.v1.changed',
    ]);
    let approve!: () => void;
    const hostModules = createTestHostModules({
      serviceEventSource: source,
      authorizeCapability: () => new Promise<void>((resolve) => {
        approve = resolve;
      }),
    });

    const unsubscribe = hostModules['host:service'].on(() => {});
    expect(subscriberCount()).toBe(0);
    unsubscribe?.();
    approve();
    await Promise.resolve();

    expect(subscriberCount()).toBe(0);
  });

  test('revocation after authorization resolution never opens the transport', async () => {
    let subscribeCalls = 0;
    let revoked = false;
    const hostModules = createTestHostModules({
      serviceEventSource: {
        subscribe: () => {
          subscribeCalls += 1;
          return () => {};
        },
      },
      onCapabilityRevoked: (_capability, listener) => {
        if (revoked) listener();
        return () => {};
      },
    });

    // The authorization promise resolves on a later microtask. The snapshot is
    // revoked before that continuation starts, so attaching the listener must
    // synchronously observe the latest state and prevent subscription startup.
    // Count starts, not net subscriptions: cleanup after an already-opened
    // transport would otherwise hide the regression.
    hostModules['host:service'].on(() => {});
    revoked = true;
    await Promise.resolve();

    expect(subscribeCalls).toBe(0);
  });
});

describe('host:service.attachmentUpload implementation', () => {
  function createUploader(result: { attachmentId: string } = { attachmentId: 'a-1' }) {
    const uploads: unknown[] = [];
    const uploader = {
      available: true,
      upload: async (input: unknown) => {
        uploads.push(input);
        return result;
      },
      dispose: () => {},
    };
    return { uploader, uploads };
  }

  test('fails closed when the native bridge is unavailable', async () => {
    const hostModules = createTestHostModules();
    await expect(
      hostModules['host:service'].uploadAttachment({
        filename: 'a.txt',
        base64: 'AAEC',
      })
    ).rejects.toThrow('native attachment upload bridge is unavailable');
  });

  test('authorizes the capability before uploading', async () => {
    const { uploader, uploads } = createUploader();
    const grants: string[] = [];
    const hostModules = createHostModules({
      hasServiceCapability: true,
      hasServiceOnCapability: true,
      callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
      authorizeCapability: async (capability) => {
        grants.push(capability);
      },
      writeClipboardText: async () => {},
      attachmentUploader: uploader as unknown as AttachmentUploadManager,
    });

    const legacy = { filename: 'a.txt', base64: 'AAEC', type: 'image' as const, fileId: undefined };
    await expect(hostModules['host:service'].uploadAttachment(legacy)).resolves.toEqual({ attachmentId: 'a-1' });
    expect(grants).toEqual(['service.attachmentUpload']);
    expect(uploads).toEqual([{ filename: 'a.txt', base64: 'AAEC', type: 'image' }]);
  });

  test('does not upload when authorization fails', async () => {
    const { uploader, uploads } = createUploader();
    const hostModules = createHostModules({
      hasServiceCapability: true,
      hasServiceOnCapability: true,
      callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
      authorizeCapability: async () => {
        throw new Error('permission_required: service.attachmentUpload');
      },
      writeClipboardText: async () => {},
      attachmentUploader: uploader as unknown as AttachmentUploadManager,
    });

    await expect(
      hostModules['host:service'].uploadAttachment({
        filename: 'a.txt',
        base64: 'AAEC',
      })
    ).rejects.toThrow('permission_required');
    expect(uploads).toEqual([]);
  });

  test('dispatches through the contract boundary', async () => {
    const { uploader } = createUploader();
    const dispatch = createHostModuleDispatch(
      contract,
      createHostModules({
        hasServiceCapability: true,
        hasServiceOnCapability: true,
        callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
        authorizeCapability: async () => {},
        writeClipboardText: async () => {},
        attachmentUploader: uploader as unknown as AttachmentUploadManager,
      })
    );

    await expect(
      dispatch['host:service'].uploadAttachment({
        filename: 'a.txt',
        base64: 'AAEC',
      })
    ).resolves.toEqual({ attachmentId: 'a-1' });
    expect(() =>
      dispatch['host:service'].uploadAttachment({ filename: 'a.txt', base64: 'AAE' })
    ).toThrow('Boundary input validation failed');
  });

  test('dispatches cached picks, preview, upload and release with the same file ID', async () => {
    const fileId = '824be89f-48d0-451e-b016-ef133fde55c2';
    const original = { filename: 'photo.png', base64: 'AAEC' };
    const files = [{ fileId, name: original.filename, size: 3, canPreview: true }];
    const operations: string[] = [];
    const { uploader, uploads } = createUploader();
    const dispatch = createHostModuleDispatch(contract, createHostModules({
      hasServiceCapability: true,
      hasServiceOnCapability: false,
      callActor: async () => ({ success: true, payloadBase64: 'AAEC' }),
      authorizeCapability: async (capability) => { operations.push(capability); },
      writeClipboardText: async () => {},
      pickFiles: async () => ({ files }),
      openFilePreview: async (id) => { expect(id).toBe(fileId); },
      releaseCachedFile: async (id) => { expect(id).toBe(fileId); },
      readCachedFile: async (id) => {
        expect(id).toBe(fileId);
        return original;
      },
      attachmentUploader: uploader as unknown as AttachmentUploadManager,
    }));
    const picker = dispatch['host:file-import'];
    expect(await picker.pick({ cache: true })).toEqual({ files });
    await picker.openPreview({ fileId });
    await dispatch['host:service'].uploadAttachment({ fileId, type: 'image' });
    await picker.release({ fileId });
    expect(uploads).toEqual([{ ...original, type: 'image', id: undefined }]);
    expect(operations).toEqual([
      'host:file-import.pick', 'host:file-import.openPreview', 'service.attachmentUpload', 'host:file-import.release',
    ]);
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

describe('host:tab implementation', () => {
  test('updates the tab title through the host adapter', async () => {
    let title = '';
    const hostModules = createTestHostModules({
      setTabTitle: (value) => {
        title = value;
      },
    });

    await hostModules['host:tab'].setTitle({ title: 'Inbox' });
    expect(title).toBe('Inbox');
  });

  test('rejects title updates when authorization fails', async () => {
    let called = false;
    const hostModules = createTestHostModules({
      authorizeCapability: async () => {
        throw new Error('permission_required: host:tab.setTitle');
      },
      setTabTitle: () => {
        called = true;
      },
    });

    await expect(
      hostModules['host:tab'].setTitle({ title: 'Inbox' })
    ).rejects.toThrow('permission_required');
    expect(called).toBe(false);
  });

  test('fails closed when the host tab title adapter is unavailable', async () => {
    const hostModules = createTestHostModules();

    await expect(
      hostModules['host:tab'].setTitle({ title: 'Inbox' })
    ).rejects.toThrow('unavailable');
  });
});

describe('host:file-import implementation', () => {
  test('authorizes before opening the selected native picker', async () => {
    const calls: unknown[] = [];
    const files = [{ name: 'photo.heic', base64: 'AAEC' }];
    const dispatch = createHostModuleDispatch(contract, createTestHostModules({
      authorizeCapability: async (capability) => { calls.push(capability); },
      pickFiles: async ({ source, multiple }) => {
        calls.push({ source, multiple });
        return { files };
      },
    }));
    await expect(dispatch['host:file-import'].pick({ source: 'photos', multiple: true })).resolves.toEqual({ files });
    expect(calls).toEqual(['host:file-import.pick', { source: 'photos', multiple: true }]);
  });

  test('does not open a native picker when authorization fails', async () => {
    let opened = false;
    const modules = createTestHostModules({
      authorizeCapability: async () => { throw new Error('permission_denied'); },
      pickFiles: async () => { opened = true; return { files: [] }; },
    });
    await expect(modules['host:file-import'].pick({})).rejects.toThrow('permission_denied');
    expect(opened).toBe(false);
  });
});

describe('host:speech implementation', () => {
  test('stopping a request during Guest authorization prevents capture after late approval', async () => {
    const grant = Promise.withResolvers<void>();
    let recognitions = 0;
    const modules = createTestHostModules({
      authorizeCapability: () => grant.promise,
      recognizeSpeech: async () => { recognitions += 1; return { text: 'hello' }; },
    });
    const outcome = Promise.resolve(modules['host:speech'].recognize({})).then(
      (result) => ({ result }),
      (error: unknown) => ({ error }),
    );
    await modules['host:speech'].stop();
    grant.resolve();
    expect(await outcome).toMatchObject({ error: { message: 'Speech recognition was cancelled.' } });
    expect(recognitions).toBe(0);
    await expect(modules['host:speech'].recognize({})).resolves.toEqual({ text: 'hello' });
  });

  test('sends interim results only to subscribers on the Guest that started the session', async () => {
    const first = createTestHostModules({
      recognizeSpeech: async (_input, interim) => {
        interim?.({ text: 'interim' });
        return { text: 'final' };
      },
    });
    const second = createTestHostModules();
    const own: unknown[] = [];
    const other: unknown[] = [];
    const unsubscribe = first['host:speech'].onInterimResult((result) => own.push(result));
    second['host:speech'].onInterimResult((result) => other.push(result));

    await expect(first['host:speech'].recognize({})).resolves.toEqual({ text: 'final' });
    expect(own).toEqual([{ text: 'interim' }]);
    expect(other).toEqual([]);
    unsubscribe?.();
    await first['host:speech'].recognize({});
    expect(own).toHaveLength(1);
  });

  test('validates the request, authorizes the Guest and returns only text', async () => {
    const calls: unknown[] = [];
    const dispatch = createHostModuleDispatch(contract, createTestHostModules({
      authorizeCapability: async (capability) => { calls.push(capability); },
      recognizeSpeech: async (input) => {
        calls.push(input);
        return { text: '\u4f60\u597d', nativeDetails: 'internal' };
      },
    }));

    expect(() => dispatch['host:speech'].recognize({ locale: 'invalid/locale' })).toThrow('locale');
    expect(() => dispatch['host:speech'].recognize([])).toThrow('must be an object');
    expect(calls).toEqual([]);
    await expect(dispatch['host:speech'].recognize({ locale: 'zh-CN' })).resolves.toEqual({ text: '\u4f60\u597d' });
    expect(calls).toEqual(['host:speech.recognize', { locale: 'zh-CN' }]);
  });

  test('does not start the recognizer when authorization fails', async () => {
    let started = false;
    const modules = createTestHostModules({
      authorizeCapability: async () => { throw new Error('permission_denied'); },
      recognizeSpeech: async () => { started = true; return { text: 'hello' }; },
    });

    await expect(modules['host:speech'].recognize({})).rejects.toThrow('permission_denied');
    expect(started).toBe(false);
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
