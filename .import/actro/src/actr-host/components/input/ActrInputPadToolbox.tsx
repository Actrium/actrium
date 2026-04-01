import React from 'react';
import {
  View,
  TouchableOpacity,
  StyleSheet,
  PanResponder,
  GestureResponderEvent,
  PanResponderGestureState,
  useWindowDimensions,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from '../Icon';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import {
  DropdownMenu,
  DropdownMenuItem,
  DropdownMenuSubText,
  DropdownMenuText,
} from '../DropdownMenu';

interface ActrInputPadToolboxProps {
  onPickCamera: () => void;
  onPickImage: () => void;
  onPickFile: () => void;
  onPickLocation: () => void;
  isWebSearchActive: boolean;
  setIsWebSearchActive: (active: boolean) => void;
  isSendOnEnter: boolean;
  setIsSendOnEnter: (active: boolean) => void;
  isLocationActive: boolean;
  setIsLocationActive: (active: boolean) => void;
  onSend: () => void;
  onAudio: () => void;
  showSearchEntry: boolean;
  showLocationEntry: boolean;
  showNewlineEntry: boolean;
  showMoreOptions: boolean;
  showAudioButton?: boolean;
  onVoiceStart?: () => void;
  onVoiceChangeState?: (isCancelling: boolean) => void;
  onVoiceEnd?: (isCancelled: boolean) => void;
}

export const ActrInputPadToolbox: React.FC<ActrInputPadToolboxProps> = ({
  onPickCamera,
  onPickImage,
  onPickFile,
  onPickLocation,
  isWebSearchActive,
  setIsWebSearchActive,
  isSendOnEnter,
  setIsSendOnEnter,
  isLocationActive,
  setIsLocationActive,
  onSend,
  onAudio,
  showSearchEntry,
  showLocationEntry,
  showNewlineEntry,
  showMoreOptions,
  showAudioButton = true,
  onVoiceStart,
  onVoiceChangeState,
  onVoiceEnd,
}) => {
  const { t } = useTranslation();
  const { width, height } = useWindowDimensions();
  const isPhone = width < 768;
  const isCompact = width < 390;
  const showInlineLocationButton = !isPhone;
  const cancelThreshold = Math.max(40, Math.min(80, height * 0.08));
  const recoverThreshold = Math.round(cancelThreshold * 0.35);

  const [internalIsCancelling, setInternalIsCancelling] = React.useState(false);
  const isCancellingRef = React.useRef(false);

  // 使用 Ref 追踪最新的回调函数，确保 PanResponder 实例永远不需要重建
  const callbacks = React.useRef({
    onVoiceStart,
    onVoiceChangeState,
    onVoiceEnd,
  });

  React.useEffect(() => {
    callbacks.current = { onVoiceStart, onVoiceChangeState, onVoiceEnd };
  }, [onVoiceStart, onVoiceChangeState, onVoiceEnd]);

  const panResponder = React.useMemo(
    () =>
      PanResponder.create({
        onStartShouldSetPanResponder: () => true,
        onStartShouldSetPanResponderCapture: () => true,
        onMoveShouldSetPanResponder: () => true,
        onMoveShouldSetPanResponderCapture: () => true,
        onPanResponderTerminationRequest: () => false,
        onPanResponderGrant: () => {
          isCancellingRef.current = false;
          setInternalIsCancelling(false);
          callbacks.current.onVoiceStart?.();
        },
        onPanResponderMove: (_: GestureResponderEvent, gestureState: PanResponderGestureState) => {
          let nextIsCancelling = isCancellingRef.current;
          
          // 滞后策略：阈值随屏幕高度变化，手机端避免手势行程过长
          if (!isCancellingRef.current && gestureState.dy < -cancelThreshold) {
            nextIsCancelling = true;
          } else if (isCancellingRef.current && gestureState.dy > -recoverThreshold) {
            nextIsCancelling = false;
          }

          if (nextIsCancelling !== isCancellingRef.current) {
            isCancellingRef.current = nextIsCancelling;
            setInternalIsCancelling(nextIsCancelling);
            callbacks.current.onVoiceChangeState?.(nextIsCancelling);
          }
        },
        onPanResponderRelease: () => {
          callbacks.current.onVoiceEnd?.(isCancellingRef.current);
          setInternalIsCancelling(false);
          isCancellingRef.current = false;
        },
        onPanResponderTerminate: () => {
          callbacks.current.onVoiceEnd?.(true);
          setInternalIsCancelling(false);
          isCancellingRef.current = false;
        },
      }),
    [cancelThreshold, recoverThreshold]
  );

  return (
    <View style={styles.actionRow}>
      <View style={[styles.leftActions, isPhone && styles.leftActionsPhone]}>
        <DropdownMenu
          estimatedWidth={228}
          minWidth={228}
          anchorMode='start'
          trigger={
            <View style={styles.actionCircle}>
              <Icon type="link" />
            </View>
          }
        >
          {(close) => (
            <>
              <DropdownMenuItem
                onPress={() => {
                  onPickCamera();
                  close();
                }}
              >
                <View style={{ width: 28, alignItems: 'center' }}>
                  <Icon type="camera" size={20} color="#333333" />
                </View>
                <DropdownMenuText>{t('actr.input.camera')}</DropdownMenuText>
              </DropdownMenuItem>
              <DropdownMenuItem
                onPress={() => {
                  onPickImage();
                  close();
                }}
              >
                <View style={{ width: 28, alignItems: 'center' }}>
                  <Icon type="image" size={20} color="#333333" />
                </View>
                <DropdownMenuText>{t('actr.input.photos')}</DropdownMenuText>
              </DropdownMenuItem>
              <DropdownMenuItem
                onPress={() => {
                  onPickFile();
                  close();
                }}
              >
                <View style={{ width: 28, alignItems: 'center' }}>
                  <Icon type="file" size={20} color="#333333" />
                </View>
                <DropdownMenuText>{t('actr.input.files')}</DropdownMenuText>
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenu>

        {showSearchEntry && (
          <TouchableOpacity
            onPress={() => setIsWebSearchActive(!isWebSearchActive)}
            style={[
              styles.actionPill,
              isWebSearchActive && styles.actionPillActive,
              isPhone && styles.actionPillPhone,
            ]}
          >
            <Icon
              type='network'
              color={isWebSearchActive ? '#34A8FB' : undefined}
            />
            {!isCompact && <AppText style={styles.actionPillText}>{t('actr.input.webSearch')}</AppText>}
          </TouchableOpacity>
        )}

        {showInlineLocationButton && (
          <TouchableOpacity onPress={onPickLocation} style={styles.actionPill}>
            <Icon type='geolocation' />
            <AppText style={styles.actionPillText}>{t('actr.input.location')}</AppText>
          </TouchableOpacity>
        )}

        {(showMoreOptions || !showInlineLocationButton) && (
          <DropdownMenu
            estimatedWidth={228}
            minWidth={228}
            anchorMode='end'
            trigger={
              <View style={styles.actionCircle}>
                <AppText style={styles.actionPillText}>•••</AppText>
              </View>
            }
          >
            {(close) => (
              <>
                {!showInlineLocationButton && (
                  <DropdownMenuItem
                    onPress={() => {
                      onPickLocation();
                      close();
                    }}
                  >
                    <View style={{ width: 18, alignItems: 'center' }}>
                      <Icon type="geolocation" size={18} color="#333333" />
                    </View>
                    <View style={styles.moreMenuTextBlock}>
                      <DropdownMenuText>{t('actr.input.location')}</DropdownMenuText>
                      <DropdownMenuSubText>{t('actr.input.sendLocation')}</DropdownMenuSubText>
                    </View>
                  </DropdownMenuItem>
                )}
                {showLocationEntry && (
                  <DropdownMenuItem
                    isActive={isLocationActive}
                    onPress={() => {
                      setIsLocationActive(!isLocationActive);
                      close();
                    }}
                  >
                    <View style={{ width: 18, alignItems: 'center' }}>
                      <Icon
                        type="geolocation"
                        size={18}
                        color={isLocationActive ? '#34A8FB' : '#333333'}
                        />
                      </View>
                    <View style={styles.moreMenuTextBlock}>
                      <DropdownMenuText>{t('actr.input.shareLocation')}</DropdownMenuText>
                      <DropdownMenuSubText>{t('actr.input.shareLocationTip')}</DropdownMenuSubText>
                    </View>
                  </DropdownMenuItem>
                )}
                {showNewlineEntry && (
                  <DropdownMenuItem
                    isActive={isSendOnEnter}
                    onPress={() => {
                      setIsSendOnEnter(!isSendOnEnter);
                      close();
                    }}
                  >
                    <View style={{ width: 18, alignItems: 'center' }}>
                      <Icon
                        type="enter"
                        size={16}
                        color={isSendOnEnter ? '#34A8FB' : '#333333'}
                        />
                      </View>
                    <View style={styles.moreMenuTextBlock}>
                      <DropdownMenuText>{t('actr.input.sendKeySettings')}</DropdownMenuText>
                      <DropdownMenuSubText>{t('actr.input.enterToSendMessage')}</DropdownMenuSubText>
                    </View>
                  </DropdownMenuItem>
                )}
              </>
            )}
          </DropdownMenu>
        )}
      </View>

      <View style={styles.rightActions}>
        {showAudioButton && (
          <View
            {...panResponder.panHandlers}
            style={styles.audioSwitch}
          >
            <Icon type="audio" style={{ height: 22 }} />
          </View>
        )}
        <TouchableOpacity style={styles.sendButton} onPress={onSend}>
          <Icon type="send" size={16} color="#fff" />
        </TouchableOpacity>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  actionRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    zIndex: 1,
  },
  leftActions: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
    flex: 1,
    minWidth: 0,
  },
  leftActionsPhone: {
    gap: 6,
  },
  rightActions: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
    marginLeft: spacing.sm,
  },
  actionCircle: {
    height: 34,
    paddingHorizontal: 14,
    borderRadius: 34,
    backgroundColor: '#7676801F',
    justifyContent: 'center',
    alignItems: 'center',
  },
  actionPill: {
    flexDirection: 'row',
    alignItems: 'center',
    height: 36,
    paddingHorizontal: 12,
    borderRadius: 34,
    backgroundColor: '#7676801F',
    gap: 6,
  },
  actionPillPhone: {
    paddingHorizontal: 10,
  },
  actionPillActive: {
    backgroundColor: '#D1D1D6',
  },
  moreMenuTextBlock: {
    alignSelf: 'flex-start',
  },
  actionPillText: {
    ...getTypographyStyle('body'),
    color: '#333333',
    fontWeight: '400',
  },
  audioSwitch: {
    width: 34,
    height: 34,
    borderRadius: 17,
    backgroundColor: '#7676801F',
    justifyContent: 'center',
    alignItems: 'center',
    marginRight: 8,
  },
  sendButton: {
    width: 34,
    height: 34,
    borderRadius: 17,
    backgroundColor: '#34A8FB',
    justifyContent: 'center',
    alignItems: 'center',
  },
});
