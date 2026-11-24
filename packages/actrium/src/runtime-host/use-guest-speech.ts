import React from 'react';
import { NativeEventEmitter, NativeModules, Platform } from 'react-native';
import type { SpeechInterimHandler, SpeechRecognizeInput, SpeechStateHandler } from './contract';
import { createSpeechRecognizer, type SpeechRecognitionBackend } from './speech-recognition';

type NativeSpeechModule = SpeechRecognitionBackend &
  NonNullable<ConstructorParameters<typeof NativeEventEmitter>[0]>;
type SpeechRequest = { controller: AbortController; finish?: () => void };

let recognize: ReturnType<typeof createSpeechRecognizer> | undefined;

export function isGuestSpeechAvailable(): boolean {
  return nativeSpeechModule() !== undefined;
}

function nativeSpeechModule(): NativeSpeechModule | undefined {
  if (Platform.OS !== 'macos') return undefined;
  const module = NativeModules.ExpoSpeechRecognition as NativeSpeechModule | undefined;
  return typeof module?.start === 'function' ? module : undefined;
}

function getRecognizer() {
  if (!recognize) {
    const module = nativeSpeechModule();
    if (!module) throw new Error('Speech recognition is unavailable.');
    // Bridgeless native methods are populated on access. Copying the module misses lazy methods.
    recognize = createSpeechRecognizer(module, new NativeEventEmitter(module));
  }
  return recognize;
}

export function useGuestSpeech(allowed: boolean) {
  const currentRequest = React.useRef<SpeechRequest | null>(null);
  const canRecognize = React.useRef(false);
  const [active, setActive] = React.useState(false);

  const cancel = React.useCallback(() => {
    const request = currentRequest.current;
    if (!request) return;
    request.controller.abort();
    currentRequest.current = null;
    setActive(false);
  }, []);

  React.useLayoutEffect(() => {
    canRecognize.current = allowed;
    if (!allowed) cancel();
  }, [allowed, cancel]);

  const stop = React.useCallback(async () => {
    const session = currentRequest.current;
    // Deliver the native state event outside the Guest's stop RPC stack.
    await Promise.resolve();
    if (!session || session !== currentRequest.current) return;
    if (session.finish) session.finish();
    else session.controller.abort();
  }, []);

  const request = React.useCallback(async (
    input: SpeechRecognizeInput, onInterim?: SpeechInterimHandler, onState?: SpeechStateHandler
  ) => {
    if (!canRecognize.current) throw new Error('Speech recognition requires the active, authorized tab.');
    if (currentRequest.current) throw new Error('Speech recognition is already in progress.');
    const session: SpeechRequest = { controller: new AbortController() };
    currentRequest.current = session;
    setActive(true);
    try {
      return await getRecognizer()(input, session.controller.signal, onInterim, {
        onStarted: (finish) => { session.finish = finish; onState?.({ state: 'recording' }); },
        onFinalizing: () => onState?.({ state: 'finalizing' }),
      });
    } finally {
      if (currentRequest.current === session) {
        currentRequest.current = null;
        setActive(false);
      }
    }
  }, []);

  return { active, request, stop, cancel };
}
