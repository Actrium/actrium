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

  // Local voice message play and original TTS It's two separate audio channels.
  // We'll stop local voice files and avoid any subsequent recording./Cut tab There are still residual sound outputs.
  stopVoiceMessagePlayback();
  // pendingPlayback Organisation“The target is ready to take over the air after the last one is really over.”,
  // We have to clear it together when the whole of the initiative stops. stop And then it's over. finish The incident re-engaged the next one.
  clearPendingPlayback();
  // activePlayback Current UI The only source of the play-state is the "Face-A-A-T"
  // Quit vacating immediately after the active stop, and ensure that the button status is synchronized with the real player status idle.
  clearPlayback();
  resetNativePlaybackTracking();

  if (activePlayback.mode === 'read_aloud' && activePlayback.status !== 'idle') {
    try {
      // Only current active source is read_aloud It's only a matter of time before you call. native Stop AVSpeechSynthesizer,
      // Or I'll just clear it. JS store This will lead to the continuation of the original broadcast.
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

  // active in loading It's probably right there.“External play is over -> Current Message start”I'm not sure what I'm talking about.
  // It's a first. finish/pause/error The event is not necessarily current. It cannot be hit by the current bubble. idle.
  return hasObservedNativeStartForActive || isStoppingCurrentReadAloud;
}

async function startReadAloudPlayback(target: ReadAloudTarget): Promise<void> {
  const playbackState = buildReadAloudPlaybackState(target, 'loading');
  const { setActivePlayback, clearPendingPlayback } =
    useVoicePlaybackStore.getState();

  // It's a real start. speak Let's get this whole thing under control. active The target is cut.
  // Here. onTTSStart / onTTSFinish I'm not sure if I'm going to be able to get back.JS The side already knows exactly where it belongs.
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
      // Native finish/cancel The last post, which was written by the blogger, is a very interesting example of the fact that the radio was actually released.
      // Switch to pending Target must advance in this event. stop() Early on return speak.
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

    // Clicking again on a target already in line for taking over will only remove the queue, without prejudice to the content currently on the air.
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
      // When other readings are on, first remember the latest hits. pending,
      // When the current play is received, finish/cancel After the event, by controller Move to the next paragraph.
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
