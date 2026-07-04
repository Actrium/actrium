import React from 'react';
import {
  Animated,
  Keyboard,
  PanResponder,
  Platform,
  StyleSheet,
  TextInput,
  TouchableOpacity,
  View,
} from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../../components';
import { AttachmentPreviewBar } from './AttachmentPreviewBar';
import { HostIconButton } from '../../../components';
import { Icon } from '../../../components';
import type { ActrInputStyles } from './ActrInput.shared';
import type { PanResponderInstance } from 'react-native';
import { StableTextInput } from './StableTextInput';
import { AuthGateOverlay } from './AuthGateOverlay';
import { getHostShadows } from '../../styles/shadows';
import { useUnreadStore } from '../../store/useUnreadStore';
import { useTab } from '../../contexts/TabContext';
import { exitFullscreen } from '../../../native-bridge';
import {
  getTypographyStyle,
  spacing,
  inputArea,
  iconSize,
} from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { triggerHaptic } from '../../utils/haptics';
import { useChatBottomInsets } from '../../hooks/useChatBottomInsets';

type ActrInputMobileSharedStyles = Pick<
  ActrInputStyles,
  'input' | 'mobileInputText'
>;

interface ActrInputMobileViewProps {
  appId: string;
  sharedStyles: ActrInputMobileSharedStyles;
  attachments: any[];
  removeAttachment: (id: string) => void;
  isAttachmentPanelOpen?: boolean;
  focusInputRequestId?: number;
  isEnterForNewLine: boolean;
  onToggleAttachmentPanel?: () => void;
  onCloseAttachmentPanel?: () => void;
  handleVoiceStart: () => Promise<void>;
  handleVoiceChangeState: (isCancelling: boolean) => void;
  handleVoiceEnd: (isCancelled: boolean) => Promise<void>;
  isVoiceRecording: boolean;
  isCancellingVoice: boolean;
  metering: number;
  inputText: string;
  onClearInput: () => void;
  handleSendRef: React.MutableRefObject<() => void>;
  onAddBookmark?: () => void;
  editingMessageId?: string | null;
  handleCancelEdit?: () => void;
  /** 输入框聚焦前的授权预检，未授权返回 false */
  checkInputAuth?: () => Promise<boolean>;
}

const withOpacity = (color: string, opacity: number) => {
  const normalized = color.trim();
  const clampedOpacity = Math.max(0, Math.min(1, opacity));

  if (normalized.startsWith('#')) {
    let hex = normalized.slice(1);
    if (hex.length === 3) {
      hex = hex
        .split('')
        .map((char) => char + char)
        .join('');
    }
    if (hex.length === 6) {
      const r = parseInt(hex.slice(0, 2), 16);
      const g = parseInt(hex.slice(2, 4), 16);
      const b = parseInt(hex.slice(4, 6), 16);
      return `rgba(${r}, ${g}, ${b}, ${clampedOpacity})`;
    }
  }

  const rgbMatch = normalized.match(
    /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)(?:\s*,\s*[\d.]+\s*)?\)$/,
  );
  if (rgbMatch) {
    const [, r, g, b] = rgbMatch;
    return `rgba(${r}, ${g}, ${b}, ${clampedOpacity})`;
  }

  return color;
};

// 放大加号按钮的点击热区，但不改变按钮本身的视觉尺寸。
const mobilePlusButtonHitSlop = {
  top: 10,
  bottom: 10,
  left: 10,
  right: 10,
} as const;

export default function ActrInputMobileView(props: ActrInputMobileViewProps) {
  const { t } = useTranslation();
  const { tab } = useTab();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const totalUnread = useUnreadStore((state) => state.totalUnread);
  // Mobile 输入区底部留白与提示文案位置统一从聊天底部 hook 获取，保持与 Chat2 列表补偿同步。
  const {
    isKeyboardVisible,
    mobileInputBottomPadding,
    mobileFloatingTextBottom,
  } = useChatBottomInsets(false);
  const {
    sharedStyles,
    appId,
    attachments,
    removeAttachment,
    isAttachmentPanelOpen = false,
    focusInputRequestId,
    isEnterForNewLine,
    onToggleAttachmentPanel,
    onCloseAttachmentPanel,
    handleVoiceStart,
    handleVoiceChangeState,
    handleVoiceEnd,
    inputText,
    onClearInput,
    handleSendRef,
    onAddBookmark,
    editingMessageId,
    handleCancelEdit,
    checkInputAuth,
  } = props;
  const [mobileInputMode, setMobileInputMode] = React.useState<
    'text' | 'voice_hold'
  >('text');
  const [isInputFocused, setIsInputFocused] = React.useState(false);
  const mobileTextInputRef = React.useRef<TextInput | null>(null);
  // 退出编辑态后补发收键盘请求，处理 Android 原生焦点切换慢于 React 状态更新的情况。
  const dismissKeyboardAfterEditExitTimeoutRef =
    React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const hasInputText = inputText.length > 0;
  const isEditing = !!editingMessageId;
  // mobile 状态机：
  // text: 常规输入
  // voice_hold: 整条输入区切为“长按讲话”模式
  const shouldEnableInputHoldToTalk =
    !isEditing &&
    mobileInputMode === 'text' &&
    !hasInputText &&
    !isInputFocused;
  // 语音切换按钮：只要输入框没有文本（包含空格）就显示，不受聚焦状态影响
  const shouldShowVoiceSwitchButton =
    mobileInputMode === 'text' && !hasInputText;

  // 仅在显式切到 voice_hold 时展示语音面板。
  // 默认文本态直接长按录音时，不在手势过程中切换布局，避免 responder 丢失 release/terminate。
  const isVoiceSurfaceMode = mobileInputMode === 'voice_hold';

  const onSwitchToVoiceHoldMode = React.useCallback(() => {
    setMobileInputMode('voice_hold');
    setIsInputFocused(false);
    mobileTextInputRef.current?.blur();
  }, []);

  const onSwitchToTextMode = React.useCallback(() => {
    setMobileInputMode('text');
    setIsInputFocused(false);
  }, []);

  const onFocusTextInput = React.useCallback(() => {
    setMobileInputMode('text');
    setIsInputFocused(true);
    setTimeout(() => {
      mobileTextInputRef.current?.focus();
    }, 0);
  }, []);

  React.useEffect(() => {
    if (!focusInputRequestId) {
      return;
    }

    setMobileInputMode('text');
    setIsInputFocused(true);
    requestAnimationFrame(() => {
      mobileTextInputRef.current?.focus();
    });
  }, [focusInputRequestId]);

  const openAttachmentMenu = React.useCallback(() => {
    if (!isAttachmentPanelOpen) {
      setMobileInputMode('text');
      if (isInputFocused) {
        setIsInputFocused(false);
        mobileTextInputRef.current?.blur();
      }
    }
    onToggleAttachmentPanel?.();
  }, [isAttachmentPanelOpen, isInputFocused, onToggleAttachmentPanel]);

  const handleExitTab = React.useCallback(async () => {
    try {
      await exitFullscreen();
    } catch (error) {
      console.error('[ActrInputMobileView] Failed to add bookmark', error);
    }
  }, [appId, tab.id, onAddBookmark]);

  const handleVoiceStartForMobile = React.useCallback(async () => {
    await handleVoiceStart();
  }, [handleVoiceStart]);

  const handleVoiceEndForMobile = React.useCallback(
    async (isCancelled: boolean) => {
      await handleVoiceEnd(isCancelled);
    },
    [handleVoiceEnd],
  );

  React.useEffect(() => {
    if (inputText.length > 0 && mobileInputMode !== 'text') {
      setMobileInputMode('text');
    }
  }, [inputText.length, mobileInputMode]);

  React.useEffect(() => {
    if (isVoiceSurfaceMode && isAttachmentPanelOpen) {
      onCloseAttachmentPanel?.();
    }
  }, [isAttachmentPanelOpen, isVoiceSurfaceMode, onCloseAttachmentPanel]);

  React.useEffect(() => {
    if (isInputFocused && isAttachmentPanelOpen) {
      onCloseAttachmentPanel?.();
    }
  }, [isAttachmentPanelOpen, isInputFocused, onCloseAttachmentPanel]);

  // 手势实例固定化：识别状态变化时也不重建 responder，
  // 只通过 ref 注入最新回调，避免 release/terminate 丢失。
  const voiceCallbacksRef = React.useRef({
    handleVoiceStart: handleVoiceStartForMobile,
    handleVoiceChangeState,
    handleVoiceEnd: handleVoiceEndForMobile,
    onFocusTextInput,
  });
  React.useEffect(() => {
    voiceCallbacksRef.current = {
      handleVoiceStart: handleVoiceStartForMobile,
      handleVoiceChangeState,
      handleVoiceEnd: handleVoiceEndForMobile,
      onFocusTextInput,
    };
  }, [
    handleVoiceChangeState,
    handleVoiceEndForMobile,
    handleVoiceStartForMobile,
    onFocusTextInput,
  ]);

  const createStableVoicePanResponder = React.useCallback(
    (allowTapFocus: boolean): PanResponderInstance => {
      const longPressThreshold = 180;
      const cancelThreshold = 24;
      const recoverThreshold = 10;
      const tapMoveSlop = 8;
      let timer: ReturnType<typeof setTimeout> | null = null;
      let longPressTriggered = false;
      let isCancelling = false;
      let latestDy = 0;
      let latestDx = 0;
      let startAt = 0;
      let startPageY = 0;
      let startPageX = 0;

      const updateLatestOffset = (
        evt: { nativeEvent?: { pageX?: number; pageY?: number } } | undefined,
        gestureState: { dx: number; dy: number; moveX: number; moveY: number },
      ) => {
        if (startPageY !== 0 && gestureState.moveY !== 0) {
          latestDy = gestureState.moveY - startPageY;
        } else if (startPageY !== 0 && evt?.nativeEvent?.pageY != null) {
          latestDy = evt.nativeEvent.pageY - startPageY;
        } else {
          latestDy = gestureState.dy;
        }

        if (startPageX !== 0 && gestureState.moveX !== 0) {
          latestDx = gestureState.moveX - startPageX;
        } else if (startPageX !== 0 && evt?.nativeEvent?.pageX != null) {
          latestDx = evt.nativeEvent.pageX - startPageX;
        } else {
          latestDx = gestureState.dx;
        }
      };

      const clearTimer = () => {
        if (timer) {
          clearTimeout(timer);
          timer = null;
        }
      };

      const finish = (isTerminated: boolean) => {
        clearTimer();
        if (longPressTriggered) {
          const shouldCancel =
            isTerminated || isCancelling || latestDy < -cancelThreshold;
          void voiceCallbacksRef.current.handleVoiceEnd(shouldCancel);
          voiceCallbacksRef.current.handleVoiceChangeState(false);
        } else if (allowTapFocus) {
          const moved =
            Math.abs(latestDy) > tapMoveSlop ||
            Math.abs(latestDx) > tapMoveSlop;
          const pressedFor = Date.now() - startAt;
          const shouldFocus =
            !isTerminated && !moved && pressedFor < longPressThreshold;
          if (shouldFocus) {
            voiceCallbacksRef.current.onFocusTextInput();
          }
        }
        longPressTriggered = false;
        isCancelling = false;
        latestDy = 0;
        latestDx = 0;
        startAt = 0;
      };

      return PanResponder.create({
        onStartShouldSetPanResponder: () => true,
        onStartShouldSetPanResponderCapture: () => true,
        onMoveShouldSetPanResponder: () => true,
        onMoveShouldSetPanResponderCapture: () => true,
        onPanResponderTerminationRequest: () => false,
        onPanResponderGrant: (evt) => {
          longPressTriggered = false;
          isCancelling = false;
          latestDy = 0;
          latestDx = 0;
          startAt = Date.now();
          startPageY = evt.nativeEvent.pageY;
          startPageX = evt.nativeEvent.pageX;
          voiceCallbacksRef.current.handleVoiceChangeState(false);
          timer = setTimeout(() => {
            longPressTriggered = true;
            triggerHaptic('impactHeavy');
            const shouldCancelNow = latestDy < -cancelThreshold;
            isCancelling = shouldCancelNow;
            voiceCallbacksRef.current.handleVoiceChangeState(shouldCancelNow);
            void voiceCallbacksRef.current.handleVoiceStart();
          }, longPressThreshold);
        },
        onPanResponderMove: (evt, gestureState) => {
          updateLatestOffset(evt, gestureState);
          if (!longPressTriggered) return;
          let nextCancelling = isCancelling;
          if (!isCancelling && latestDy < -cancelThreshold) {
            nextCancelling = true;
          } else if (isCancelling && latestDy > -recoverThreshold) {
            nextCancelling = false;
          }
          if (nextCancelling !== isCancelling) {
            isCancelling = nextCancelling;
            if (nextCancelling) triggerHaptic('impactHeavy');
            voiceCallbacksRef.current.handleVoiceChangeState(nextCancelling);
          }
        },
        onPanResponderRelease: (evt, gestureState) => {
          updateLatestOffset(evt, gestureState);
          finish(false);
        },
        onPanResponderTerminate: (evt, gestureState) => {
          updateLatestOffset(evt, gestureState);
          finish(true);
        },
      });
    },
    [],
  );

  const inputHoldPanResponder = React.useMemo(
    () => createStableVoicePanResponder(true),
    [createStableVoicePanResponder],
  );
  const voiceHoldPanResponder = React.useMemo(
    () => createStableVoicePanResponder(false),
    [createStableVoicePanResponder],
  );

  const hasAttachments = attachments.length > 0;

  // 进入编辑模式时自动聚焦输入框，唤起键盘和光标
  // 退出编辑模式时自动失焦输入框
  const prevEditingMessageIdRef = React.useRef(editingMessageId);
  React.useEffect(() => {
    if (dismissKeyboardAfterEditExitTimeoutRef.current) {
      clearTimeout(dismissKeyboardAfterEditExitTimeoutRef.current);
      dismissKeyboardAfterEditExitTimeoutRef.current = null;
    }

    if (editingMessageId) {
      setMobileInputMode('text');
      setIsInputFocused(true);
    } else if (prevEditingMessageIdRef.current) {
      setMobileInputMode('text');
      setIsInputFocused(false);
      mobileTextInputRef.current?.blur();
      // Android 上编辑态输入框会按 key 重建，ref.blur 可能打到新输入框，需显式收起系统键盘。
      Keyboard.dismiss();
      if (Platform.OS === 'android') {
        dismissKeyboardAfterEditExitTimeoutRef.current = setTimeout(() => {
          dismissKeyboardAfterEditExitTimeoutRef.current = null;
          Keyboard.dismiss();
        }, 120);
      }
    }
    prevEditingMessageIdRef.current = editingMessageId;
    return () => {
      if (dismissKeyboardAfterEditExitTimeoutRef.current) {
        clearTimeout(dismissKeyboardAfterEditExitTimeoutRef.current);
        dismissKeyboardAfterEditExitTimeoutRef.current = null;
      }
    };
  }, [editingMessageId]);
  const shouldShowExitTabButton = !isEditing && !isInputFocused;
  const authOverlayRightInset = shouldShowExitTabButton ? 58 : 0;
  // 附件选择区展开时隐藏底部 AI 提示文案，避免与附件面板在同一垂直区域叠加。
  const shouldShowFloatingText = !isKeyboardVisible && !isAttachmentPanelOpen;
  return (
    <View
      style={[
        styles.mobileSafeArea,
        {
          paddingBottom: mobileInputBottomPadding,
        },
      ]}
    >
      {/* 中文注释：安全区顶部 24 高度单独做透明到半透明的过渡，24 以下区域保持聊天页实色背景。 */}
      <LinearGradient
        pointerEvents="none"
        colors={[
          withOpacity(colors.bgChatPage, 0),
          withOpacity(colors.bgChatPage, 0.9),
        ]}
        start={{ x: 0.5, y: 0 }}
        end={{ x: 0.5, y: 1 }}
        style={styles.mobileSafeAreaTopGradient}
      />
      <View pointerEvents="none" style={styles.mobileSafeAreaSolidBackground} />
      <View style={[styles.mobileContainer, { paddingHorizontal: 16 }]}>
        <View style={styles.mobileInputStack}>
          <View style={styles.mobileInputRow}>
            {/* 中文注释：Phone 端除独立退出 Tab 按钮外，其余输入操作区统一由授权门禁覆盖。 */}
            <AuthGateOverlay
              appId={appId}
              checkInputAuth={checkInputAuth}
              style={{ right: authOverlayRightInset }}
            />

            <View style={styles.mobilePrimaryArea}>
              {!isVoiceSurfaceMode ? (
                <View
                  style={[
                    styles.mobileInputPill,
                    hasAttachments && styles.mobileInputPillWithAttachment,
                  ]}
                >
                  {hasAttachments && (
                    <>
                      <View style={styles.mobileAttachmentArea}>
                        <AttachmentPreviewBar
                          attachments={attachments}
                          removeAttachment={removeAttachment}
                          hideSeparator
                        />
                      </View>
                      <View style={styles.mobileAttachmentDivider} />
                    </>
                  )}
                  <View style={styles.mobileInputInnerRow}>
                    <TouchableOpacity
                      style={styles.mobilePlusButton}
                      activeOpacity={0.7}
                      hitSlop={mobilePlusButtonHitSlop}
                      onPress={openAttachmentMenu}
                    >
                      <Icon
                        type="link"
                        size={inputArea.smallIconSize}
                        color={colors.textIconStrong}
                      />
                    </TouchableOpacity>

                    <View style={styles.mobileTextInputWrap}>
                      <StableTextInput
                        // 编辑态按消息 ID 重建输入框，让 Android 使用挂载期 autoFocus 稳定唤起软键盘。
                        key={editingMessageId ? `editing-${editingMessageId}` : 'compose'}
                        tabId={tab.id}
                        hasAttachments={attachments.length > 0}
                        isEnterForNewLine={isEnterForNewLine}
                        handleSendRef={handleSendRef}
                        mobileMode={true}
                        editable={!shouldEnableInputHoldToTalk}
                        autoFocus={isEditing}
                        inputRefExternal={mobileTextInputRef}
                        onFocus={() => {
                          setMobileInputMode('text');
                          setIsInputFocused(true);
                        }}
                        onBlur={() => setIsInputFocused(false)}
                        desktopInputStyle={sharedStyles.input}
                        mobileInputStyle={sharedStyles.mobileInputText}
                      />
                      {shouldEnableInputHoldToTalk && (
                        // 空文本+未聚焦时，覆盖一层手势区：
                        // 短按走聚焦输入，长按走录音。
                        <View
                          style={styles.mobileInputGestureOverlay}
                          {...inputHoldPanResponder.panHandlers}
                        />
                      )}
                    </View>

                    {hasInputText ? (
                      <TouchableOpacity
                        style={styles.mobileMicButton}
                        onPress={onClearInput}
                      >
                        <Icon
                          type="input_clear"
                          size={inputArea.mediumIconSize}
                          color={colors.textIconSecondary}
                        />
                      </TouchableOpacity>
                    ) : shouldShowVoiceSwitchButton ? (
                      <TouchableOpacity
                        style={styles.mobileMicButton}
                        onPress={onSwitchToVoiceHoldMode}
                      >
                        <Icon
                          type="audio"
                          size={inputArea.smallIconSize}
                          color={colors.textIconStrong}
                        />
                      </TouchableOpacity>
                    ) : (
                      <View style={styles.mobileMicButton} />
                    )}
                  </View>
                </View>
              ) : (
                <View
                  style={[
                    styles.mobileVoiceSurface,
                    hasAttachments && styles.mobileVoiceSurfaceWithAttachment,
                  ]}
                >
                  {hasAttachments && (
                    <>
                      <View style={styles.mobileAttachmentArea}>
                        <AttachmentPreviewBar
                          attachments={attachments}
                          removeAttachment={removeAttachment}
                          hideSeparator
                        />
                      </View>
                      <View style={styles.mobileAttachmentDivider} />
                    </>
                  )}
                  <View style={styles.mobileVoiceInnerRow}>
                    <View style={styles.mobileVoiceModeSpacer} />
                    <View
                      style={styles.mobileVoiceModeMain}
                      {...voiceHoldPanResponder.panHandlers}
                    >
                      <AppText
                        variant="menuLabel"
                        style={styles.mobileVoiceHoldText}
                      >
                        {t('actr.input.holdToSpeak')}
                      </AppText>
                    </View>

                    <TouchableOpacity
                      style={styles.mobileVoiceModeKeyboardButton}
                      onPress={onSwitchToTextMode}
                    >
                      <Icon
                        type="keyboard"
                        size={inputArea.smallIconSize}
                        color={colors.textIconStrong}
                      />
                    </TouchableOpacity>
                  </View>
                </View>
              )}
            </View>

            {/* 右侧取消编辑按钮 - 编辑状态时显示 */}
            {isEditing && (
              <HostIconButton
                iconType="close"
                size={inputArea.sideButtonSize}
                iconSize={inputArea.cancelIconSize}
                iconColor={colors.textIconSecondary}
                onPress={handleCancelEdit}
                style={styles.mobileReservedButton}
              />
            )}

            {/* 右侧按钮 - 非编辑状态时显示 */}
            {!isEditing && (
              <>
                {/* 退出按钮 - 未聚焦时显示 */}
                {!isInputFocused && (
                  <HostIconButton
                    iconType="tab_preview"
                    size={inputArea.sideButtonSize}
                    iconSize={inputArea.sideButtonIconSize}
                    iconColor={colors.textIconStrong}
                    onPress={handleExitTab}
                    style={styles.mobileReservedButton}
                  >
                    {/* 未读消息角标 */}
                    {totalUnread > 0 && (
                      <View style={styles.unreadBadge}>
                        <AppText style={styles.unreadBadgeText}>
                          {totalUnread > 99 ? '99+' : totalUnread}
                        </AppText>
                      </View>
                    )}
                  </HostIconButton>
                )}

                {/* send 按钮 - 聚焦 + 开启回车换行时显示 */}
                {isInputFocused && isEnterForNewLine && (
                  <HostIconButton
                    iconType="send"
                    size={inputArea.sideButtonSize}
                    iconSize={inputArea.sideButtonIconSize}
                    iconColor={
                      hasInputText
                        ? colors.textPrimary
                        : colors.textIconSecondary
                    }
                    onPress={() => {
                      if (hasInputText) {
                        handleSendRef.current();
                      }
                    }}
                    disabled={!hasInputText}
                    style={styles.mobileReservedButton}
                  />
                )}
              </>
            )}
          </View>
        </View>
      </View>

      {/* 中文注释：键盘弹起或附件选择区展开时隐藏附加文案，避免底部元素同时堆叠。 */}
      {shouldShowFloatingText && (
        <View
          pointerEvents="none"
          style={[
            styles.mobileFloatingText,
            {
              bottom: mobileFloatingTextBottom,
            },
          ]}
        >
          <AppText style={styles.mobileFloatingTextLabel}>
            {t('actr.input.aiGeneratedLabel')}
          </AppText>
        </View>
      )}
    </View>
  );
}

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
    mobileContainer: {
      width: '100%',
    },
    mobileInputStack: {
      width: '100%',
    },
    mobileSafeArea: {
      width: '100%',
      position: 'relative',
    },
    mobileSafeAreaTopGradient: {
      position: 'absolute',
      left: 0,
      right: 0,
      top: 0,
      height: 24,
    },
    mobileSafeAreaSolidBackground: {
      position: 'absolute',
      left: 0,
      right: 0,
      top: 24,
      bottom: 0,
      // backgroundColor: colors.bgChatPage,
    },
    mobileFloatingText: {
      position: 'absolute',
      left: 26,
      right: 26,
      // bottom: Platform.OS === 'ios' ? 0 : -10,
      alignItems: 'center',
    },
    mobileFloatingTextLabel: {
      color: '#A0A0A0',
      fontSize: Platform.OS === 'ios' ? 10 : 8,
    },
    mobileInputRow: {
      flexDirection: 'row',
      alignItems: 'flex-end',
      width: '100%',
      gap: 10,
      position: 'relative',
    },
    mobilePrimaryArea: {
      flex: 1,
      minHeight: inputArea.minHeight,
      position: 'relative',
    },
    mobileInputPill: {
      minHeight: inputArea.minHeight,
      borderRadius: inputArea.borderRadius,
      backgroundColor: colors.bgInputPill,
      ...hostShadows.inputPill,
    },
    mobileInputPillWithAttachment: {
      borderRadius: 20,
    },
    mobileInputInnerRow: {
      flexDirection: 'row',
      alignItems: 'flex-end',
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.sm,
      gap: 10,
    },
    mobileVoiceSurface: {
      minHeight: inputArea.minHeight,
      borderRadius: 24,
      backgroundColor: colors.bgInputPill,
      position: 'relative',
      ...hostShadows.inputPill,
    },
    mobileVoiceSurfaceWithAttachment: {
      borderRadius: 20,
    },
    mobileVoiceInnerRow: {
      flexDirection: 'row',
      alignItems: 'center',
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.sm,
    },
    mobilePlusButton: {
      width: inputArea.smallButtonSize,
      height: inputArea.smallButtonSize,
      borderRadius: inputArea.smallButtonSize / 2,
      alignItems: 'center',
      justifyContent: 'center',
    },
    mobileAttachmentArea: {
      paddingTop: spacing.md,
      paddingHorizontal: spacing.md,
    },
    mobileAttachmentDivider: {
      height: 1,
      backgroundColor: colors.bgToolbarButton,
      marginHorizontal: spacing.md,
      marginTop: spacing.md,
    },
    mobileTextInputWrap: {
      flex: 1,
      alignSelf: 'stretch',
      justifyContent: 'center',
      position: 'relative',
    },
    mobileInputGestureOverlay: {
      position: 'absolute',
      left: 0,
      right: 0,
      top: -4,
      bottom: -4,
      zIndex: 20,
    },
    mobileMicButton: {
      width: inputArea.smallButtonSize,
      height: inputArea.smallButtonSize,
      borderRadius: inputArea.smallButtonSize / 2,
      alignItems: 'center',
      justifyContent: 'center',
    },
    mobileReservedButton: {
      borderWidth: 1,
      borderColor: colors.borderReservedButton,
      backgroundColor: colors.bgInputPill,
      ...hostShadows.inputPill,
    },
    mobileVoiceModeSpacer: {
      width: inputArea.smallButtonSize,
      height: inputArea.smallButtonSize,
    },
    mobileVoiceModeMain: {
      flex: 1,
      height: inputArea.smallButtonSize,
      alignItems: 'center',
      justifyContent: 'center',
    },
    mobileVoiceModeKeyboardButton: {
      width: inputArea.smallButtonSize,
      height: inputArea.smallButtonSize,
      borderRadius: inputArea.smallButtonSize / 2,
      alignItems: 'center',
      justifyContent: 'center',
    },
    mobileVoiceHoldText: {
      ...getTypographyStyle('menuLabel'),
      color: colors.textIconPrimary,
      fontWeight: '500',
    },
    unreadBadge: {
      position: 'absolute',
      top: -12,
      right: -12,
      backgroundColor: colors.danger,
      borderRadius: 10,
      minWidth: 20,
      height: 20,
      paddingHorizontal: 5,
      alignItems: 'center',
      justifyContent: 'center',
      borderWidth: 2,
      borderColor: colors.bgInputMobile,
    },
    unreadBadgeText: {
      color: colors.textInverse,
      ...getTypographyStyle('caption1'),
      fontWeight: '600',
    },
  });
};
