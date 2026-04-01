import React from 'react';
import { StyleSheet, Pressable, Animated } from 'react-native';
import { AppText } from '../../components';
import {
  getTypographyStyle,
  scrollToBottomButton,
} from '../../theme/tokens';
import { HOST_SHADOWS } from '../styles/shadows';

interface ScrollToBottomButtonProps {
  visible: boolean;
  onPress: () => void;
}

export const ScrollToBottomButton: React.FC<ScrollToBottomButtonProps> = ({
  visible,
  onPress,
}) => {
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

const styles = StyleSheet.create({
  container: {
    alignSelf: 'center',
  },
  button: {
    width: scrollToBottomButton.size,
    height: scrollToBottomButton.size,
    borderRadius: scrollToBottomButton.size / 2,
    backgroundColor: '#FFFFFF',
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.floatingButton,
  },
  buttonPressed: {
    opacity: 0.7,
    transform: [{ scale: 0.95 }],
  },
  arrow: {
    ...getTypographyStyle(scrollToBottomButton.labelTypographyVariant),
    color: '#666',
    fontWeight: '600',
  },
});
