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
  // 保存语音识别期间的实时音量，驱动录音浮层波形高度。
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
      // 把底层识别流的实时音量同步到 React state，供录音浮层刷新波形。
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
    // 按住说话开始前先收口当前所有播放源，
    // 避免语音识别、语音消息播放、TTS 朗读同时占用音频会话导致状态和实际音频输出不一致。
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

    // 简体中文：zh-CN
    // 繁体中文：zh-TW
    // 英文：en-US
    // 日语：ja-JP
    // 韩语：ko-KR
    // 西班牙语：es-ES
    // 俄语：ru-RU
    // 法语：fr-FR
    const started = await speechRecognitionService.start({
      lang: 'zh-CN',
      continuous: true,
      interimResults: true,
      addsPunctuation: true,
      // 开启原生音量事件，驱动 VoiceRecordingOverlay 的真实波形。
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
            // 语音识别启动失败时统一从业务层引导去系统设置，
            // 避免不同平台分别弹原生权限框，导致交互和视觉不一致。
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

      // 停止识别前先缓存一份当前转写，避免部分机型 stop 后未补发 final 结果时丢字。
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
