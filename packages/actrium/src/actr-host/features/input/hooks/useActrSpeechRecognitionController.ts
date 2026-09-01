import React, { useCallback, useRef, useState } from 'react';
import { Linking } from 'react-native';
import { useTranslation } from 'react-i18next';
import {
  speechRecognitionService,
  type SpeechRecognitionErrorEvent,
  type SpeechRecognitionTranscriptEvent,
} from '../../../utils';
import { stopAllVoicePlayback } from '../../chat/hooks/useReadAloudPlayback';
import { GlobalAlert } from '../../../../components';

interface UseActrSpeechRecognitionControllerParams {
  processMessageSend: (content: string) => Promise<unknown>;
}

interface UseActrSpeechRecognitionControllerResult {
  isVoiceRecording: boolean;
  isCancellingVoice: boolean;
  metering: number;
  handleVoiceStart: () => Promise<void>;
  handleVoiceEnd: (isCancelled: boolean) => Promise<void>;
  handleVoiceChangeState: (isCancelling: boolean) => void;
}

export function useActrSpeechRecognitionController(
  params: UseActrSpeechRecognitionControllerParams,
): UseActrSpeechRecognitionControllerResult {
  const { t } = useTranslation();
  const { processMessageSend } = params;
  const [isVoiceRecording, setIsVoiceRecording] = useState(false);
  const [isCancellingVoice, setIsCancellingVoice] = useState(false);
  // Saves the volume of real time sound during speech recognition, driving the floating wave height of the sound.
  const [metering, setMetering] = useState(0);
  const isVoiceRecordingRef = useRef(false);
  const isCancellingVoiceRef = useRef(false);
  const sessionIdRef = useRef(0);
  const activeSessionIdRef = useRef<number | null>(null);
  const lastTranscriptRef = useRef('');
  const fallbackTranscriptRef = useRef('');
  const shouldSubmitOnEndRef = useRef(false);

  const setVoiceRecording = useCallback((next: boolean) => {
    isVoiceRecordingRef.current = next;
    setIsVoiceRecording(next);
  }, []);

  const resetRecognitionState = useCallback(() => {
    setVoiceRecording(false);
    isCancellingVoiceRef.current = false;
    setIsCancellingVoice(false);
    setMetering(0);
    activeSessionIdRef.current = null;
    lastTranscriptRef.current = '';
    fallbackTranscriptRef.current = '';
    shouldSubmitOnEndRef.current = false;
  }, [setVoiceRecording]);

  const updateTranscript = useCallback(
    (event: SpeechRecognitionTranscriptEvent) => {
      if (activeSessionIdRef.current == null || isCancellingVoiceRef.current) {
        return;
      }
      lastTranscriptRef.current = event.text;
    },
    [],
  );

  React.useEffect(() => {
    const unsubscribe = speechRecognitionService.subscribe({
      onPartialResult: updateTranscript,
      onFinalResult: updateTranscript,
      // Synchronize the real-time volume of the lower recognition stream React state, for the sound float to refresh the waveform.
      onMetering: (nextMetering: number) => {
        setMetering(nextMetering);
      },
      onEnd: () => {
        const shouldSubmit = shouldSubmitOnEndRef.current;
        const transcript =
          lastTranscriptRef.current.trim() || fallbackTranscriptRef.current.trim();
        resetRecognitionState();
        if (!shouldSubmit || !transcript) {
          return;
        }
        void processMessageSend(transcript);
      },
      onError: (error: SpeechRecognitionErrorEvent) => {
        console.log(
          '[useActrSpeechRecognitionController] speech recognition error:',
          error.error,
          error.message,
        );
        resetRecognitionState();
      },
    });

    return () => {
      unsubscribe();
    };
  }, [processMessageSend, resetRecognitionState, updateTranscript]);

  const handleVoiceStart = useCallback(async () => {
    // The current broadcast is closed before you start holding your voice.
    // Avoid voice recognition, voice messages,TTS The simultaneous use of audio sessions for reading results in a state and actual audio output not consistent.
    await stopAllVoicePlayback('speech-recognition-start');

    sessionIdRef.current += 1;
    activeSessionIdRef.current = sessionIdRef.current;
    lastTranscriptRef.current = '';
    fallbackTranscriptRef.current = '';
    shouldSubmitOnEndRef.current = false;
    isCancellingVoiceRef.current = false;
    setIsCancellingVoice(false);
    setMetering(0);
    setVoiceRecording(true);

    // CREDIT Chinese:zh-CN
    // Chinese:zh-TW
    // English:en-US
    // Japanese:ja-JP
    // Korean:ko-KR
    // Spanish:es-ES
    // Russian:ru-RU
    // French:fr-FR
    const started = await speechRecognitionService.start({
      lang: 'zh-CN',
      continuous: true,
      interimResults: true,
      addsPunctuation: true,
      // Turn on the original volume event, drive VoiceRecordingOverlay Real wave shape.
      volumeChangeEventOptions: {
        enabled: true,
        intervalMillis: 80,
      },
    });

    if (!started) {
      resetRecognitionState();
      console.warn(
        '[useActrSpeechRecognitionController] speech recognition permission denied or unavailable',
      );
      GlobalAlert.alert(
        t('actr.voice.speechRecognitionPermissionTitle'),
        t('actr.voice.speechRecognitionPermissionMsg'),
        [
          { text: t('common.cancel'), style: 'cancel' },
          {
            text: t('actr.voice.goToSettings'),
            variant: 'primary',
            // The system setup is harmonized when speech recognition failed to start.
            // Avoiding separate bombardian permission frames for different platforms, leading to interactivity and visual incoherence.
            onPress: () => Linking.openSettings(),
          },
        ],
      );
      return;
    }

    console.log(
      '[useActrSpeechRecognitionController] speech recognition session started:',
      activeSessionIdRef.current,
    );
  }, [resetRecognitionState, setVoiceRecording, t]);

  const handleVoiceEnd = useCallback(
    async (isCancelled: boolean) => {
      if (!isVoiceRecordingRef.current) {
        return;
      }

      isCancellingVoiceRef.current = isCancelled;
      setIsCancellingVoice(isCancelled);
      setVoiceRecording(false);

      if (isCancelled) {
        activeSessionIdRef.current = null;
        lastTranscriptRef.current = '';
        fallbackTranscriptRef.current = '';
        shouldSubmitOnEndRef.current = false;
        speechRecognitionService.abort();
        setIsCancellingVoice(false);
        return;
      }

      // Cache a current transcrib before stopping recognition, avoiding some type of machine stop Not completed final The result is lost.
      const { finalText, partialText } = speechRecognitionService.getCurrentTranscript();
      fallbackTranscriptRef.current =
        finalText || partialText || lastTranscriptRef.current;
      shouldSubmitOnEndRef.current = true;
      speechRecognitionService.stop();
      setIsCancellingVoice(false);
    },
    [setVoiceRecording],
  );

  const handleVoiceChangeState = useCallback((isCancelling: boolean) => {
    isCancellingVoiceRef.current = isCancelling;
    setIsCancellingVoice(isCancelling);
  }, []);

  React.useEffect(() => {
    return () => {
      if (activeSessionIdRef.current != null) {
        speechRecognitionService.abort();
      }
    };
  }, []);

  return {
    isVoiceRecording,
    isCancellingVoice,
    metering,
    handleVoiceStart,
    handleVoiceEnd,
    handleVoiceChangeState,
  };
}
