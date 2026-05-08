import React from 'react';
import { Pressable, StyleSheet, Animated, Easing } from 'react-native';
import { Icon } from '../../../components';
import { chatBubbleToolbar } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { WaveForm } from '../shared/WaveForm';
import { useVoicePlaybackStore,
  type VoicePlaybackPart,
} from '../../store/useVoicePlaybackStore';
import { type SelectionMode } from '../../store/useMessageStore';
import { stopVoiceMessagePlayback } from '../../utils/VoiceMessageStorage';

interface ReadAloudActionButtonProps {
  appId?: string;
  msgId?: string;
  part: VoicePlaybackPart;
  selectionMode: SelectionMode;
}

interface UseReadAloudActionOptions {
  appId?: string;
  msgId?: string;
  part: VoicePlaybackPart;
}

export function useReadAloudAction({
  appId,
  msgId,
  part,
}: UseReadAloudActionOptions) {
  const { activePlayback, setActivePlayback, updatePlaybackStatus, clearPlayback } =
    useVoicePlaybackStore();
  const timersRef = React.useRef<ReturnType<typeof setTimeout>[]>([]);

  const isCurrentPlaybackTarget =
    activePlayback.messageId === msgId &&
    activePlayback.part === part &&
    activePlayback.appId === (appId ?? null);
  const readAloudStatus: 'idle' | 'loading' | 'playing' =
    isCurrentPlaybackTarget && activePlayback.mode === 'read_aloud'
      ? activePlayback.status
      : 'idle';

  const clearPlaybackIfCurrent = React.useCallback(() => {
    const playback = useVoicePlaybackStore.getState().activePlayback;
    const isSamePlayback =
      playback.messageId === msgId &&
      playback.part === part &&
      playback.appId === (appId ?? null);
    if (isSamePlayback) {
      clearPlayback();
    }
  }, [appId, clearPlayback, msgId, part]);

  const clearLocalTimers = React.useCallback(() => {
    timersRef.current.forEach(clearTimeout);
    timersRef.current = [];
  }, []);

  const startPlayback = React.useCallback(() => {
    if (!msgId) return;

    // 中文注释：切换到 AI 朗读前先停掉语音消息播放，避免两个播放源同时占用全局播放态。
    stopVoiceMessagePlayback();
    setActivePlayback({
      appId: appId ?? null,
      messageId: msgId,
      part,
      mode: 'read_aloud',
      status: 'loading',
    });

    // 中文注释：当前 AI 朗读仍是前端模拟状态机，因此继续保留原有 loading -> playing -> clear 的时序。
    const loadTimer = setTimeout(() => {
      const playback = useVoicePlaybackStore.getState().activePlayback;
      const isSamePlayback =
        playback.messageId === msgId &&
        playback.part === part &&
        playback.appId === (appId ?? null) &&
        playback.mode === 'read_aloud';
      if (!isSamePlayback) return;

      updatePlaybackStatus('playing');
      const playTimer = setTimeout(() => {
        clearPlaybackIfCurrent();
      }, 3000);
      timersRef.current.push(playTimer);
    }, 1000);
    timersRef.current.push(loadTimer);
  }, [
    appId,
    clearPlaybackIfCurrent,
    msgId,
    part,
    setActivePlayback,
    updatePlaybackStatus,
  ]);

  const stopPlayback = React.useCallback(() => {
    clearPlaybackIfCurrent();
    clearLocalTimers();
  }, [clearLocalTimers, clearPlaybackIfCurrent]);

  const handlePress = React.useCallback(() => {
    if (!msgId) return;

    if (readAloudStatus !== 'idle') {
      stopPlayback();
      return;
    }

    startPlayback();
  }, [msgId, readAloudStatus, startPlayback, stopPlayback]);

  React.useEffect(() => {
    // 中文注释：其它气泡接管全局播放后，需要立即清掉当前气泡的本地计时器，避免 UI 状态残留。
    if (!isCurrentPlaybackTarget && readAloudStatus !== 'idle') {
      clearLocalTimers();
    }
  }, [clearLocalTimers, isCurrentPlaybackTarget, readAloudStatus]);

  React.useEffect(() => {
    return () => clearLocalTimers();
  }, [clearLocalTimers]);

  return {
    handlePress,
    readAloudStatus,
  };
}

export function ReadAloudActionButton({
  appId,
  msgId,
  part,
  selectionMode,
}: ReadAloudActionButtonProps) {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const { handlePress, readAloudStatus } = useReadAloudAction({
    appId,
    msgId,
    part,
  });
  const spinValue = React.useRef(new Animated.Value(0)).current;

  React.useEffect(() => {
    if (readAloudStatus === 'loading') {
      Animated.loop(
        Animated.timing(spinValue, {
          toValue: 1,
          duration: 1000,
          easing: Easing.linear,
          useNativeDriver: true,
        }),
      ).start();
      return;
    }

    spinValue.stopAnimation();
    spinValue.setValue(0);
  }, [readAloudStatus, spinValue]);

  const spin = spinValue.interpolate({
    inputRange: [0, 1],
    outputRange: ['0deg', '360deg'],
  });

  return (
    <Pressable
      style={[
        styles.toolbarButton,
        readAloudStatus === 'playing' && styles.toolbarButtonPlaying,
        selectionMode !== 'none' && styles.toolbarButtonDisabled,
      ]}
      onPress={handlePress}
      disabled={readAloudStatus !== 'idle' || selectionMode !== 'none'}
      hitSlop={8}
    >
      {readAloudStatus === 'loading' && (
        <Animated.View
          style={[
            styles.loadingBorder,
            { transform: [{ rotate: spin }] },
          ]}
        />
      )}
      {readAloudStatus === 'playing' ? (
        <WaveForm />
      ) : (
        <Icon type='voice' size={chatBubbleToolbar.iconSize} />
      )}
    </Pressable>
  );
}

const makeStyles = (colors: ColorTokens) =>
  StyleSheet.create({
    toolbarButton: {
      width: chatBubbleToolbar.buttonSize,
      height: chatBubbleToolbar.buttonSize,
      borderRadius: chatBubbleToolbar.buttonSize / 2,
      backgroundColor: colors.bgToolbarButton,
      alignItems: 'center',
      justifyContent: 'center',
    },
    toolbarButtonPlaying: {
      backgroundColor: colors.bgToolbarButtonPlaying,
    },
    toolbarButtonDisabled: {
      opacity: 0.3,
    },
    loadingBorder: {
      position: 'absolute',
      width: chatBubbleToolbar.buttonSize,
      height: chatBubbleToolbar.buttonSize,
      borderRadius: chatBubbleToolbar.buttonSize / 2,
      borderWidth: 2,
      borderColor: 'transparent',
      borderTopColor: colors.accentBlue,
      borderRightColor: colors.accentBlue,
    },
  });
