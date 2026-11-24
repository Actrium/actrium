import { expect, mock, test } from 'bun:test';
import { createHostModuleDispatch } from 'keel/contract';
import { createV1CapabilityManifest, writeV1Bundle, type V1Linkage } from 'keel/v1';
import type { FrontCapability } from './capabilities';
import { contract } from './contract';
import { GuestTTS, type TTSSpeakOutput } from './guest-tts';
import { createHostModules } from './host-modules';
import { bindV1HostModules, verifyV1RuntimeGuest } from './v1-guest';

const input = { text: '\u4f60\u597d', locale: 'zh-CN', voice: 'male' } as const;
const cancelled = { status: 'cancelled' } as const;
const finished = { status: 'finished' } as const;
const linkage: V1Linkage = {
  guestEntry: 'workload/guest.js',
  links: [['host:tts', 'speak'], ['host:tts', 'stop']],
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

function setup(authorize = mock(async (_capability: FrontCapability) => {})) {
  const requests = new Map<string, ReturnType<typeof deferred<TTSSpeakOutput>>>();
  const bridge = {
    speak: mock((sessionId: string, _text: string, _locale: string | null, _voice: string) => {
      requests.get(sessionId)?.resolve(cancelled);
      const request = deferred<TTSSpeakOutput>();
      requests.set(sessionId, request);
      return request.promise;
    }),
    stop: mock(async (sessionId: string) => {
      requests.get(sessionId)?.resolve(cancelled);
      requests.delete(sessionId);
    }),
  };
  const revoke = new Set<() => void>();
  const cleanupError = mock((_error: unknown) => {});
  function guest() {
    const tts = new GuestTTS({
      bridge,
      authorize,
      onRevoked: (_capability, listener) => {
        revoke.add(listener);
        return () => { revoke.delete(listener); };
      },
      onCleanupError: cleanupError,
    });
    const host = createHostModules({
      hasServiceCapability: false,
      hasServiceOnCapability: false,
      callActor: async () => ({ success: false }),
      authorizeCapability: authorize,
      writeClipboardText: async () => {},
      tts,
    });
    const bound = bindV1HostModules(linkage, host, authorize);
    const dispatch = createHostModuleDispatch(bound.contract, bound.hostModules)['host:tts'];
    return { tts, dispatch };
  }
  return { bridge, requests, revoke, cleanupError, guest };
}

test('requires the capabilities declared by the linked TTS methods', () => {
  const manifest = JSON.stringify(createV1CapabilityManifest(linkage, contract));
  const script = writeV1Bundle(linkage, 'module.exports = {};');
  expect(verifyV1RuntimeGuest(script, manifest, ['host:tts.speak', 'host:tts.stop']).ok).toBe(true);
  expect(verifyV1RuntimeGuest(script, manifest, ['host:tts.speak']).ok).toBe(false);
  expect(verifyV1RuntimeGuest(script, manifest, ['host:speech']).ok).toBe(false);
});

test('rejects invalid TTS input before authorization or playback', () => {
  const authorize = mock(async (_capability: FrontCapability) => {});
  const { guest, bridge } = setup(authorize);
  const { dispatch } = guest();
  for (const value of [
    [], { text: '   ' }, { text: 1 },
    { ...input, locale: 'zh_CN' }, { ...input, voice: 'other' },
  ]) {
    expect(() => dispatch.speak(value)).toThrow();
  }
  expect(() => dispatch.stop({ sessionId: 'another-guest' })).toThrow();
  expect(authorize).not.toHaveBeenCalled();
  expect(bridge.speak).not.toHaveBeenCalled();
});

test('requires speak authorization and propagates unavailability', async () => {
  const authorize = mock(async (_capability: FrontCapability): Promise<void> => {
    throw new Error('permission_denied');
  });
  const { guest, bridge } = setup(authorize);
  const { dispatch } = guest();
  await expect(dispatch.speak(input)).rejects.toThrow('permission_denied');
  expect(authorize).toHaveBeenCalledWith('host:tts.speak');
  expect(bridge.speak).not.toHaveBeenCalled();
  authorize.mockImplementation(async () => {});
  bridge.speak.mockImplementation(async () => { throw new Error('voice_unavailable: No matching voice.'); });
  await expect(dispatch.speak(input)).rejects.toThrow('voice_unavailable');
});

test('replaces a Guest playback, isolates stop, and returns native results', async () => {
  const { guest, bridge, requests } = setup();
  const first = guest().dispatch;
  const second = guest().dispatch;
  const original = first.speak(input);
  const other = second.speak({ text: 'Hello' });
  await Bun.sleep(0);
  const replacement = first.speak({ ...input, voice: 'female' });
  await expect(original).resolves.toEqual(cancelled);
  const [firstId, secondId] = requests.keys();
  expect(firstId).not.toEqual(secondId);
  expect(bridge.speak.mock.calls).toEqual([
    [firstId, input.text, 'zh-CN', 'male'],
    [secondId, 'Hello', null, 'system'],
    [firstId, input.text, 'zh-CN', 'female'],
  ]);
  await first.stop();
  await expect(replacement).resolves.toEqual(cancelled);
  expect(bridge.stop.mock.calls).toEqual([[firstId]]);
  requests.get(secondId)!.resolve(finished);
  await expect(other).resolves.toEqual(finished);
});

test('stops playback on revocation and releases the listener', async () => {
  const { guest, bridge, revoke, cleanupError } = setup();
  const { dispatch } = guest();
  const playback = dispatch.speak(input);
  await Bun.sleep(0);
  expect(revoke.size).toBe(1);
  for (const listener of revoke) listener();
  await expect(playback).resolves.toEqual(cancelled);
  expect(bridge.stop).toHaveBeenCalledTimes(1);
  expect(revoke.size).toBe(0);
  expect(cleanupError).not.toHaveBeenCalled();
});

test('cancels a pending authorization on stop or disposal', async () => {
  const grant = deferred<void>();
  const authorize = mock((capability: FrontCapability) =>
    capability === 'host:tts.stop' ? Promise.resolve() : grant.promise);
  const { guest, bridge } = setup(authorize);
  const { dispatch, tts } = guest();
  const stopped = dispatch.speak(input);
  await dispatch.stop();
  await expect(stopped).resolves.toEqual(cancelled);
  const disposed = dispatch.speak(input);
  await tts.dispose();
  await expect(disposed).resolves.toEqual(cancelled);
  grant.resolve();
  await expect(dispatch.speak(input)).rejects.toThrow('session has ended');
  expect(bridge.speak).not.toHaveBeenCalled();
});
