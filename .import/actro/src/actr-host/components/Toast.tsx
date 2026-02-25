import React, {
  useEffect,
  useState,
  forwardRef,
  useImperativeHandle,
} from 'react';
import { View, Text, StyleSheet, Animated } from 'react-native';

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
        <Text style={styles.text}>{message}</Text>
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
    paddingHorizontal: 24,
    paddingVertical: 12,
    borderRadius: 24,
    // Shadow for white background
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.1,
    shadowRadius: 8,
    elevation: 4,
  },
  text: {
    color: '#333333',
    fontSize: 15,
    fontWeight: '600',
  },
});
