/**
 * useAudioRecordingStream Hook
 *
 * It's a live voice stream. React Hook
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
 * Requesting microphone access.
 */
async function requestMicrophonePermission(): Promise<boolean> {
  if (Platform.OS === 'android') {
    try {
      const granted = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
        {
          title: 'Microphone Permissions',
          message: 'We need to use the microphone for voice recognition.',
          buttonPositive: 'Allow',
          buttonNegative: 'Reject',
        },
      );
      return granted === PermissionsAndroid.RESULTS.GRANTED;
    } catch (error) {
      console.error('Requesting microphone access failed:', error);
      GlobalAlert.alert('Need microphone access', 'You need microphone permission to use voice input', [
        { text: 'Cancel', style: 'cancel' },
        { text: 'To set it up.', onPress: () => Linking.openSettings() },
      ]);
      return false;
    }
  }
  // iOS Request automatically on first use
  return true;
}

/**
 * Real-time voice streaming Hook
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
  // Record whether the recording is really active for the spacing after the filter has stopped
  const activeRef = useRef(false);

  const { start: startTimer, end: endTimer, duration } = useIntervalTimer();

  // Auto Request Permissions
  useEffect(() => {
    if (autoRequestPermission && hasPermission === null) {
      requestMicrophonePermission().then(setHasPermission);
    }
  }, [autoRequestPermission, hasPermission]);

  // Request permission
  const requestPermission = useCallback(async (): Promise<boolean> => {
    const granted = await requestMicrophonePermission();
    setHasPermission(granted);
    return granted;
  }, []);

  // Start recording.
  const start = useCallback(async () => {
    try {
      setError(null);

      // Check Permissions
      if (hasPermission === false) {
        throw new Error('No microphone privileges granted');
      }

      if (hasPermission === null) {
        const granted = await requestPermission();
        if (!granted) {
          throw new Error('No microphone privileges granted');
        }
      }

      // Create Audio Stream
      audioStreamRef.current = createAudioStream(
        async (chunk) => {
          // If it's already done, stop, no subsequent slices that may be caused by birth delay
          if (!activeRef.current) return;

          setChunkCount((prev) => prev + 1);
          // Calculate and update volume fluctuations
          const m = calculatePcmMetering(chunk);
          setMetering(m);

          try {
            await onAudioChunk(chunk);
          } catch (error) {
            console.error('Failed to process audio slices:', error);
            const err =
              error instanceof Error ? error : new Error(String(error));
            setError(err);
            onError?.(err);
          }
        },
        (error) => {
          console.error('Record error:', error);
          setError(error);
          setIsRecording(false);
          onError?.(error);
        },
      );

      // Start recording.
      await audioStreamRef.current.start();
      activeRef.current = true;
      setIsRecording(true);
      setChunkCount(0);
      setMetering(0);
      onStart?.();
      startTimer();

      console.log('[useAudioRecordingStream] The recording is in progress.');
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      console.error('[useAudioRecordingStream] Failed to start audio recording:', err);
      setError(err);
      onError?.(err);
      throw err;
    }
  }, [hasPermission, onAudioChunk, onError, onStart, requestPermission]);

  // Stop recording
  const stop = useCallback(() => {
    activeRef.current = false;
    try {
      audioStreamRef.current?.stop();
      audioStreamRef.current = null;
      setIsRecording(false);
      setMetering(0);
      onStop?.();
      endTimer();

      console.log('[useAudioRecordingStream] Sound recording stopped');
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      console.error('[useAudioRecordingStream] Stop Record Failed:', err);
      setError(err);
      onError?.(err);
    }
  }, [onError, onStop]);

  // More than 60s Autostop
  useEffect(() => {
    const MAX_DURATION = 60000; // 60s
    if (isRecording && duration >= MAX_DURATION) {
      console.log('[useAudioRecordingStream] Maximum recording time reached, automatic stop');
      stop();
    }
  }, [duration, isRecording, stop]);

  // Autostops component when unmounting
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
