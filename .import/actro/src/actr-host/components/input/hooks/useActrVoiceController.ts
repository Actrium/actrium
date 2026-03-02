import { Alert, Linking } from 'react-native';
import { Camera } from 'react-native-vision-camera';
import React, { useCallback, useRef, useState } from 'react';
import { useAudioStream } from '../../../hooks/useAudioStream';

interface UseActrVoiceControllerParams {
  processVoiceMessageSend: (
    content: string,
    options?: { isVoice?: boolean; skipAuthCheck?: boolean },
  ) => Promise<any>;
  sendVoiceChunk: (chunk: Uint8Array) => Promise<unknown>;
  startVoice: (payload: any) => void;
  endVoice: () => Promise<unknown>;
}

interface UseActrVoiceControllerResult {
  isVoiceRecording: boolean;
  isCancellingVoice: boolean;
  metering: number;
  handleVoiceStart: () => Promise<void>;
  handleVoiceEnd: (isCancelled: boolean) => Promise<void>;
  handleVoiceChangeState: (isCancelling: boolean) => void;
}

export function useActrVoiceController(
  params: UseActrVoiceControllerParams,
): UseActrVoiceControllerResult {
  const { processVoiceMessageSend, sendVoiceChunk, startVoice, endVoice } = params;

  // 语音流本地缓冲：<1s 先入队，达到阈值后再按序发送。
  const chunkQueueRef = useRef<Uint8Array[]>([]);
  const isSendingRef = useRef(false);
  const isDrainingRef = useRef(false);
  // 1s 阈值：避免误触就触发服务端语音会话。
  const thresholdTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isCommittedRef = useRef(false);
  const isStartingRef = useRef(false);
  const stopRequestedRef = useRef(false);
  const hasEverStartedRef = useRef(false);
  // 通过 ref 同步追踪录音状态，避免依赖 React state 带来的时序延迟。
  const isVoiceRecordingRef = useRef(false);
  const isVoiceStartPendingRef = useRef(false);

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
  } = useAudioStream({
    onAudioChunk: (chunk) => {
      chunkQueueRef.current.push(chunk);
      void drainQueue();
    },
  });

  const drainQueue = useCallback(async () => {
    // 未开闸或已在发送中时直接返回，避免并发发送打乱顺序。
    if (!isSendingRef.current) return;
    if (isDrainingRef.current) return;

    isDrainingRef.current = true;
    try {
      while (chunkQueueRef.current.length > 0) {
        const chunk = chunkQueueRef.current.shift()!;
        await sendVoiceChunk(chunk);
      }
    } catch (e) {
      console.error('[ActrInput] drainQueue 发送切片失败:', e);
    }
    isDrainingRef.current = false;
  }, [sendVoiceChunk]);

  const onThresholdReached = useCallback(async () => {
    isStartingRef.current = true;

    try {
      const reqMsg = await processVoiceMessageSend('😌 发送一条语音消息', { isVoice: true });

      if (stopRequestedRef.current) {
        console.log('[Voice] Stop requested during threshold commit, aborting');
        isStartingRef.current = false;
        chunkQueueRef.current = [];
        return;
      }

      // 进入 committed 态后再启动服务端语音会话。
      startVoice({ ...reqMsg! });

      if (stopRequestedRef.current) {
        console.log('[Voice] Stop requested after startVoice, ending');
        await endVoice();
        isStartingRef.current = false;
        chunkQueueRef.current = [];
        return;
      }

      isCommittedRef.current = true;
      isSendingRef.current = true;
      isStartingRef.current = false;

      console.log('[ActrInput] 1s 阈值到达，开始发送积压切片');
      void drainQueue();
    } catch (e) {
      console.error('[Voice] onThresholdReached failed:', e);
      isStartingRef.current = false;
      setVoiceRecording(false);
      stopRecording();
      chunkQueueRef.current = [];
      Alert.alert('录音失败', '无法启动语音会话，请重试');
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
      Alert.alert('需要麦克风权限', '需要麦克风权限才能使用语音输入', [
        { text: '取消', style: 'cancel' },
        { text: '去设置', onPress: () => Linking.openSettings() },
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

    // 每次新录音会话都重置状态，避免继承上次未清理的队列/标志位。
    chunkQueueRef.current = [];
    isSendingRef.current = false;
    isDrainingRef.current = false;
    isCommittedRef.current = false;
    isStartingRef.current = false;
    stopRequestedRef.current = false;
    hasEverStartedRef.current = false;

    isVoiceStartPendingRef.current = true;
    setVoiceRecording(true);
    setIsCancellingVoice(false);

    try {
      await startRecording();
      isVoiceStartPendingRef.current = false;

      // 若启动过程中已经收到结束请求，立即停止并退出，不再进入阈值计时。
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

      console.log('[ActrInput] 录音已启动，等待 1s 阈值');
    } catch (e) {
      console.error('[Voice] handleVoiceStart failed:', e);
      isVoiceStartPendingRef.current = false;
      setVoiceRecording(false);
      chunkQueueRef.current = [];
      if (thresholdTimerRef.current) {
        clearTimeout(thresholdTimerRef.current);
        thresholdTimerRef.current = null;
      }
      Alert.alert('录音失败', '无法启动录制，请重试');
    }
  }, [onThresholdReached, setVoiceRecording, startRecording, stopRecording]);

  const handleVoiceEnd = useCallback(
    async (isCancelled: boolean) => {
      // 允许在“录音启动中/录音中/阈值等待中”任一状态下响应结束，避免快速松手丢事件。
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
        `[ActrInput] handleVoiceEnd | cancelled=${isCancelled} | committed=${isCommittedRef.current} | starting=${isStartingRef.current}`,
      );

      // 阈值提交中就松手：标记 stopRequested，由阈值流程自己兜底退出。
      if (isStartingRef.current) {
        console.log('[Voice] Stop requested while onThresholdReached in progress');
        stopRequestedRef.current = true;
        setVoiceRecording(false);
        stopRecording();
        return;
      }

      // 启动录音尚未完成时也允许直接结束，防止“先松手后启动完成”的竞态。
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

      try {
        stopRecording();

        // <1s 未 committed：判定误触，静默丢弃。
        if (!isCommittedRef.current) {
          console.log(
            `[Voice] 误触丢弃 | 时长: ${duration}ms | 丢弃切片: ${chunkQueueRef.current.length} 个`,
          );
          chunkQueueRef.current = [];
          isSendingRef.current = false;
          return;
        }

        // >=1s 且用户取消：结束远端会话并丢弃剩余切片。
        if (isCancelled) {
          console.log(`[Voice] 用户取消发送 | 时长: ${duration}ms`);
          isSendingRef.current = false;
          chunkQueueRef.current = [];
          await endVoice();
          return;
        }

        console.log(
          `[Voice] 正常结束 | 时长: ${duration}ms | 剩余切片: ${chunkQueueRef.current.length} 个`,
        );

        // 正常结束：等待队列发送完再 endVoice，确保不丢尾包。
        while (isDrainingRef.current) {
          await new Promise((r) => setTimeout(r, 10));
        }
        await drainQueue();
        await endVoice();
        console.log('[Voice] endVoice 完成，语音发送结束');
      } catch (e) {
        console.error('[Voice] handleVoiceEnd failed:', e);
        chunkQueueRef.current = [];
        isSendingRef.current = false;
        Alert.alert('录音失败', '录音处理发生错误');
      }
    },
    [drainQueue, duration, endVoice, hookIsRecording, setVoiceRecording, stopRecording],
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
      isSendingRef.current = false;
      isDrainingRef.current = false;
      isVoiceRecordingRef.current = false;
      isVoiceStartPendingRef.current = false;
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
