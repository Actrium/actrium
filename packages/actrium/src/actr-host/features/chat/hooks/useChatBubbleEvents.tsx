import React from 'react';
import { useTranslation } from 'react-i18next';
import {
  Alert,
  Dimensions,
  Keyboard,
  LayoutChangeEvent,
  Platform,
} from 'react-native';
import {
  runOnJS,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
import { Icon } from '../../../../components';
import type { ColorTokens } from '../../../../theme/colors';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import type { MenuOption } from '../../shared/LongPressMenu';
import { useVoicePlaybackStore } from '../../../store/useVoicePlaybackStore';
import { useMessageStore, type SelectionMode } from '../../../store/useMessageStore';
import { triggerHaptic } from '../../../utils/haptics';
import {
  parseVoicePayload,
  playVoiceMessageFile,
  stopVoiceMessagePlayback,
} from '../../../utils/VoiceMessageStorage';
import { useReadAloudPlayback } from './useReadAloudPlayback';

const SHOW_AI_READ_ALOUD_ENTRY = Platform.OS === 'ios';

type MenuAnchor = {
  x: number;
  y: number;
};

interface UseChatBubbleEventsParams {
  appId?: string;
  attachments?: string;
  colors: ColorTokens;
  content?: string;
  isQuestion: boolean;
  msgId?: string;
  onCopy?: () => void;
  onEdit?: () => void;
  onFeedback?: () => void;
  parsedVoicePayload: ReturnType<typeof parseVoicePayload>;
  selectionMode: SelectionMode;
  t: ReturnType<typeof useTranslation>['t'];
  tabId?: string;
  type: 'question' | 'answer';
  voicePayload?: string;
}

export const useChatBubbleEvents = ({
  appId,
  attachments,
  colors,
  content,
  isQuestion,
  msgId,
  onCopy,
  onEdit,
  onFeedback,
  parsedVoicePayload,
  selectionMode,
  t,
  tabId,
  type,
  voicePayload,
}: UseChatBubbleEventsParams) => {
  const insets = useSafeAreaInsets();
  const { activePlayback, setActivePlayback, clearPlayback } =
    useVoicePlaybackStore();
  const enterSelectionMode = useMessageStore((s) => s.enterSelectionMode);
  const [isCopied, setIsCopied] = React.useState(false);
  const [showMoreMenu, setShowMoreMenu] = React.useState(false);
  const [moreMenuPosition, setMoreMenuPosition] =
    React.useState<MenuAnchor | null>(null);
  const [moreMenuSize, setMoreMenuSize] = React.useState({
    width: 228,
    height: 68,
  });
  const moreMenuPendingActionRef = React.useRef<(() => void) | null>(null);
  const moreMenuBackdropOpacity = useSharedValue(0);
  const moreMenuOpacity = useSharedValue(0);
  const moreMenuBackdropAnimatedStyle = useAnimatedStyle(() => ({
    opacity: moreMenuBackdropOpacity.value,
  }));
  const moreMenuAnimatedStyle = useAnimatedStyle(() => ({
    opacity: moreMenuOpacity.value,
  }));

  const bubblePart = type;
  const isCurrentPlaybackTarget =
    activePlayback.messageId === msgId &&
    activePlayback.part === bubblePart &&
    activePlayback.appId === (appId ?? null);
  const isVoiceBubblePlaying =
    isCurrentPlaybackTarget && activePlayback.mode === 'voice_message';

  const readAloudAction = useReadAloudPlayback({
    appId,
    msgId,
    part: bubblePart,
    text: content,
  });

  const shouldShowReadAloudEntry =
    SHOW_AI_READ_ALOUD_ENTRY &&
    !isQuestion &&
    !!content?.trim() &&
    readAloudAction.isSupported;

  const beginPlayback = React.useCallback(
    (status: 'loading' | 'playing' = 'playing') => {
      if (!msgId) return;
      // Toggle the global target to stop local voice file playing before it is played, ensuring that new play-states and actual audio output are matched.
      stopVoiceMessagePlayback();
      setActivePlayback({
        appId: appId ?? null,
        messageId: msgId,
        part: bubblePart,
        mode: 'voice_message',
        status,
      });
    },
    [appId, bubblePart, msgId, setActivePlayback],
  );

  const clearPlaybackIfCurrent = React.useCallback(() => {
    const playback = useVoicePlaybackStore.getState().activePlayback;
    const isSamePlayback =
      playback.messageId === msgId &&
      playback.part === bubblePart &&
      playback.appId === (appId ?? null);
    if (isSamePlayback) {
      clearPlayback();
    }
  }, [appId, bubblePart, clearPlayback, msgId]);

  const cleanupMoreMenuPosition = React.useCallback(() => {
    setShowMoreMenu(false);
    setMoreMenuPosition(null);
    if (moreMenuPendingActionRef.current) {
      const pendingAction = moreMenuPendingActionRef.current;
      moreMenuPendingActionRef.current = null;
      pendingAction();
    }
  }, []);

  const closeMoreMenu = React.useCallback(
    (callback?: () => void) => {
      if (callback) {
        moreMenuPendingActionRef.current = callback;
      }
      moreMenuBackdropOpacity.value = withTiming(0, { duration: 150 });
      moreMenuOpacity.value = withTiming(0, { duration: 150 }, (isFinished) => {
        if (isFinished) {
          runOnJS(cleanupMoreMenuPosition)();
        }
      });
    },
    [cleanupMoreMenuPosition, moreMenuBackdropOpacity, moreMenuOpacity],
  );

  const handleMoreMenuBackdropPress = React.useCallback(() => {
    closeMoreMenu();
  }, [closeMoreMenu]);

  const handleMoreMenuLayout = React.useCallback(
    (event: LayoutChangeEvent) => {
      const { width, height } = event.nativeEvent.layout;
      if (
        Math.abs(width - moreMenuSize.width) > 1 ||
        Math.abs(height - moreMenuSize.height) > 1
      ) {
        setMoreMenuSize({ width, height });
      }
      if (height > 0) {
        moreMenuOpacity.value = withTiming(1, { duration: 150 });
      }
    },
    [moreMenuOpacity, moreMenuSize.height, moreMenuSize.width],
  );

  const handleMoreMenuFeedbackPress = React.useCallback(() => {
    closeMoreMenu(() => {
      onFeedback?.();
    });
  }, [closeMoreMenu, onFeedback]);

  const handleToolbarCopy = React.useCallback(() => {
    if (selectionMode !== 'none') return;
    setTimeout(() => {
      onCopy?.();
      setIsCopied(true);
    }, 0);
  }, [onCopy, selectionMode]);

  const handleVoiceBubblePress = React.useCallback(async () => {
    if (!msgId || !parsedVoicePayload) return;

    if (isVoiceBubblePlaying) {
      stopVoiceMessagePlayback();
      clearPlaybackIfCurrent();
      return;
    }

    try {
      beginPlayback();
      await playVoiceMessageFile(voicePayload);
    } catch (error) {
      Alert.alert(
        t('common.hint'),
        error instanceof Error ? error.message : t('common.error'),
      );
    } finally {
      clearPlaybackIfCurrent();
    }
  }, [
    beginPlayback,
    clearPlaybackIfCurrent,
    isVoiceBubblePlaying,
    msgId,
    parsedVoicePayload,
    t,
    voicePayload,
  ]);

  const handleNormalPress = React.useCallback(() => {
    // Voice bubble clicks and long presses to move the outer packagings to avoid the inner layer Pressable Take over long menu gestures.
    Keyboard.dismiss();
    if (parsedVoicePayload) {
      void handleVoiceBubblePress();
    }
  }, [handleVoiceBubblePress, parsedVoicePayload]);

  const handleEnterShareSelection = React.useCallback(() => {
    if (!msgId || !tabId) return;
    enterSelectionMode(msgId, tabId, undefined, 'share');
  }, [enterSelectionMode, msgId, tabId]);

  const handleEnterDeleteSelection = React.useCallback(() => {
    if (!msgId || !tabId) return;
    const defaultPart = attachments && !content ? 'attachments' : undefined;
    enterSelectionMode(msgId, tabId, defaultPart, 'delete');
  }, [attachments, content, enterSelectionMode, msgId, tabId]);

  const menuOptions = React.useMemo((): MenuOption[] => {
    const options: MenuOption[] = [];
    if (isQuestion && onEdit) {
      options.push({
        label: t('common.edit'),
        onPress: onEdit,
        // Android Press menu with a long waiting time Modal Focus input box after completely closed, otherwise the system keyboard may not respond to programization focus.
        invokeAfterClose: true,
        icon: <Icon type="edit" size={16} />,
      });
    }
    if (onCopy && !attachments?.length) {
      options.push({
        label: t('common.copy'),
        onPress: onCopy,
        icon: <Icon type="copy" size={16} />,
      });
    }
    if (msgId && tabId) {
      options.push({
        label: t('actr.chat.share'),
        onPress: handleEnterShareSelection,
        icon: <Icon type="share" size={16} />,
      });
    }
    if (shouldShowReadAloudEntry) {
      options.push({
        label: t('actr.chat.readAloud'),
        onPress: () => {
          void readAloudAction.handlePress();
        },
        icon: <Icon type="voice" size={16} />,
      });
    }
    if (!isQuestion) {
      options.push({
        label: t('common.more'),
        onPress: (position?: MenuAnchor) => {
          if (!position) return;
          setMoreMenuPosition(position);
          setMoreMenuSize((prev) => ({ ...prev, height: 0 }));
          moreMenuBackdropOpacity.value = 0;
          moreMenuOpacity.value = 0;
          triggerHaptic('impactHeavy');
          setShowMoreMenu(true);
        },
        invokeAfterClose: true,
        icon: <Icon type="more" size={16} />,
      });
    }
    if (msgId && tabId) {
      options.push({
        label: t('common.delete'),
        onPress: handleEnterDeleteSelection,
        icon: <Icon type="delete" size={16} color={colors.danger} />,
        textColor: colors.danger,
      });
    }
    return options;
  }, [
    attachments,
    colors.danger,
    handleEnterDeleteSelection,
    handleEnterShareSelection,
    isQuestion,
    moreMenuBackdropOpacity,
    moreMenuOpacity,
    msgId,
    onCopy,
    onEdit,
    readAloudAction,
    shouldShowReadAloudEntry,
    t,
    tabId,
  ]);

  const resolvedMoreMenuStyle = React.useMemo(() => {
    if (!moreMenuPosition) return null;

    const screenWidth = Dimensions.get('window').width;
    const screenHeight = Dimensions.get('window').height;
    const horizontalMargin = 20;
    const verticalMargin = 20;
    const topBoundary = insets.top + verticalMargin;
    const bottomBoundary = screenHeight - insets.bottom - verticalMargin;

    // The secondary menu also has the original contact point as the anchor, and the first long menu-by-menu keeps the same set of positioning syntax.
    const top = Math.min(
      Math.max(moreMenuPosition.y - moreMenuSize.height / 2, topBoundary),
      Math.max(topBoundary, bottomBoundary - moreMenuSize.height),
    );
    const left = Math.min(
      Math.max(moreMenuPosition.x - moreMenuSize.width / 2, horizontalMargin),
      screenWidth - horizontalMargin - moreMenuSize.width,
    );

    return {
      top,
      left,
      width: moreMenuSize.width,
      position: 'absolute' as const,
    };
  }, [insets.bottom, insets.top, moreMenuPosition, moreMenuSize.height, moreMenuSize.width]);

  React.useEffect(() => {
    if (showMoreMenu) {
      moreMenuBackdropOpacity.value = withTiming(1, { duration: 200 });
    }
  }, [moreMenuBackdropOpacity, showMoreMenu]);

  React.useEffect(() => {
    if (isCopied) {
      const timer = setTimeout(() => setIsCopied(false), 1000);
      return () => clearTimeout(timer);
    }
  }, [isCopied]);

  return {
    handleEnterDeleteSelection,
    handleEnterShareSelection,
    handleMoreMenuBackdropPress,
    handleMoreMenuFeedbackPress,
    handleMoreMenuLayout,
    handleNormalPress,
    handleToolbarCopy,
    isCopied,
    isVoiceBubblePlaying,
    menuOptions,
    moreMenuAnimatedStyle,
    moreMenuBackdropAnimatedStyle,
    readAloudStatus: readAloudAction.readAloudStatus,
    resolvedMoreMenuStyle,
    shouldShowReadAloudEntry,
    showMoreMenu,
    triggerReadAloud: readAloudAction.handlePress,
  };
};
