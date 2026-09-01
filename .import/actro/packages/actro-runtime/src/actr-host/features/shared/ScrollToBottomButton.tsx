import React from 'react';
import { StyleSheet, Pressable, Animated } from 'react-native';
import { AppText } from '../../../components';
import {
  getTypographyStyle,
  scrollToBottomButton,
} from '../../../theme/tokens';
import { getHostShadows } from '../../styles/shadows';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

interface ScrollToBottomButtonProps {
  visible: boolean;
  onPress: () => void;
}

export const ScrollToBottomButton: React.FC<ScrollToBottomButtonProps> = ({
  visible,
  onPress,
}) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const opacity = React.useRef(new Animated.Value(0)).current;

  React.useEffect(() => {
    Animated.timing(opacity, {
      toValue: visible ? 1 : 0,
      duration: 200,
      useNativeDriver: true,
    }).start();
  }, [visible, opacity]);

  if (!visible) return null;

  return (
    <Animated.View style={[styles.container, { opacity }]}>
      <Pressable
        onPress={onPress}
        style={({ pressed }) => [
          styles.button,
          pressed && styles.buttonPressed,
        ]}
      >
        <AppText style={styles.arrow}>↓</AppText>
      </Pressable>
    </Animated.View>
  );
};

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
  container: {
    alignSelf: 'center',
  },
  button: {
    width: scrollToBottomButton.size,
    height: scrollToBottomButton.size,
    borderRadius: scrollToBottomButton.size / 2,
    backgroundColor: colors.bgToBottom,
    alignItems: 'center',
    justifyContent: 'center',
    ...hostShadows.floatingButton,
  },
  buttonPressed: {
    opacity: 0.7,
    transform: [{ scale: 0.95 }],
  },
  arrow: {
    ...getTypographyStyle(scrollToBottomButton.labelTypographyVariant),
    color: colors.textSecondary,
    fontWeight: '600',
  },
});
};
