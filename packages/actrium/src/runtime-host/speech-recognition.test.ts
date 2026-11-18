import { expect, mock, test } from 'bun:test';
import { createSpeechRecognizer, type SpeechRecognitionBackend, type SpeechRecognitionEvents } from './speech-recognition';

type EventName = Parameters<SpeechRecognitionEvents['addListener']>[0];

function nativeSpeech() {
  const listeners = new Map<string, (event: never) => void>();
  const backend: SpeechRecognitionBackend = {
    requestPermissionsAsync: mock(async () => ({ granted: true })),
    start: mock(() => {}),
    stop: mock(() => {}),
    abort: mock(() => {}),
  };
  const events: SpeechRecognitionEvents = {
    addListener: (name, listener) => {
      listeners.set(name, listener);
      return { remove: () => { listeners.delete(name); } };
    },
  };
  return {
    backend,
    events,
    emit: (name: EventName, event: unknown = null) => listeners.get(name)?.(event as never),
    listeners,
  };
}

test('calls prototype-backed native methods and returns the transcript after end', async () => {
  const { backend, events, emit, listeners } = nativeSpeech();
  const proxy: SpeechRecognitionBackend = Object.create(backend);
  const recognize = createSpeechRecognizer(proxy, events);
  const request = recognize({ locale: 'zh-CN' }, new AbortController().signal);
  await Promise.resolve();
  expect(backend.start).toHaveBeenCalledWith(expect.objectContaining({ lang: 'zh-CN' }));
  emit('start');
  emit('result', { isFinal: false, results: [{ transcript: '\u4f60' }] });
  emit('result', { isFinal: true, results: [{ transcript: '\u4f60\u597d' }] });
  await expect(recognize({}, new AbortController().signal)).rejects.toThrow('already in progress');
  emit('end');
  await expect(request).resolves.toEqual({ text: '\u4f60\u597d' });
  expect(listeners.size).toBe(0);
});

test('does not start capture after the Guest leaves during the permission prompt', async () => {
  const { backend, events } = nativeSpeech();
  const permission = Promise.withResolvers<{ granted: boolean }>();
  backend.requestPermissionsAsync = () => permission.promise;
  const controller = new AbortController();
  const request = createSpeechRecognizer(backend, events)({}, controller.signal);
  controller.abort();
  permission.resolve({ granted: true });
  await expect(request).rejects.toThrow('cancelled');
  expect(backend.start).not.toHaveBeenCalled();
});

test('cancels a pending native start and holds the recognizer until it ends', async () => {
  const { backend, events, emit, listeners } = nativeSpeech();
  const recognize = createSpeechRecognizer(backend, events);
  const controller = new AbortController();
  const request = recognize({}, controller.signal);
  await Promise.resolve();
  controller.abort();
  await expect(request).rejects.toThrow('cancelled');
  expect(backend.abort).not.toHaveBeenCalled();
  await expect(recognize({}, new AbortController().signal)).rejects.toThrow('already in progress');
  emit('start');
  expect(backend.abort).toHaveBeenCalledTimes(1);
  emit('end');
  expect(listeners.size).toBe(0);
});

test('denied OS permission never starts capture', async () => {
  const { backend, events } = nativeSpeech();
  backend.requestPermissionsAsync = async () => ({ granted: false });
  await expect(createSpeechRecognizer(backend, events)({}, new AbortController().signal))
    .rejects.toThrow('system permission');
  expect(backend.start).not.toHaveBeenCalled();
});

test('discards native results that arrive before the new session starts', async () => {
  const { backend, events, emit } = nativeSpeech();
  const request = createSpeechRecognizer(backend, events)({}, new AbortController().signal);
  await Promise.resolve();
  emit('result', { isFinal: true, results: [{ transcript: 'previous session' }] });
  emit('end');
  emit('start');
  emit('end');
  await expect(request).rejects.toThrow('No speech');
});

test('emits complete interim hypotheses and delivers the final text once', async () => {
  const { backend, events, emit } = nativeSpeech();
  const interim = mock((_result: { text: string }) => {});
  const recognize = createSpeechRecognizer(backend, events);
  const request = recognize({}, new AbortController().signal, interim);
  await Promise.resolve();
  expect(backend.start).toHaveBeenCalledWith(expect.objectContaining({ interimResults: true }));
  emit('start');
  emit('result', { isFinal: false, results: [{ transcript: '\u4f60\u597d' }] });
  emit('result', { isFinal: false, results: [{ transcript: '\u60a8\u597d\uff0c\u4e16\u754c' }] });
  emit('result', { isFinal: true, results: [{ transcript: '\u60a8\u597d\uff0c\u4e16\u754c\u3002' }] });
  expect(interim.mock.calls).toEqual([[{ text: '\u4f60\u597d' }], [{ text: '\u60a8\u597d\uff0c\u4e16\u754c' }]]);
  emit('end');
  await expect(request).resolves.toEqual({ text: '\u60a8\u597d\uff0c\u4e16\u754c\u3002' });
});

test('reports recording and stops gracefully once while preserving all dictated segments', async () => {
  const { backend, events, emit } = nativeSpeech();
  const interim = mock((_result: { text: string }) => {});
  const phases: string[] = [];
  let stop: (() => void) | undefined;
  const recognize = createSpeechRecognizer(backend, events);
  const request = recognize({ continuous: true }, new AbortController().signal, interim, {
    onStarted: (finish) => { stop = finish; phases.push('recording'); },
    onFinalizing: () => { phases.push('finalizing'); },
  });
  await Promise.resolve();
  expect(backend.start).toHaveBeenCalledWith(expect.objectContaining({ continuous: true }));
  expect(phases).toEqual([]);
  emit('start');
  emit('result', { isFinal: true, results: [{ transcript: 'First.' }] });
  emit('result', { isFinal: false, results: [{ transcript: ' Sec' }] });
  expect(interim.mock.lastCall).toEqual([{ text: 'First. Sec' }]);
  stop?.();
  stop?.();
  expect(backend.stop).toHaveBeenCalledTimes(1);
  expect(backend.abort).not.toHaveBeenCalled();
  expect(phases).toEqual(['recording', 'finalizing']);
  emit('result', { isFinal: true, results: [{ transcript: ' Second.' }] });
  emit('end');
  await expect(request).resolves.toEqual({ text: 'First. Second.' });
  stop?.();
  expect(backend.stop).toHaveBeenCalledTimes(1);
});
