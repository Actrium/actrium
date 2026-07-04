import React from 'react';
import { Alert, Platform } from 'react-native';
import { useTranslation } from 'react-i18next';
import {
  TTSEvents,
  speak,
  stopTTS,
  type TTSSpeakOptions,
  type TTSFinishEvent,
} from '../../../../native-bridge';
import {
  useVoicePlaybackStore,
  type IVoicePlaybackState,
  type VoicePlaybackPart,
  type VoicePlaybackStatus,
} from '../../../store/useVoicePlaybackStore';
import { stopVoiceMessagePlayback } from '../../../utils/VoiceMessageStorage';

interface ReadAloudTarget {
  appId?: string;
  msgId?: string;
  part: VoicePlaybackPart;
  text?: string;
  options?: TTSSpeakOptions;
}

const IS_IOS = Platform.OS === 'ios';

let isStoppingCurrentReadAloud = false;
let hasObservedNativeStartForActive = false;

function isReadAloudPlayback(
  playback: IVoicePlaybackState | null | undefined,
): boolean {
  return playback?.mode === 'read_aloud' && playback.status !== 'idle';
}

function hasReadAloudPlayback(
  playback: IVoicePlaybackState | null | undefined,
): playback is IVoicePlaybackState & {
  mode: 'read_aloud';
  status: Exclude<VoicePlaybackStatus, 'idle'>;
} {
  return isReadAloudPlayback(playback);
}

function isSameReadAloudTarget(
  playback: IVoicePlaybackState | null | undefined,
  target: ReadAloudTarget,
): boolean {
  return (
    playback?.messageId === target.msgId &&
    playback?.part === target.part &&
    playback?.appId === (target.appId ?? null)
  );
}

function buildReadAloudPlaybackState(
  target: ReadAloudTarget,
  status: Exclude<VoicePlaybackStatus, 'idle'> = 'loading',
): Omit<IVoicePlaybackState, 'status'> & {
  status: Exclude<VoicePlaybackStatus, 'idle'>;
} {
  return {
    appId: target.appId ?? null,
    messageId: target.msgId ?? null,
    part: target.part,
    mode: 'read_aloud',
    status,
    text: target.text,
    ttsOptions: target.options,
  };
}

function clearActivePlaybackIfMatch(target: ReadAloudTarget): void {
  const { activePlayback, clearPlayback } = useVoicePlaybackStore.getState();
  if (isSameReadAloudTarget(activePlayback, target)) {
    clearPlayback();
  }
}

function resetNativePlaybackTracking(): void {
  isStoppingCurrentReadAloud = false;
  hasObservedNativeStartForActive = false;
}

export async function stopAllVoicePlayback(
  reason: string,
): Promise<void> {
  const {
    activePlayback,
    pendingPlayback,
    clearPlayback,
    clearPendingPlayback,
  } = useVoicePlaybackStore.getState();

  if (activePlayback.status !== 'idle' || pendingPlayback) {
    console.log('[useReadAloudPlayback] stop all voice playback:', {
      reason,
      activePlayback,
      pendingPlayback,
    });
  }

  // 本地语音消息播放和原生 TTS 是两条独立音频通道，
  // 这里先统一停止本地语音文件播放，避免后续录音/切 tab 时仍有残留声音输出。
  stopVoiceMessagePlayback();
  // pendingPlayback 表示“上一段真正结束后准备接管播放的目标”，
  // 主动全量停止时必须一起清掉，避免 stop 完又被 finish 事件重新拉起下一条。
  clearPendingPlayback();
  // activePlayback 是当前 UI 播放态的唯一来源，
  // 主动停止后立即清空，保证按钮状态和真实播放器状态同步回 idle。
  clearPlayback();
  resetNativePlaybackTracking();

  if (activePlayback.mode === 'read_aloud' && activePlayback.status !== 'idle') {
    try {
      // 只有当前活跃源是 read_aloud 时才需要通知 native 停止 AVSpeechSynthesizer，
      // 否则只清 JS store 会导致原生仍继续播报。
      await stopTTS();
    } catch (error) {
      console.error('[useReadAloudPlayback] failed to stop native TTS:', {
        reason,
        error,
      });
    }
  }
}

function shouldConsumeIdleEvent(activePlayback: IVoicePlaybackState): boolean {
  if (!isReadAloudPlayback(activePlayback)) {
    return false;
  }

  // active 处于 loading 时，可能正夹在“外部播放结束 -> 当前消息 start”之间，
  // 这时先到的 finish/pause/error 事件不一定属于当前消息，不能直接把当前气泡打回 idle。
  return hasObservedNativeStartForActive || isStoppingCurrentReadAloud;
}

async function startReadAloudPlayback(target: ReadAloudTarget): Promise<void> {
  const playbackState = buildReadAloudPlaybackState(target, 'loading');
  const { setActivePlayback, clearPendingPlayback } =
    useVoicePlaybackStore.getState();

  // 真正发起原生 speak 前先把全局 active 目标切过去，
  // 这样 onTTSStart / onTTSFinish 回来时，JS 侧已经能准确知道当前归属。
  setActivePlayback(playbackState);
  clearPendingPlayback();
  hasObservedNativeStartForActive = false;
  isStoppingCurrentReadAloud = false;

  try {
    console.log('[useReadAloudPlayback] start read aloud playback:', target);
    await speak(
      target.text ?? '',
      Object.assign({ language: 'zh-CN' }, target.options ?? {}),
    );
  } catch (error) {
    resetNativePlaybackTracking();
    clearActivePlaybackIfMatch(target);
    throw error;
  }
}

async function requestStopCurrentReadAloud(): Promise<void> {
  if (isStoppingCurrentReadAloud) {
    console.log('[useReadAloudPlayback] skip duplicate stop request');
    return;
  }

  isStoppingCurrentReadAloud = true;

  try {
    console.log('[useReadAloudPlayback] request stop current read aloud');
    await stopTTS();
  } catch (error) {
    isStoppingCurrentReadAloud = false;
    throw error;
  }
}

function resolvePlaybackStatus(
  activePlayback: IVoicePlaybackState,
  pendingPlayback: IVoicePlaybackState | null,
  target: ReadAloudTarget,
): VoicePlaybackStatus {
  if (
    isReadAloudPlayback(activePlayback) &&
    isSameReadAloudTarget(activePlayback, target)
  ) {
    return activePlayback.status;
  }

  if (
    hasReadAloudPlayback(pendingPlayback) &&
    isSameReadAloudTarget(pendingPlayback, target)
  ) {
    return pendingPlayback.status;
  }

  return 'idle';
}

export function useReadAloudPlaybackController(): void {
  React.useEffect(() => {
    if (!IS_IOS) {
      return;
    }

    const startUnsubscribe = TTSEvents.onTTSStart(() => {
      const { activePlayback, updatePlaybackStatus } =
        useVoicePlaybackStore.getState();
      if (!isReadAloudPlayback(activePlayback)) {
        return;
      }

      hasObservedNativeStartForActive = true;
      console.log('[useReadAloudPlayback] native onTTSStart:', activePlayback);
      updatePlaybackStatus('playing');
    });

    const resumeUnsubscribe = TTSEvents.onTTSResume(() => {
      const { activePlayback, updatePlaybackStatus } =
        useVoicePlaybackStore.getState();
      if (!isReadAloudPlayback(activePlayback)) {
        return;
      }

      hasObservedNativeStartForActive = true;
      console.log('[useReadAloudPlayback] native onTTSResume:', activePlayback);
      updatePlaybackStatus('playing');
    });

    const pauseUnsubscribe = TTSEvents.onTTSPause((event) => {
      const { activePlayback, clearPlayback, clearPendingPlayback } =
        useVoicePlaybackStore.getState();
      if (!shouldConsumeIdleEvent(activePlayback)) {
        console.log('[useReadAloudPlayback] ignore native onTTSPause:', {
          event,
          activePlayback,
          hasObservedNativeStartForActive,
          isStoppingCurrentReadAloud,
        });
        return;
      }

      console.log('[useReadAloudPlayback] native onTTSPause:', {
        event,
        activePlayback,
      });
      resetNativePlaybackTracking();
      clearPlayback();
      clearPendingPlayback();
    });

    const finishUnsubscribe = TTSEvents.onTTSFinish((event: TTSFinishEvent) => {
      const {
        activePlayback,
        pendingPlayback,
        clearPlayback,
        clearPendingPlayback,
      } = useVoicePlaybackStore.getState();

      if (!shouldConsumeIdleEvent(activePlayback)) {
        console.log('[useReadAloudPlayback] ignore native onTTSFinish:', {
          event,
          activePlayback,
          hasObservedNativeStartForActive,
          isStoppingCurrentReadAloud,
        });
        return;
      }

      console.log('[useReadAloudPlayback] native onTTSFinish:', {
        event,
        activePlayback,
        pendingPlayback,
      });
      // 原生 finish/cancel 才代表上一段朗读已经真正释放播报器，
      // 因此切换到 pending 目标必须在这个事件里推进，不能在 stop() 返回后提前 speak。
      clearPlayback();
      resetNativePlaybackTracking();

      const nextPendingPlayback = pendingPlayback;
      if (
        hasReadAloudPlayback(nextPendingPlayback) &&
        nextPendingPlayback.text
      ) {
        console.log(
          '[useReadAloudPlayback] promote pending playback:',
          nextPendingPlayback,
        );
        void startReadAloudPlayback({
          appId: nextPendingPlayback.appId ?? undefined,
          msgId: nextPendingPlayback.messageId ?? undefined,
          part: nextPendingPlayback.part as VoicePlaybackPart,
          text: nextPendingPlayback.text,
          options: nextPendingPlayback.ttsOptions,
        }).catch((error) => {
          console.error(
            '[useReadAloudPlayback] start pending playback failed:',
            error,
            event,
          );
        });
        return;
      }

      clearPendingPlayback();
    });

    const errorUnsubscribe = TTSEvents.onTTSError((event) => {
      const { activePlayback, clearPlayback, clearPendingPlayback } =
        useVoicePlaybackStore.getState();
      if (!shouldConsumeIdleEvent(activePlayback)) {
        console.log('[useReadAloudPlayback] ignore native onTTSError:', {
          event,
          activePlayback,
          hasObservedNativeStartForActive,
          isStoppingCurrentReadAloud,
        });
        return;
      }

      resetNativePlaybackTracking();
      clearPlayback();
      clearPendingPlayback();
      console.error('[useReadAloudPlayback] TTS error:', event);
    });

    return () => {
      startUnsubscribe();
      resumeUnsubscribe();
      pauseUnsubscribe();
      finishUnsubscribe();
      errorUnsubscribe();
    };
  }, []);
}

export function useReadAloudPlayback(target: ReadAloudTarget) {
  const { t } = useTranslation();
  const activePlayback = useVoicePlaybackStore((state) => state.activePlayback);
  const pendingPlayback = useVoicePlaybackStore(
    (state) => state.pendingPlayback,
  );
  const setPendingPlayback = useVoicePlaybackStore(
    (state) => state.setPendingPlayback,
  );
  const clearPendingPlayback = useVoicePlaybackStore(
    (state) => state.clearPendingPlayback,
  );
  const clearPlayback = useVoicePlaybackStore((state) => state.clearPlayback);

  const readAloudStatus = React.useMemo(
    () => resolvePlaybackStatus(activePlayback, pendingPlayback, target),
    [activePlayback, pendingPlayback, target],
  );

  const handlePress = React.useCallback(async () => {
    const text = target.text?.trim();
    if (!IS_IOS || !target.msgId || !text) {
      console.log(
        '[useReadAloudPlayback] ignore press due to unsupported target:',
        {
          isIOS: IS_IOS,
          msgId: target.msgId,
          hasText: !!text,
        },
      );
      return;
    }

    const { activePlayback: currentActivePlayback } =
      useVoicePlaybackStore.getState();

    // 再次点击已经排队等待接管的目标时，只撤销排队，不影响当前仍在播的内容。
    if (
      isReadAloudPlayback(pendingPlayback) &&
      isSameReadAloudTarget(pendingPlayback, target)
    ) {
      console.log(
        '[useReadAloudPlayback] cancel pending playback target:',
        target,
      );
      clearPendingPlayback();
      return;
    }

    if (
      isReadAloudPlayback(currentActivePlayback) &&
      isSameReadAloudTarget(currentActivePlayback, target)
    ) {
      console.log(
        '[useReadAloudPlayback] stop current playback target:',
        target,
      );
      clearPendingPlayback();

      try {
        await requestStopCurrentReadAloud();
      } catch (error) {
        clearActivePlaybackIfMatch(target);
        Alert.alert(
          t('common.hint'),
          error instanceof Error ? error.message : t('common.error'),
        );
      }
      return;
    }

    if (
      currentActivePlayback.mode === 'voice_message' &&
      currentActivePlayback.status !== 'idle'
    ) {
      console.log(
        '[useReadAloudPlayback] stop voice message playback before read aloud:',
        {
          currentActivePlayback,
          nextTarget: target,
        },
      );
      stopVoiceMessagePlayback();
      clearPlayback();
    }

    if (isReadAloudPlayback(currentActivePlayback)) {
      // 有其它朗读正在播时，先把最新点击目标记到 pending，
      // 等当前播放收到原生 finish/cancel 事件后，再由 controller 推进到下一段。
      console.log('[useReadAloudPlayback] queue pending playback target:', {
        currentActivePlayback,
        nextTarget: target,
      });
      setPendingPlayback(
        buildReadAloudPlaybackState({ ...target, text }, 'loading'),
      );

      try {
        await requestStopCurrentReadAloud();
      } catch (error) {
        clearPendingPlayback();
        Alert.alert(
          t('common.hint'),
          error instanceof Error ? error.message : t('common.error'),
        );
      }
      return;
    }

    try {
      await startReadAloudPlayback({ ...target, text });
    } catch (error) {
      Alert.alert(
        t('common.hint'),
        error instanceof Error ? error.message : t('common.error'),
      );
    }
  }, [
    clearPendingPlayback,
    clearPlayback,
    pendingPlayback,
    setPendingPlayback,
    t,
    target,
  ]);

  return {
    handlePress,
    readAloudStatus,
    isSupported: IS_IOS,
  };
}
