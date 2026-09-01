import { Linking } from 'react-native';
import { Camera } from 'react-native-vision-camera';
import React, { useCallback, useRef, useState } from 'react';
import { useAudioRecordingStream } from '../../../hooks/useAudioRecordingStream';
import { useTranslation } from 'react-i18next';
import { GlobalAlert } from '../../../../components';
import type { IMessage } from '../../../../repository/MessageRepository';
import { saveVoiceMessageWav } from '../../../utils/VoiceMessageStorage';

interface UseActrVoiceControllerParams {
  processVoiceMessageSend: (
    content: string,
    options?: { isVoice?: boolean },
  ) => Promise<any>;
  sendVoiceChunk: (chunk: Uint8Array) => Promise<unknown>;
  startVoice: (payload: any) => Promise<string>;
  endVoice: () => Promise<unknown>;
  onVoiceMessageReady: (payload: {
    messageId: string;
    message: IMessage;
    voicePayload: string;
  }) => void;
  onVoiceMessageCanceled: (messageId: string) => void;
}

interface UseActrVoiceControllerResult {
  isVoiceRecording: boolean;
  isCancellingVoice: boolean;
  metering: number;
  handleVoiceStart: () => Promise<void>;
  handleVoiceEnd: (isCancelled: boolean) => Promise<void>;
  handleVoiceChangeState: (isCancelling: boolean) => void;
}

export function useAppVoiceController(
  params: UseActrVoiceControllerParams,
): UseActrVoiceControllerResult {
  const { t } = useTranslation();
  const {
    processVoiceMessageSend,
    sendVoiceChunk,
    startVoice,
    endVoice,
    onVoiceMessageReady,
    onVoiceMessageCanceled,
  } = params;

  // Voice stream local buffer:<1s First in line, when the threshold is reached, then in sequence.
  const chunkQueueRef = useRef<Uint8Array[]>([]);
  // Local full recording is cached to avoid sending queues by drain Can't go back to the drop-off.
  const recordedChunksRef = useRef<Uint8Array[]>([]);
  const isSendingRef = useRef(false);
  const isDrainingRef = useRef(false);
  // 1s Threshold: Avoiding error triggers a service voice session.
  const thresholdTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isCommittedRef = useRef(false);
  const isStartingRef = useRef(false);
  const stopRequestedRef = useRef(false);
  const hasEverStartedRef = useRef(false);
  // Pass. ref Synchronize the recording status to avoid dependency React state ) causes time-series delays.
  const isVoiceRecordingRef = useRef(false);
  const isVoiceStartPendingRef = useRef(false);
  const currentVoiceMessageIdRef = useRef<string | null>(null);
  const pendingVoiceMessageRef = useRef<IMessage | null>(null);

  const [isVoiceRecording, setIsVoiceRecording] = useState(false);
  const [isCancellingVoice, setIsCancellingVoice] = useState(false);

  const setVoiceRecording = useCallback((next: boolean) => {
    isVoiceRecordingRef.current = next;
    setIsVoiceRecording(next);
  }, []);

  const {
    isRecording: hookIsRecording,
    duration,
    metering,
    start: startRecording,
    stop: stopRecording,
  } = useAudioRecordingStream({
    onAudioChunk: (chunk) => {
      chunkQueueRef.current.push(chunk);
      recordedChunksRef.current.push(chunk);
      void drainQueue();
    },
  });

  const drainQueue = useCallback(async () => {
    // (c) The unopened or already directly returned at the time of dispatch, avoiding and sending disruptions in order.
    if (!isSendingRef.current) return;
    if (isDrainingRef.current) return;

    isDrainingRef.current = true;
    try {
      while (chunkQueueRef.current.length > 0) {
        const chunk = chunkQueueRef.current.shift()!;
        await sendVoiceChunk(chunk);
      }
    } catch (e) {
      console.error('[AppInput] drainQueue Sending Slicing Failed:', e);
    }
    isDrainingRef.current = false;
  }, [sendVoiceChunk]);

  const onThresholdReached = useCallback(async () => {
    isStartingRef.current = true;

    try {
      const reqMsg = await processVoiceMessageSend(t('actr.input.sendVoicePlaceholder'), { isVoice: true });
      if (!reqMsg) {
        console.warn('[Voice] threshold reached but request message is empty, aborting voice start');
        isStartingRef.current = false;
        isCommittedRef.current = false;
        isSendingRef.current = false;
        chunkQueueRef.current = [];
        setVoiceRecording(false);
        stopRecording();
        return;
      }

      currentVoiceMessageIdRef.current = reqMsg.message_id;
      pendingVoiceMessageRef.current = reqMsg.localMessage ?? null;

      if (stopRequestedRef.current) {
        console.log('[Voice] Stop requested during threshold commit, aborting');
        isStartingRef.current = false;
        chunkQueueRef.current = [];
        recordedChunksRef.current = [];
        if (currentVoiceMessageIdRef.current) {
          onVoiceMessageCanceled(currentVoiceMessageIdRef.current);
          currentVoiceMessageIdRef.current = null;
        }
        pendingVoiceMessageRef.current = null;
        return;
      }

      // Enter committed Activate the service voice session after the state.
      await startVoice({ ...reqMsg! });

      if (stopRequestedRef.current) {
        console.log('[Voice] Stop requested after startVoice, ending');
        await endVoice();
        isStartingRef.current = false;
        chunkQueueRef.current = [];
        recordedChunksRef.current = [];
        if (currentVoiceMessageIdRef.current) {
          onVoiceMessageCanceled(currentVoiceMessageIdRef.current);
          currentVoiceMessageIdRef.current = null;
        }
        pendingVoiceMessageRef.current = null;
        return;
      }

      isCommittedRef.current = true;
      isSendingRef.current = true;
      isStartingRef.current = false;

      console.log('[AppInput] 1s Thresholds are reached, start sending the backlog slices');
      void drainQueue();
    } catch (e: any) {
      console.error('[Voice] onThresholdReached failed:', e);
      isStartingRef.current = false;
      setVoiceRecording(false);
      stopRecording();
      chunkQueueRef.current = [];
      GlobalAlert.alert(t('actr.voice.recordingFailed'), t('actr.voice.voiceSessionFailed') + e.message);
    }
  }, [
    drainQueue,
    endVoice,
    processVoiceMessageSend,
    setVoiceRecording,
    startVoice,
    stopRecording,
  ]);

  const handleVoiceStart = useCallback(async () => {
    const status = Camera.getMicrophonePermissionStatus();

    if (status === 'denied') {
      GlobalAlert.alert(t('actr.voice.micPermissionTitle'), t('actr.voice.micPermissionMsg'), [
        { text: t('common.cancel'), style: 'cancel' },
        { text: t('actr.voice.goToSettings'), onPress: () => Linking.openSettings() },
      ]);
      return;
    }

    if (status === 'not-determined') {
      const newStatus = await Camera.requestMicrophonePermission();
      if (newStatus === 'granted') {
        console.log('[Voice] Microphone permission granted after request');
      }
      return;
    }

    // Reset every new audio session to avoid the last uncleaned queue/Marked bit.
    chunkQueueRef.current = [];
    recordedChunksRef.current = [];
    isSendingRef.current = false;
    isDrainingRef.current = false;
    isCommittedRef.current = false;
    isStartingRef.current = false;
    stopRequestedRef.current = false;
    hasEverStartedRef.current = false;
    currentVoiceMessageIdRef.current = null;
    pendingVoiceMessageRef.current = null;

    isVoiceStartPendingRef.current = true;
    setVoiceRecording(true);
    setIsCancellingVoice(false);

    try {
      await startRecording();
      isVoiceStartPendingRef.current = false;

      // If a request for closure has been received during the start-up, it is stopped and withdrawn and no longer enters the threshold time.
      if (stopRequestedRef.current) {
        stopRecording();
        setVoiceRecording(false);
        chunkQueueRef.current = [];
        return;
      }

      thresholdTimerRef.current = setTimeout(() => {
        thresholdTimerRef.current = null;
        void onThresholdReached();
      }, 1000);

      console.log('[AppInput] Sounding started, waiting 1s Threshold');
    } catch (e) {
      console.error('[Voice] handleVoiceStart failed:', e);
      isVoiceStartPendingRef.current = false;
      setVoiceRecording(false);
      chunkQueueRef.current = [];
      if (thresholdTimerRef.current) {
        clearTimeout(thresholdTimerRef.current);
        thresholdTimerRef.current = null;
      }
      GlobalAlert.alert(t('actr.voice.recordingFailed'), t('actr.voice.recordingStartFailed'));
    }
  }, [
    onThresholdReached,
    setVoiceRecording,
    startRecording,
    stopRecording,
  ]);

  const handleVoiceEnd = useCallback(
    async (isCancelled: boolean) => {
      // Allow in“Recording is on./Recording/Threshold is waiting”Response terminated in either state, avoiding quick release.
      const shouldHandleEnd =
        isVoiceRecordingRef.current ||
        isVoiceStartPendingRef.current ||
        hookIsRecording ||
        thresholdTimerRef.current != null ||
        isStartingRef.current;
      if (!shouldHandleEnd) return;

      if (thresholdTimerRef.current) {
        clearTimeout(thresholdTimerRef.current);
        thresholdTimerRef.current = null;
      }

      console.log(
        `[AppInput] handleVoiceEnd | cancelled=${isCancelled} | committed=${isCommittedRef.current} | starting=${isStartingRef.current}`,
      );

      // Release when the threshold is submitted: mark stopRequested, exits by the threshold process by its own back.
      if (isStartingRef.current) {
        console.log('[Voice] Stop requested while onThresholdReached in progress');
        stopRequestedRef.current = true;
        setVoiceRecording(false);
        stopRecording();
        return;
      }

      // If the sound is not finished, it will be allowed to stop.“Release and start.”The competition.
      if (isVoiceStartPendingRef.current && !hookIsRecording) {
        stopRequestedRef.current = true;
        isVoiceStartPendingRef.current = false;
        setVoiceRecording(false);
        stopRecording();
        chunkQueueRef.current = [];
        return;
      }

      setVoiceRecording(false);
      isVoiceStartPendingRef.current = false;
      hasEverStartedRef.current = false;
      stopRequestedRef.current = false;

      try {
        stopRecording();

        // <1s Not available committed: Decision error, silence.
        if (!isCommittedRef.current) {
          console.log(
            `[Voice] Scratch and discard. | Length: ${duration}ms | Drop Slices: ${chunkQueueRef.current.length} individual`,
          );
          chunkQueueRef.current = [];
          recordedChunksRef.current = [];
          isSendingRef.current = false;
          pendingVoiceMessageRef.current = null;
          currentVoiceMessageIdRef.current = null;
          return;
        }

        // >=1s And the user cancels: Ends the remote session and discards the remaining slices.
        if (isCancelled) {
          console.log(`[Voice] User Cancel Send | Length: ${duration}ms`);
          isSendingRef.current = false;
          chunkQueueRef.current = [];
          recordedChunksRef.current = [];
          await endVoice();
          if (currentVoiceMessageIdRef.current) {
            onVoiceMessageCanceled(currentVoiceMessageIdRef.current);
          }
          currentVoiceMessageIdRef.current = null;
          pendingVoiceMessageRef.current = null;
          return;
        }

        console.log(
          `[Voice] Normal end | Length: ${duration}ms | Surplus Slices: ${chunkQueueRef.current.length} individual`,
        );

        // Normal end: Waiting for Queue to be sent endVoiceMake sure you don't throw the tailbag.
        while (isDrainingRef.current) {
          await new Promise((r) => setTimeout(r, 10));
        }
        await drainQueue();

        // FIXME Manual Delay 100ms♪ Prevention end Package sent before all slices are sent
        await new Promise(resolve => setTimeout(resolve, 100))
        
        await endVoice();
        if (
          currentVoiceMessageIdRef.current &&
          recordedChunksRef.current.length > 0 &&
          pendingVoiceMessageRef.current
        ) {
          const voicePayload = await saveVoiceMessageWav({
            messageId: currentVoiceMessageIdRef.current,
            chunks: recordedChunksRef.current,
          });
          onVoiceMessageReady({
            messageId: currentVoiceMessageIdRef.current,
            message: pendingVoiceMessageRef.current,
            voicePayload: JSON.stringify(voicePayload),
          });
        }
        chunkQueueRef.current = [];
        isSendingRef.current = false;
        recordedChunksRef.current = [];
        currentVoiceMessageIdRef.current = null;
        pendingVoiceMessageRef.current = null;
        console.log('[Voice] endVoice Done. Voice sent over.');
      } catch (e) {
        console.error('[Voice] handleVoiceEnd failed:', e);
        chunkQueueRef.current = [];
        recordedChunksRef.current = [];
        isSendingRef.current = false;
        GlobalAlert.alert(t('actr.voice.recordingFailed'), t('actr.voice.recordingError'));
      }
    },
    [
      drainQueue,
      duration,
      endVoice,
      hookIsRecording,
      onVoiceMessageCanceled,
      onVoiceMessageReady,
      setVoiceRecording,
      stopRecording,
    ],
  );

  const handleVoiceChangeState = useCallback((isCancelling: boolean) => {
    setIsCancellingVoice(isCancelling);
  }, []);

  React.useEffect(() => {
    if (hookIsRecording) {
      hasEverStartedRef.current = true;
    }
  }, [hookIsRecording]);

  React.useEffect(() => {
    if (isVoiceRecording && !hookIsRecording && hasEverStartedRef.current) {
      void handleVoiceEnd(false);
    }
  }, [handleVoiceEnd, hookIsRecording, isVoiceRecording]);

  React.useEffect(() => {
    return () => {
      if (thresholdTimerRef.current) {
        clearTimeout(thresholdTimerRef.current);
        thresholdTimerRef.current = null;
      }
      chunkQueueRef.current = [];
      recordedChunksRef.current = [];
      isSendingRef.current = false;
      isDrainingRef.current = false;
      isVoiceRecordingRef.current = false;
      isVoiceStartPendingRef.current = false;
      currentVoiceMessageIdRef.current = null;
      pendingVoiceMessageRef.current = null;
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
