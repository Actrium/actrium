import React, {
  useEffect,
  useState,
  forwardRef,
  useImperativeHandle,
} from 'react';
import { View, StyleSheet, Animated } from 'react-native';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { HOST_SHADOWS } from '../../styles/shadows';

export interface ToastRef {
  show: (message: string) => void;
}

export const Toast = forwardRef<ToastRef, {}>((_, ref) => {
  const [message, setMessage] = useState('');
  const [visible, setVisible] = useState(false);
  const opacity = useState(new Animated.Value(0))[0];

  useImperativeHandle(ref, () => ({
    show: (msg: string) => {
      setMessage(msg);
      setVisible(true);
      Animated.sequence([
        Animated.timing(opacity, {
          toValue: 1,
          duration: 200,
          useNativeDriver: true,
        }),
        Animated.delay(1500),
        Animated.timing(opacity, {
          toValue: 0,
          duration: 300,
          useNativeDriver: true,
        }),
      ]).start(() => setVisible(false));
    },
  }));

  if (!visible) return null;

  return (
    <Animated.View style={[styles.container, { opacity }]}>
      <View style={styles.bubble}>
        <AppText style={styles.text}>{message}</AppText>
      </View>
    </Animated.View>
  );
});

const styles = StyleSheet.create({
  container: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 60,
    justifyContent: 'center',
    alignItems: 'center',
    zIndex: 9999,
  },
  bubble: {
    backgroundColor: '#FFFFFF',
    paddingHorizontal: spacing.xxl,
    paddingVertical: spacing.md,
    borderRadius: 24,
    ...HOST_SHADOWS.floatingButton,
  },
  text: {
    color: '#333333',
    ...getTypographyStyle('subhead'),
    fontWeight: '600',
  },
});
