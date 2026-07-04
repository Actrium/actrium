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
import { Icon } from '../../../components';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import {
  DropdownMenu,
  DropdownMenuItem,
  DropdownMenuSubText,
  DropdownMenuText,
} from '../shared/DropdownMenu';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { triggerHaptic } from '../../utils/haptics';

interface ActrInputPadToolboxProps {
  onPickCamera: () => void;
  onPickImage: () => void;
  onPickFile: () => void;
  onPickLocation: () => void;
  isWebSearchActive: boolean;
  setIsWebSearchActive: (active: boolean) => void;
  isEnterForNewLine: boolean;
  setIsEnterForNewLine: (active: boolean) => void;
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
  isEnterForNewLine,
  setIsEnterForNewLine,
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
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const { width, height } = useWindowDimensions();
  const isPhone = width < 768;
  const isCompact = width < 390;
  const showInlineLocationButton = !isPhone;
  const cancelThreshold = Math.max(40, Math.min(80, height * 0.08));
  const recoverThreshold = Math.round(cancelThreshold * 0.35);

  const [internalIsCancelling, setInternalIsCancelling] = React.useState(false);
  const isCancellingRef = React.useRef(false);
  const lastVoiceStartRef = React.useRef(0);

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
          const now = Date.now();
          if (now - lastVoiceStartRef.current < 1000) return;
          lastVoiceStartRef.current = now;
          isCancellingRef.current = false;
          setInternalIsCancelling(false);
          triggerHaptic('impactHeavy')
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
            if (nextIsCancelling) triggerHaptic('impactHeavy');
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
          trigger={<Icon type="link_pad" disableTint size={34} />}
        >
          {(close) => (
            <>
              <DropdownMenuItem
                onPress={() => {
                  close(onPickCamera);
                }}
              >
                <View style={{ width: 28, alignItems: 'center' }}>
                  <Icon type="camera" size={20} color={colors.textIconPrimary} />
                </View>
                <DropdownMenuText>{t('actr.input.camera')}</DropdownMenuText>
              </DropdownMenuItem>
              <DropdownMenuItem
                onPress={() => {
                  close(onPickImage);
                }}
              >
                <View style={{ width: 28, alignItems: 'center' }}>
                  <Icon type="image" size={20} color={colors.textIconPrimary} />
                </View>
                <DropdownMenuText>{t('actr.input.photos')}</DropdownMenuText>
              </DropdownMenuItem>
              <DropdownMenuItem
                onPress={() => {
                  close(onPickFile);
                }}
              >
                <View style={{ width: 28, alignItems: 'center' }}>
                  <Icon type="file" size={20} color={colors.textIconPrimary} />
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
              color={isWebSearchActive ? colors.accentBlue : undefined}
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
                <Icon type='more' />
              </View>
            }
          >
            {(close) => (
              <>
                {!showInlineLocationButton && (
                  <DropdownMenuItem
                    onPress={() => {
                      close(onPickLocation);
                    }}
                  >
                    <View style={{ width: 18, alignItems: 'center' }}>
                      <Icon
                        type="geolocation"
                        size={18}
                        color={colors.textIconPrimary}
                        />
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
                        color={isLocationActive ? colors.accentBlue : colors.textIconPrimary}
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
                    isActive={isEnterForNewLine}
                    onPress={() => {
                      setIsEnterForNewLine(!isEnterForNewLine);
                      close();
                    }}
                  >
                    <View style={{ width: 18, alignItems: 'center' }}>
                      <Icon
                        type="enter"
                        size={16}
                        color={isEnterForNewLine ? colors.accentBlue : colors.textIconPrimary}
                        />
                      </View>
                    <View style={styles.moreMenuTextBlock}>
                      <DropdownMenuText>{t('actr.input.enterToNewLine')}</DropdownMenuText>
                      <DropdownMenuSubText>{t('actr.input.enterToNewLineTip')}</DropdownMenuSubText>
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
          <View {...panResponder.panHandlers}>
            <Icon type="audio_pad" disableTint size={34} />
          </View>
        )}
        <TouchableOpacity style={styles.sendButton} onPress={onSend}>
          <Icon type="send" size={16} color={colors.textInverse} />
        </TouchableOpacity>
      </View>
    </View>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
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
    backgroundColor: colors.bgActionButton,
    justifyContent: 'center',
    alignItems: 'center',
  },
  actionPill: {
    flexDirection: 'row',
    alignItems: 'center',
    height: 36,
    paddingHorizontal: 12,
    borderRadius: 34,
    backgroundColor: colors.bgActionButton,
    gap: 6,
  },
  actionPillPhone: {
    paddingHorizontal: 10,
  },
  actionPillActive: {
    backgroundColor: colors.bgActionButtonActive,
  },
  moreMenuTextBlock: {
    alignSelf: 'flex-start',
  },
  actionPillText: {
    ...getTypographyStyle('body'),
    color: colors.textIconPrimary,
    fontWeight: '400',
  },
  sendButton: {
    width: 34,
    height: 34,
    borderRadius: 17,
    backgroundColor: colors.accentBlue,
    justifyContent: 'center',
    alignItems: 'center',
  },
});
