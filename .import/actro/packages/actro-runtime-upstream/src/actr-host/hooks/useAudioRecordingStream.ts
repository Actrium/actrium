/**
 * useAudioRecordingStream Hook
 *
 * 实时语音流式传输的 React Hook
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import { Platform, PermissionsAndroid, Alert, Linking } from 'react-native';
import {
  AudioStreamManager,
  createAudioStream,
} from '../utils/AudioStreamManager';
import { calculatePcmMetering } from '../utils/AudioMetering';
import { useIntervalTimer } from '../../hooks';
import { GlobalAlert } from '../../components';

export interface UseAudioRecordingStreamOptions {
  onAudioChunk: (chunk: Uint8Array) => void | Promise<void>;
  onError?: (error: Error) => void;
  onStart?: () => void;
  onStop?: () => void;
  autoRequestPermission?: boolean;
}

export interface UseAudioRecordingStreamReturn {
  isRecording: boolean;
  chunkCount: number;
  error: Error | null;
  start: () => Promise<void>;
  stop: () => void;
  hasPermission: boolean | null;
  requestPermission: () => Promise<boolean>;
  duration: number;
  metering: number;
}

/**
 * 请求麦克风权限
 */
async function requestMicrophonePermission(): Promise<boolean> {
  if (Platform.OS === 'android') {
    try {
      const granted = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
        {
          title: '麦克风权限',
          message: '需要使用麦克风进行语音识别',
          buttonPositive: '允许',
          buttonNegative: '拒绝',
        },
      );
      return granted === PermissionsAndroid.RESULTS.GRANTED;
    } catch (error) {
      console.error('请求麦克风权限失败:', error);
      GlobalAlert.alert('需要麦克风权限', '需要麦克风权限才能使用语音输入', [
        { text: '取消', style: 'cancel' },
        { text: '去设置', onPress: () => Linking.openSettings() },
      ]);
      return false;
    }
  }
  // iOS 会在首次使用时自动请求
  return true;
}

/**
 * 实时语音流式传输 Hook
 */
export function useAudioRecordingStream(
  options: UseAudioRecordingStreamOptions,
): UseAudioRecordingStreamReturn {
  const {
    onAudioChunk,
    onError,
    onStart,
    onStop,
    autoRequestPermission = true,
  } = options;

  const [isRecording, setIsRecording] = useState(false);
  const [chunkCount, setChunkCount] = useState(0);
  const [metering, setMetering] = useState(0);
  const [error, setError] = useState<Error | null>(null);
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);

  const audioStreamRef = useRef<AudioStreamManager | null>(null);
  // 记录录音是否真正处于活跃状态，用于过滤停止后的异步回调
  const activeRef = useRef(false);

  const { start: startTimer, end: endTimer, duration } = useIntervalTimer();

  // 自动请求权限
  useEffect(() => {
    if (autoRequestPermission && hasPermission === null) {
      requestMicrophonePermission().then(setHasPermission);
    }
  }, [autoRequestPermission, hasPermission]);

  // 请求权限
  const requestPermission = useCallback(async (): Promise<boolean> => {
    const granted = await requestMicrophonePermission();
    setHasPermission(granted);
    return granted;
  }, []);

  // 开始录音
  const start = useCallback(async () => {
    try {
      setError(null);

      // 检查权限
      if (hasPermission === false) {
        throw new Error('未授予麦克风权限');
      }

      if (hasPermission === null) {
        const granted = await requestPermission();
        if (!granted) {
          throw new Error('未授予麦克风权限');
        }
      }

      // 创建音频流
      audioStreamRef.current = createAudioStream(
        async (chunk) => {
          // 如果已经执行了 stop，则不再处理后续可能由于原生延迟产生的切片
          if (!activeRef.current) return;

          setChunkCount((prev) => prev + 1);
          // 计算并更新音量波动
          const m = calculatePcmMetering(chunk);
          setMetering(m);

          try {
            await onAudioChunk(chunk);
          } catch (error) {
            console.error('处理音频切片失败:', error);
            const err =
              error instanceof Error ? error : new Error(String(error));
            setError(err);
            onError?.(err);
          }
        },
        (error) => {
          console.error('录音错误:', error);
          setError(error);
          setIsRecording(false);
          onError?.(error);
        },
      );

      // 开始录音
      await audioStreamRef.current.start();
      activeRef.current = true;
      setIsRecording(true);
      setChunkCount(0);
      setMetering(0);
      onStart?.();
      startTimer();

      console.log('[useAudioRecordingStream] 录音已开始');
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      console.error('[useAudioRecordingStream] 启动录音失败:', err);
      setError(err);
      onError?.(err);
      throw err;
    }
  }, [hasPermission, onAudioChunk, onError, onStart, requestPermission]);

  // 停止录音
  const stop = useCallback(() => {
    activeRef.current = false;
    try {
      audioStreamRef.current?.stop();
      audioStreamRef.current = null;
      setIsRecording(false);
      setMetering(0);
      onStop?.();
      endTimer();

      console.log('[useAudioRecordingStream] 录音已停止');
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      console.error('[useAudioRecordingStream] 停止录音失败:', err);
      setError(err);
      onError?.(err);
    }
  }, [onError, onStop]);

  // 超过 60s 自动停止
  useEffect(() => {
    const MAX_DURATION = 60000; // 60s
    if (isRecording && duration >= MAX_DURATION) {
      console.log('[useAudioRecordingStream] 达到最大录音时长，自动停止');
      stop();
    }
  }, [duration, isRecording, stop]);

  // 组件卸载时自动停止
  useEffect(() => {
    return () => {
      activeRef.current = false;
      if (audioStreamRef.current) {
        audioStreamRef.current.stop();
        audioStreamRef.current = null;
      }
    };
  }, []);

  return {
    isRecording,
    chunkCount,
    error,
    duration,
    metering,
    start,
    stop,
    hasPermission,
    requestPermission,
  };
}
