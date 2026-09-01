import React from 'react';
import { View, StyleSheet, Animated, Easing } from 'react-native';
import { useColors } from '../../../theme/useColors';

interface WaveFormProps {
  color?: string;
  size?: { width: number; height: number };
  barCount?: number;
}

export const WaveForm: React.FC<WaveFormProps> = ({
  color,
  size = { width: 24, height: 18 },
  barCount = 5,
}) => {
  const colors = useColors();
  const resolvedColor = color ?? colors.accentBlue;
  // Generate Initial Height
  const initialValues = [0.4, 0.7, 0.5, 0.8, 0.4];
  const animations = React.useRef(
    Array.from({ length: barCount }).map((_, i) => new Animated.Value(initialValues[i % initialValues.length])),
  ).current;

  React.useEffect(() => {
    const createAnimation = (anim: Animated.Value, delay: number) => {
      return Animated.loop(
        Animated.sequence([
          Animated.delay(delay),
          Animated.timing(anim, {
            toValue: 0.2 + Math.random() * 0.1,
            duration: 400 + Math.random() * 200,
            useNativeDriver: false,
            easing: Easing.inOut(Easing.ease),
          }),
          Animated.timing(anim, {
            toValue: 0.7 + Math.random() * 0.3,
            duration: 400 + Math.random() * 200,
            useNativeDriver: false,
            easing: Easing.inOut(Easing.ease),
          }),
        ]),
      );
    };

    const loops = animations.map((anim, i) =>
      createAnimation(anim, i * 100),
    );
    loops.forEach((l) => l.start());

    return () => loops.forEach((l) => l.stop());
  }, [animations]);

  return (
    <View style={[styles.waveContainer, { width: size.width, height: size.height }]}>
      {animations.map((anim, i) => (
        <Animated.View
          key={i}
          style={[
            styles.waveBar,
            {
              backgroundColor: resolvedColor,
              height: anim.interpolate({
                inputRange: [0, 1],
                outputRange: ['15%', '85%'],
              }),
            },
          ]}
        />
      ))}
    </View>
  );
};

const styles = StyleSheet.create({
  waveContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 2,
  },
  waveBar: {
    width: 2,
    borderRadius: 1,
  },
});
