import React from 'react';
import {
  PanResponder,
  StyleSheet,
  Text,
  TextInput,
  TouchableOpacity,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { AttachmentPreviewBar } from '../AttachmentPreviewBar';
import { DropdownMenu } from '../DropdownMenu';
import { Icon } from '../Icon';
import type { ActrInputStyles } from './ActrInput.shared';
import type { PanResponderInstance } from 'react-native';
import { StableTextInput } from './StableTextInput';
import { HOST_SHADOWS } from '../../styles/shadows';
import { BookmarkRepository } from '../../../repository/BookmarkRepository';
import { useUnreadStore } from '../../store/useUnreadStore';
import { useTab } from '../../contexts/TabContext';
import { exitFullscreen } from '../../../native-bridge';
import SafeBottomView from '../SafeBottomView';

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
  isSendOnEnter: boolean;
  setIsSendOnEnter: (active: boolean) => void;
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
}

export default function ActrInputMobileView(props: ActrInputMobileViewProps) {
  const { t } = useTranslation();
  const { tab } = useTab();
  const totalUnread = useUnreadStore((state) => state.totalUnread);
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
    isSendOnEnter,
    setIsSendOnEnter,
    takePhoto,
    pickImage,
    pickDocument,
    handleVoiceStart,
    handleVoiceChangeState,
    handleVoiceEnd,
    isVoiceRecording,
    isCancellingVoice,
    metering,
    inputText,
    onClearInput,
    handleSendRef,
    onAddBookmark,
    editingMessageId,
    handleCancelEdit,
  } = props;
  const [mobileInputMode, setMobileInputMode] = React.useState<'text' | 'voice_hold'>(
    'text',
  );
  const [isInputFocused, setIsInputFocused] = React.useState(false);
  const mobileTextInputRef = React.useRef<TextInput | null>(null);

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

  const handleExitTab = React.useCallback(async () => {
    try {
      await exitFullscreen();
    } catch (error) {
      console.error('[ActrInputMobileView] Failed to add bookmark', error);
    }
  }, [appId, tab.id, onAddBookmark]);

  React.useEffect(() => {
    if (inputText.length > 0 && mobileInputMode !== 'text') {
      setMobileInputMode('text');
    }
  }, [inputText.length, mobileInputMode]);

  // 手势实例固定化：录音期间 metering/duration 高频更新时也不重建 responder，
  // 只通过 ref 注入最新回调，避免 release/terminate 丢失。
  const voiceCallbacksRef = React.useRef({
    handleVoiceStart,
    handleVoiceChangeState,
    handleVoiceEnd,
    onFocusTextInput,
  });
  React.useEffect(() => {
    voiceCallbacksRef.current = {
      handleVoiceStart,
      handleVoiceChangeState,
      handleVoiceEnd,
      onFocusTextInput,
    };
  }, [handleVoiceChangeState, handleVoiceEnd, handleVoiceStart, onFocusTextInput]);

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

  return (
    <SafeBottomView style={styles.mobileSafeArea} minBottom={10}>
      <View style={[styles.mobileContainer, { paddingHorizontal: 26 }]}>
        <View style={styles.mobileInputRow}>
          {/* 左侧取消编辑按钮 - 编辑状态时显示 */}
          {isEditing && (
            <TouchableOpacity
              style={styles.mobileReservedButton}
              onPress={handleCancelEdit}
            >
              <Icon type='close' size={16} color='#666' />
            </TouchableOpacity>
          )}

          <View style={styles.mobilePrimaryArea}>
            {!isVoiceSurfaceMode ? (
              <View style={[
                styles.mobileInputPill,
                hasAttachments && styles.mobileInputPillWithAttachment,
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
                <View style={styles.mobileInputInnerRow}>
                  <DropdownMenu
                    verticalGap={24}
                    trigger={
                      <View style={styles.mobilePlusButton}>
                        <Icon type='link' size={20} color='#404040' />
                      </View>
                    }
                  >
                    {(close) => (
                      <View>
                        {showSearchEntry && (
                          <TouchableOpacity
                            style={[
                              styles.mobileMenuItem,
                              isWebSearchActive && styles.mobileMenuItemActive,
                            ]}
                            onPress={() => {
                              setIsWebSearchActive(!isWebSearchActive);
                              close();
                            }}
                          >
                            <Icon
                              type='network'
                              active={isWebSearchActive}
                              size={20}
                              color={isWebSearchActive ? '#34A8FB' : '#2C2C2C'}
                            />
                            <View style={styles.mobileMenuTextBlock}>
                              <Text style={styles.mobileMenuTitle}>{t('actr.input.webSearch')}</Text>
                              <Text style={styles.mobileMenuSubtitle}>
                                {t('actr.input.webSearchTip')}
                              </Text>
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
                              setIsLocationActive(!isLocationActive);
                              close();
                            }}
                          >
                            <Icon
                              type='share_position'
                              active={isLocationActive}
                              size={20}
                              color={isLocationActive ? '#34A8FB' : '#2C2C2C'}
                            />
                            <View style={styles.mobileMenuTextBlock}>
                              <Text style={styles.mobileMenuTitle}>{t('actr.input.shareLocation')}</Text>
                              <Text style={styles.mobileMenuSubtitle}>
                                {t('actr.input.shareLocationTip')}
                              </Text>
                            </View>
                          </TouchableOpacity>
                        )}

                        <TouchableOpacity
                          style={styles.mobileMenuItem}
                          onPress={() => {
                            setShowMapModal(true);
                            close();
                          }}
                        >
                          <Icon type='geolocation' size={22} color='#2C2C2C' />
                          <View style={styles.mobileMenuTextBlock}>
                            <Text style={styles.mobileMenuTitle}>{t('actr.input.location')}</Text>
                            <Text style={styles.mobileMenuSubtitle}>{t('actr.input.sendLocation')}</Text>
                          </View>
                        </TouchableOpacity>

                        <View style={styles.mobileMenuDivider} />

                        {showNewlineEntry && (
                          <TouchableOpacity
                            style={[
                              styles.mobileMenuItem,
                              isSendOnEnter && styles.mobileMenuItemActive,
                            ]}
                            onPress={() => {
                              setIsSendOnEnter(!isSendOnEnter);
                              close();
                            }}
                          >
                            <Icon
                              type='enter'
                              active={isSendOnEnter}
                              size={18}
                              color={isSendOnEnter ? '#34A8FB' : '#2C2C2C'}
                            />
                             <View style={styles.mobileMenuTextBlock}>
                              <Text style={styles.mobileMenuTitle}>{t('actr.input.enterToNewLine')}</Text>
                              <Text style={styles.mobileMenuSubtitle}>
                                {t('actr.input.enterToNewLineTip')}
                              </Text>
                            </View>
                          </TouchableOpacity>
                        )}

                        <View style={styles.mobileMenuDivider} />

                        <View style={styles.mobileQuickActions}>
                          <TouchableOpacity
                            style={styles.mobileQuickAction}
                            onPress={() => {
                              takePhoto();
                              close();
                            }}
                          >
                             <Icon type='camera' size={20} />
                            <Text style={styles.mobileQuickActionText}>{t('actr.input.camera')}</Text>
                          </TouchableOpacity>
                          <TouchableOpacity
                            style={styles.mobileQuickAction}
                            onPress={() => {
                              pickImage();
                              close();
                            }}
                          >
                             <Icon type='image' size={20} />
                            <Text style={styles.mobileQuickActionText}>{t('actr.input.photos')}</Text>
                          </TouchableOpacity>
                          <TouchableOpacity
                            style={styles.mobileQuickAction}
                            onPress={() => {
                              pickDocument();
                              close();
                            }}
                          >
                             <Icon type='file' size={20} />
                            <Text style={styles.mobileQuickActionText}>{t('actr.input.files')}</Text>
                          </TouchableOpacity>
                        </View>
                      </View>
                    )}
                  </DropdownMenu>

                  <View style={styles.mobileTextInputWrap}>
                    <StableTextInput
                      appId={appId}
                      hasAttachments={attachments.length > 0}
                      isSendOnEnter={isSendOnEnter}
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
                      <Icon type='input_clear' size={22} color='#666' />
                    </TouchableOpacity>
                  ) : shouldShowVoiceSwitchButton ? (
                    <TouchableOpacity
                      style={styles.mobileMicButton}
                      onPress={onSwitchToVoiceHoldMode}
                    >
                      <Icon type='audio' size={24} color='#666' />
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
                     <Text style={styles.mobileVoiceHoldText}>
                      {t('actr.input.holdToSpeak')}
                    </Text>
                  </View>

                  <TouchableOpacity
                    style={styles.mobileVoiceModeKeyboardButton}
                    onPress={onSwitchToTextMode}
                  >
                    <Icon
                      type='keyboard'
                      size={24}
                      color='#666666'
                    />
                  </TouchableOpacity>
                </View>
              </View>
            )}

          </View>

          {/* 右侧按钮 - 非编辑状态时显示 */}
          {!isEditing && (
            <>
              {/* 收藏按钮 - 未聚焦时显示 */}
              {!isInputFocused && (
                <TouchableOpacity
                  style={styles.mobileReservedButton}
                  onPress={handleExitTab}
                >
                  <Icon type='tab_preview' size={20} color='#404040' />
                  {/* 未读消息角标 */}
                  {totalUnread > 0 && (
                    <View style={styles.unreadBadge}>
                      <Text style={styles.unreadBadgeText}>
                        {totalUnread > 99 ? '99+' : totalUnread}
                      </Text>
                    </View>
                  )}
                </TouchableOpacity>
              )}

              {/* send 按钮 - 聚焦 + isSendOnEnter 时显示 */}
              {isInputFocused && isSendOnEnter && (
                <TouchableOpacity
                  style={styles.mobileReservedButton}
                  onPress={() => {
                    if (hasInputText) {
                      handleSendRef.current();
                    }
                  }}
                  disabled={!hasInputText}
                >
                  <Icon
                    type='send'
                    size={20}
                    color={hasInputText ? '#000000' : '#666'}
                  />
                </TouchableOpacity>
              )}
            </>
          )}
        </View>
      </View>
    </SafeBottomView>
  );
}

const styles = StyleSheet.create({
  mobileContainer: {
    width: '100%',
    paddingTop: 6,
    // paddingBottom: 10,
    backgroundColor: '#F2F2F6',
  },
  mobileSafeArea: {
    width: '100%',
    backgroundColor: '#F2F2F6',
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
    minHeight: 48,
    position: 'relative',
  },
  mobileInputPill: {
    minHeight: 48,
    borderRadius: 24,
    backgroundColor: '#FFFFFF',
    ...HOST_SHADOWS.inputPill,
  },
  mobileInputPillWithAttachment: {
    borderRadius: 20,
  },
  mobileAttachmentArea: {
    paddingTop: 12,
    paddingHorizontal: 12,
  },
  mobileAttachmentDivider: {
    height: 1,
    backgroundColor: '#EAEAEA',
    marginHorizontal: 12,
    marginTop: 12,
  },
  mobileInputInnerRow: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    paddingVertical: 8,
    paddingHorizontal: 8,
  },
  mobileVoiceSurface: {
    minHeight: 48,
    borderRadius: 24,
    backgroundColor: '#FFFFFF',
    position: 'relative',
    ...HOST_SHADOWS.inputPill,
  },
  mobileVoiceSurfaceWithAttachment: {
    borderRadius: 20,
  },
  mobileVoiceInnerRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 8,
    paddingHorizontal: 8,
  },
  mobilePlusButton: {
    width: 34,
    height: 34,
    borderRadius: 17,
    alignItems: 'center',
    justifyContent: 'center',
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
    width: 34,
    height: 34,
    borderRadius: 17,
    alignItems: 'center',
    justifyContent: 'center',
  },
  mobileReservedButton: {
    width: 48,
    height: 48,
    borderRadius: 24,
    borderWidth: 1,
    borderColor: '#ECECEF',
    backgroundColor: '#FFFFFF',
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.inputPill,
  },
  mobileVoiceModeSpacer: {
    width: 34,
    height: 34,
  },
  mobileVoiceModeMain: {
    flex: 1,
    height: 34,
    alignItems: 'center',
    justifyContent: 'center',
  },
  mobileVoiceModeKeyboardButton: {
    width: 34,
    height: 34,
    borderRadius: 17,
    alignItems: 'center',
    justifyContent: 'center',
  },
  mobileVoiceHoldText: {
    color: '#333333',
    fontSize: 17,
    fontWeight: '500',
  },
  mobileMenuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 14,
    paddingHorizontal: 10,
    paddingVertical: 10,
    marginVertical: 3,
    borderRadius: 20,
  },
  mobileMenuItemActive: {
    backgroundColor: '#EDEDED',
  },
  mobileMenuTextBlock: {
    flex: 1,
  },
  mobileMenuTitle: {
    fontSize: 17,
    color: '#333333',
  },
  mobileMenuSubtitle: {
    fontSize: 13,
    color: '#999999',
    marginTop: 2,
  },
  mobileMenuDivider: {
    height: 1,
    backgroundColor: '#E0E0E0',
    marginVertical: 8,
    marginHorizontal: 10,
  },
  mobileQuickActions: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingTop: 8,
    paddingBottom: 4,
    alignItems: 'flex-start',
  },
  mobileQuickAction: {
    width: 60,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 4,
  },
  mobileQuickActionText: {
    fontSize: 14,
    color: '#2C2C2C',
    fontWeight: '500',
  },
  unreadBadge: {
    position: 'absolute',
    top: -2,
    right: -2,
    backgroundColor: '#FF3B30',
    borderRadius: 10,
    minWidth: 20,
    height: 20,
    paddingHorizontal: 5,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 2,
    borderColor: '#F2F2F6',
  },
  unreadBadgeText: {
    color: '#FFFFFF',
    fontSize: 11,
    fontWeight: '600',
    lineHeight: 14,
  },
});
