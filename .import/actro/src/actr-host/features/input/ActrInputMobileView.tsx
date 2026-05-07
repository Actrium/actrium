import React from 'react';
import {
  Animated,
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
import { HOST_SHADOWS } from '../../styles/shadows';
import { useUnreadStore } from '../../store/useUnreadStore';
import { useTab } from '../../contexts/TabContext';
import { exitFullscreen } from '../../../native-bridge';
import SafeBottomView from '../../../components/SafeBottomView';
import MobileAttachmentMenu from './MobileAttachmentMenu';
import {
  attachmentActionMenu,
  getTypographyStyle,
  spacing,
  inputArea,
  iconSize,
} from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { triggerHaptic } from '../../utils/haptics';
import { useKeyboardInset } from '../../../hooks/useKeyboardInset';

type ActrInputMobileSharedStyles = Pick<ActrInputStyles, 'input' | 'mobileInputText'>;

interface ActrInputMobileViewProps {
  appId: string;
  sharedStyles: ActrInputMobileSharedStyles;
  attachments: any[];
  removeAttachment: (id: string) => void;
  setShowMapModal: (visible: boolean) => void;
  showSearchEntry: boolean;
  showLocationEntry: boolean;
  showNewlineEntry: boolean;
  isWebSearchActive: boolean;
  setIsWebSearchActive: (active: boolean) => void;
  isLocationActive: boolean;
  setIsLocationActive: (active: boolean) => void;
  isEnterForNewLine: boolean;
  setIsEnterForNewLine: (active: boolean) => void;
  takePhoto: () => void;
  pickImage: () => void;
  pickDocument: () => void;
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

export default function ActrInputMobileView(props: ActrInputMobileViewProps) {
  const { t } = useTranslation();
  const { tab } = useTab();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const totalUnread = useUnreadStore((state) => state.totalUnread);
  const { isKeyboardVisible } = useKeyboardInset();
  const {
    sharedStyles,
    appId,
    attachments,
    removeAttachment,
    setShowMapModal,
    showSearchEntry,
    showLocationEntry,
    showNewlineEntry,
    isWebSearchActive,
    setIsWebSearchActive,
    isLocationActive,
    setIsLocationActive,
    isEnterForNewLine,
    setIsEnterForNewLine,
    takePhoto,
    pickImage,
    pickDocument,
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
  const [mobileInputMode, setMobileInputMode] = React.useState<'text' | 'voice_hold'>(
    'text',
  );
  const [isInputFocused, setIsInputFocused] = React.useState(false);
  const [isAttachmentMenuOpen, setIsAttachmentMenuOpen] = React.useState(false);
  const [attachmentMenuAnchorRect, setAttachmentMenuAnchorRect] = React.useState<{
    x: number;
    y: number;
    width: number;
    height: number;
  } | null>(null);
  const mobileTextInputRef = React.useRef<TextInput | null>(null);
  const mobileInputPillRef = React.useRef<View | null>(null);
  const attachmentActionLockRef = React.useRef(false);
  const webSearchIconTranslateY = React.useRef(new Animated.Value(0)).current;
  const locationIconTranslateY = React.useRef(new Animated.Value(0)).current;
  const enterIconTranslateY = React.useRef(new Animated.Value(0)).current;

  const hasInputText = inputText.length > 0;
  // mobile 状态机：
  // text: 常规输入
  // voice_hold: 整条输入区切为“长按讲话”模式
  const shouldEnableInputHoldToTalk =
    mobileInputMode === 'text' && !hasInputText && !isInputFocused;
  const shouldShowVoiceSwitchButton =
    mobileInputMode === 'text' && !hasInputText && !isInputFocused;

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

  const playMenuIconBounce = React.useCallback((translateY: Animated.Value) => {
    translateY.stopAnimation();
    translateY.setValue(0);
    // 中文注释：利用菜单关闭前的延迟窗口，让图标先轻微上跳再回落，保证反馈在菜单消失前可见。
    Animated.sequence([
      Animated.timing(translateY, {
        toValue: -7,
        duration: 70,
        useNativeDriver: true,
      }),
      Animated.timing(translateY, {
        toValue: 2.5,
        duration: 55,
        useNativeDriver: true,
      }),
      Animated.timing(translateY, {
        toValue: 0,
        duration: 45,
        useNativeDriver: true,
      }),
    ]).start();
  }, []);

  const handleDelayedToggleMenuAction = React.useCallback(
    (
      toggle: () => void,
      translateY: Animated.Value,
      close: (callback?: () => void, options?: { delayMs?: number }) => void,
    ) => {
      toggle();
      playMenuIconBounce(translateY);
      close(undefined, { delayMs: 200 });
    },
    [playMenuIconBounce],
  );

  const closeAttachmentMenu = React.useCallback(() => {
    setIsAttachmentMenuOpen(false);
  }, []);

  const openAttachmentMenu = React.useCallback(() => {
    if (attachmentActionLockRef.current) {
      return;
    }
    attachmentActionLockRef.current = true;
    setMobileInputMode('text');
    setIsInputFocused(false);
    mobileTextInputRef.current?.blur();

    // 中文注释：菜单位置需要基于整块输入胶囊测量，而不是左侧按钮自身，否则无法满足“左对齐输入区域”的要求。
    mobileInputPillRef.current?.measureInWindow((x, y, width, height) => {
      attachmentActionLockRef.current = false;
      if (width <= 0 || height <= 0) {
        return;
      }
      setAttachmentMenuAnchorRect({ x, y, width, height });
      setIsAttachmentMenuOpen(true);
    });
  }, []);

  const renderBouncingMenuIcon = React.useCallback(
    (
      type: string,
      active: boolean,
      color: string,
      size: number,
      translateY: Animated.Value,
    ) => (
      <Animated.View style={{ transform: [{ translateY }] }}>
        <Icon type={type} active={active} size={size} color={color} />
      </Animated.View>
    ),
    [],
  );



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
    if (isVoiceSurfaceMode && isAttachmentMenuOpen) {
      setIsAttachmentMenuOpen(false);
    }
  }, [isAttachmentMenuOpen, isVoiceSurfaceMode]);

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
          const shouldCancel = isTerminated || isCancelling || latestDy < -cancelThreshold;
          void voiceCallbacksRef.current.handleVoiceEnd(shouldCancel);
          voiceCallbacksRef.current.handleVoiceChangeState(false);
        } else if (allowTapFocus) {
          const moved =
            Math.abs(latestDy) > tapMoveSlop || Math.abs(latestDx) > tapMoveSlop;
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
  const isEditing = !!editingMessageId;

  // 进入编辑模式时自动聚焦输入框，唤起键盘和光标
  // 退出编辑模式时自动失焦输入框
  const prevEditingMessageIdRef = React.useRef(editingMessageId);
  React.useEffect(() => {
    if (editingMessageId) {
      setMobileInputMode('text');
      setTimeout(() => {
        mobileTextInputRef.current?.focus();
      }, 100);
    } else if (prevEditingMessageIdRef.current) {
      mobileTextInputRef.current?.blur();
    }
    prevEditingMessageIdRef.current = editingMessageId;
  }, [editingMessageId]);
  const shouldShowExitTabButton = !isEditing && !isInputFocused;
  const authOverlayRightInset = shouldShowExitTabButton ? 58 : 0;

  return (
    <SafeBottomView style={styles.mobileSafeArea} minBottom={10}>
      {/* 中文注释：安全区顶部 24 高度单独做透明到半透明的过渡，24 以下区域保持聊天页实色背景。 */}
      <LinearGradient
        pointerEvents='none'
        colors={[
          withOpacity(colors.bgChatPage, 0),
          withOpacity(colors.bgChatPage, 0.9),
        ]}
        start={{ x: 0.5, y: 0 }}
        end={{ x: 0.5, y: 1 }}
        style={styles.mobileSafeAreaTopGradient}
      />
      <View pointerEvents='none' style={styles.mobileSafeAreaSolidBackground} />
      <View style={[styles.mobileContainer, { paddingHorizontal: 26 }]}>
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
                ref={mobileInputPillRef}
                collapsable={false}
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
                    onPress={openAttachmentMenu}
                  >
                    <Icon type='link' size={inputArea.smallIconSize} color={colors.textIconStrong} />
                  </TouchableOpacity>

                  <MobileAttachmentMenu
                    visible={isAttachmentMenuOpen}
                    anchorRect={attachmentMenuAnchorRect}
                    width={attachmentActionMenu.width}
                    onRequestClose={closeAttachmentMenu}
                  >
                    {(close) => (
                      <View style={styles.mobileDropdownContent}>
                        {showSearchEntry && (
                          <TouchableOpacity
                            style={[
                              styles.mobileMenuItem,
                              isWebSearchActive && styles.mobileMenuItemActive,
                            ]}
                            onPress={() => {
                              handleDelayedToggleMenuAction(
                                () => setIsWebSearchActive(!isWebSearchActive),
                                webSearchIconTranslateY,
                                close,
                              );
                            }}
                          >
                            <View style={styles.mobileMenuItemIcon}>
                              {renderBouncingMenuIcon(
                                'network',
                                isWebSearchActive,
                                isWebSearchActive ? colors.accentBlue : colors.textIconDark,
                                inputArea.smallIconSize,
                                webSearchIconTranslateY,
                              )}
                            </View>
                            <View style={styles.mobileMenuTextBlock}>
                              <AppText variant='menuLabel' style={styles.mobileMenuTitle}>
                                {t('actr.input.webSearch')}
                              </AppText>
                              <AppText style={styles.mobileMenuSubtitle}>
                                {t('actr.input.webSearchTip')}
                              </AppText>
                            </View>
                          </TouchableOpacity>
                        )}

                        {showLocationEntry && (
                          <TouchableOpacity
                            style={[
                              styles.mobileMenuItem,
                              isLocationActive && styles.mobileMenuItemActive,
                            ]}
                            onPress={() => {
                              handleDelayedToggleMenuAction(
                                () => setIsLocationActive(!isLocationActive),
                                locationIconTranslateY,
                                close,
                              );
                            }}
                          >
                            <View style={styles.mobileMenuItemIcon}>
                              {renderBouncingMenuIcon(
                                'share_position',
                                isLocationActive,
                                isLocationActive ? colors.accentBlue : colors.textIconDark,
                                inputArea.smallIconSize,
                                locationIconTranslateY,
                              )}
                            </View>
                            <View style={styles.mobileMenuTextBlock}>
                              <AppText variant='menuLabel' style={styles.mobileMenuTitle}>
                                {t('actr.input.shareLocation')}
                              </AppText>
                              <AppText style={styles.mobileMenuSubtitle}>
                                {t('actr.input.shareLocationTip')}
                              </AppText>
                            </View>
                          </TouchableOpacity>
                        )}

                        <TouchableOpacity
                          style={styles.mobileMenuItem}
                          onPress={() => {
                            close(() => {
                              setShowMapModal(true);
                            });
                          }}
                        >
                          <View style={styles.mobileMenuItemIcon}>
                            <Icon type='geolocation' size={inputArea.mediumIconSize} color={colors.textIconDark} />
                          </View>
                          <View style={styles.mobileMenuTextBlock}>
                            <AppText variant='menuLabel' style={styles.mobileMenuTitle}>
                              {t('actr.input.location')}
                            </AppText>
                            <AppText style={styles.mobileMenuSubtitle}>{t('actr.input.sendLocation')}</AppText>
                          </View>
                        </TouchableOpacity>

                        {showNewlineEntry ? <View style={styles.mobileMenuDivider} /> : null}

                        {showNewlineEntry && (
                          <TouchableOpacity
                            style={[
                              styles.mobileMenuItem,
                              isEnterForNewLine && styles.mobileMenuItemActive,
                            ]}
                            onPress={() => {
                              handleDelayedToggleMenuAction(
                                () => setIsEnterForNewLine(!isEnterForNewLine),
                                enterIconTranslateY,
                                close,
                              );
                            }}
                          >
                            <View style={styles.mobileMenuItemIcon}>
                              {renderBouncingMenuIcon(
                                'enter',
                                isEnterForNewLine,
                                isEnterForNewLine ? colors.accentBlue : colors.textIconDark,
                                inputArea.smallIconSize,
                                enterIconTranslateY,
                              )}
                            </View>
                            <View style={styles.mobileMenuTextBlock}>
                              <AppText variant='menuLabel' style={styles.mobileMenuTitle}>
                                {t('actr.input.enterToNewLine')}
                              </AppText>
                              <AppText style={styles.mobileMenuSubtitle}>
                                {t('actr.input.enterToNewLineTip')}
                              </AppText>
                            </View>
                          </TouchableOpacity>
                        )}

                        <View style={styles.mobileMenuDivider} />

                        <View style={styles.mobileQuickActions}>
                          <TouchableOpacity
                            style={styles.mobileQuickAction}
                            onPress={() => {
                              close(takePhoto);
                            }}
                          >
                            <Icon type='camera' size={inputArea.smallIconSize} />
                            <AppText style={styles.mobileQuickActionText}>{t('actr.input.camera')}</AppText>
                          </TouchableOpacity>
                          <TouchableOpacity
                            style={styles.mobileQuickAction}
                            onPress={() => {
                              close(pickImage);
                            }}
                          >
                            <Icon type='image' size={inputArea.smallIconSize} />
                            <AppText style={styles.mobileQuickActionText}>{t('actr.input.photos')}</AppText>
                          </TouchableOpacity>
                          <TouchableOpacity
                            style={styles.mobileQuickAction}
                            onPress={() => {
                              close(pickDocument);
                            }}
                          >
                            <Icon type='file' size={inputArea.smallIconSize} />
                            <AppText style={styles.mobileQuickActionText}>{t('actr.input.files')}</AppText>
                          </TouchableOpacity>
                        </View>
                      </View>
                    )}
                  </MobileAttachmentMenu>

                  <View style={styles.mobileTextInputWrap}>
                    <StableTextInput
                      tabId={tab.id}
                      hasAttachments={attachments.length > 0}
                      isEnterForNewLine={isEnterForNewLine}
                      handleSendRef={handleSendRef}
                      mobileMode={true}
                      editable={!shouldEnableInputHoldToTalk}
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
                      <Icon type='input_clear' size={inputArea.mediumIconSize} color={colors.textIconSecondary} />
                    </TouchableOpacity>
                  ) : shouldShowVoiceSwitchButton ? (
                    <TouchableOpacity
                      style={styles.mobileMicButton}
                      onPress={onSwitchToVoiceHoldMode}
                    >
                      <Icon type='audio' size={inputArea.smallIconSize} color={colors.textIconStrong} />
                    </TouchableOpacity>
                  ) : (
                    <View style={styles.mobileMicButton} />
                  )}
                </View>
              </View>
            ) : (
              <View style={[
                styles.mobileVoiceSurface,
                hasAttachments && styles.mobileVoiceSurfaceWithAttachment,
              ]}>
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
                     <AppText variant='menuLabel' style={styles.mobileVoiceHoldText}>
                      {t('actr.input.holdToSpeak')}
                    </AppText>
                  </View>

                  <TouchableOpacity
                    style={styles.mobileVoiceModeKeyboardButton}
                    onPress={onSwitchToTextMode}
                  >
                    <Icon
                      type='keyboard'
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
              iconType='close'
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
                  iconType='tab_preview'
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
                  iconType='send'
                  size={inputArea.sideButtonSize}
                  iconSize={inputArea.sideButtonIconSize}
                  iconColor={hasInputText ? colors.textPrimary : colors.textIconSecondary}
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

      {/* 中文注释：键盘弹起时隐藏附加文案，避免与输入态内容同时堆叠。 */}
      {!isKeyboardVisible && (
        <View pointerEvents='none' style={styles.mobileFloatingText}>
          <AppText style={styles.mobileFloatingTextLabel}>
            {t('actr.input.aiGeneratedLabel')}
          </AppText>
        </View>
      )}
    </SafeBottomView>
  );
}

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  mobileContainer: {
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
    backgroundColor: colors.bgChatPage,
  },
  mobileFloatingText: {
    position: 'absolute',
    left: 26,
    right: 26,
    bottom: Platform.OS === 'ios' ? 0 : -10,
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
    ...HOST_SHADOWS.inputPill,
  },
  mobileInputPillWithAttachment: {
    borderRadius: 20,
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
    ...HOST_SHADOWS.inputPill,
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
  mobileDropdownContent: {
    width: attachmentActionMenu.width,
    paddingHorizontal: attachmentActionMenu.contentPaddingHorizontal,
    paddingTop: attachmentActionMenu.contentPaddingTop,
    paddingBottom: attachmentActionMenu.contentPaddingBottom,
  },
  mobileMenuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 14,
    paddingHorizontal: attachmentActionMenu.itemPaddingHorizontal,
    paddingVertical: attachmentActionMenu.itemPaddingVertical,
    marginVertical: 3,
    borderRadius: 20,
  },
  mobileMenuItemActive: {
    backgroundColor: colors.bgMenuItemActive,
  },
  mobileMenuItemIcon: {
    width: 22,
    alignItems: 'center',
    justifyContent: 'center',
  },
  mobileMenuTextBlock: {
    flex: 1,
  },
  mobileMenuTitle: {
    ...getTypographyStyle('menuLabel'),
    color: colors.textIconPrimary,
  },
  mobileMenuSubtitle: {
    ...getTypographyStyle('caption1'),
    color: colors.textTertiary,
    marginTop: spacing.xxxs,
  },
  mobileMenuDivider: {
    height: 1,
    backgroundColor: colors.bgMenuDivider,
    marginVertical: spacing.sm,
    marginHorizontal: 10,
  },
  mobileQuickActions: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingTop: spacing.sm,
    paddingBottom: spacing.xxs,
    alignItems: 'flex-start',
  },
  mobileQuickAction: {
    width: 60,
    alignItems: 'center',
    justifyContent: 'center',
    gap: spacing.xxs,
  },
  mobileQuickActionText: {
    ...getTypographyStyle('body'),
    color: colors.textIconDark,
    fontWeight: '500',
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
    ...HOST_SHADOWS.inputPill,
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
