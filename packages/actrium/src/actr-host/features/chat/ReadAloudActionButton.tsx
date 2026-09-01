import React from 'react';
import { Pressable, StyleSheet, Animated, Easing } from 'react-native';
import { Icon } from '../../../components';
import { chatBubbleToolbar } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { WaveForm } from '../shared/WaveForm';
import type { VoicePlaybackStatus } from '../../store/useVoicePlaybackStore';
import { SelectionMode } from '../../../../src/actr-host/store/useMessageStore';

interface ReadAloudActionButtonProps {
  status: VoicePlaybackStatus;
  selectionMode: SelectionMode;
  onPress: () => void;
}

export function ReadAloudActionButton({
  status,
  selectionMode,
  onPress,
}: ReadAloudActionButtonProps) {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const spinValue = React.useRef(new Animated.Value(0)).current;

  React.useEffect(() => {
    if (status === 'loading') {
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
  }, [spinValue, status]);

  const spin = spinValue.interpolate({
    inputRange: [0, 1],
    outputRange: ['0deg', '360deg'],
  });

  return (
    <Pressable
      style={[
        styles.toolbarButton,
        status === 'playing' && styles.toolbarButtonPlaying,
        selectionMode !== 'none' && styles.toolbarButtonDisabled,
      ]}
      onPress={onPress}
      disabled={selectionMode !== 'none'}
      hitSlop={8}
    >
      {status === 'loading' && (
        <Animated.View
          style={[styles.loadingBorder, { transform: [{ rotate: spin }] }]}
        />
      )}
      {status === 'playing' ? (
        <WaveForm />
      ) : (
        <Icon type="voice" size={chatBubbleToolbar.iconSize} />
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
