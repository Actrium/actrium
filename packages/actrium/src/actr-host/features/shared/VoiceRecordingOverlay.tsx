import React from 'react';
import {
  Animated,
  StyleSheet,
  useWindowDimensions,
  View,
} from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';

interface VoiceRecordingOverlayProps {
  isCancelling: boolean;
  metering?: number;
}

// Pad Top-Show 50 Root columns, all animation values are precreated in maximum number to avoid reconstruction when rotate Hook.
const MAX_WAVEFORM_RECT_COUNT = 50;

// Phone The end reduces the number of columns to ensure the space and readability of the columns under the narrow screen.
const PHONE_WAVEFORM_RECT_COUNT = 36;

// The base height of the wave-shaped column is maintained as the visible bottom line is maintained when the sound is silent.
const MIN_WAVEFORM_RECT_HEIGHT = 6;

// The maximum increment height of the wave column controls the visual range of the sound wave in the float.
const MAX_WAVEFORM_RECT_DELTA = 42;

// High coefficients are used for volume increases to make wave-to-mouth openings more timely.
const WAVEFORM_ATTACK_ALPHA = 0.48;

// Keep a certain speed when volume falls and avoid being held back by smooth values too long after you have finished speaking.
const WAVEFORM_RELEASE_ALPHA = 0.34;

// When no new volume events were received beyond that time, the wave shape was voluntarily recovered to the silent bottom line.
const WAVEFORM_IDLE_DECAY_DELAY_MS = 180;

// Check interval for free-time decay to avoid non-singlinging volumechange The government has been able to stop the situation.
const WAVEFORM_IDLE_DECAY_INTERVAL_MS = 80;

// The loss of free time is reduced by a higher coefficient, which ensures a clear fall after the speech.
const WAVEFORM_IDLE_DECAY_ALPHA = 0.55;

// The larger the central weight index, the steeper the two sides, and the waveforms are more concentrated in the middle.
const WAVEFORM_CENTER_WEIGHT_POWER = 1.9;

// A small jump to the column when the non-quiet is heard, so that the whispering is completely invisible.
const WAVEFORM_VISUAL_START_OFFSET = 0.05;

// Flexible animation to block the nymphoma and control the range of the echoes per column following the target height.
const WAVEFORM_SPRING_DAMPING = 14;

// Flexible aerodynamic intensity, and control of the speed of each column following the target height.
const WAVEFORM_SPRING_STIFFNESS = 190;

export const VoiceRecordingOverlay: React.FC<VoiceRecordingOverlayProps> = ({ isCancelling, metering = 0 }) => {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const colors = useColors();
  const isPhone = width < 768;
  const baseColor = isCancelling ? colors.voiceCancelBaseColor : colors.voiceBaseColor;
  const instructionText = isCancelling ? t('actr.voice.releasedToCancel') : t('actr.voice.releasedToSend');
  const transColor = isCancelling ? colors.voiceCancelTransColor : colors.voiceTransColor;
  const textColor = isCancelling ? colors.voiceCancelTextColor : colors.voiceTextColor;
  const rectBgColor = isCancelling ? colors.voiceCancelRectBgColor : colors.voiceRectBgColor;

  const rectCount = isPhone ? PHONE_WAVEFORM_RECT_COUNT : MAX_WAVEFORM_RECT_COUNT;
  const rectData = React.useMemo(() => Array.from({ length: rectCount }), [rectCount]);
  // Each column holds a high-intensity animation value independently, so that the volume changes without a simultaneous hard jump.
  const animatedHeights = React.useRef(
    Array.from({ length: MAX_WAVEFORM_RECT_COUNT }, () =>
      new Animated.Value(MIN_WAVEFORM_RECT_HEIGHT),
    ),
  ).current;
  // Saves the smooth volume of the previous frame, and asymmetric low-link filtering process the volume to shake.
  const smoothedMeteringRef = React.useRef(0);
  // The latest real volume update is recorded to determine whether active free fallback is required.
  const lastMeteringUpdateAtRef = React.useRef(Date.now());

  const animateWaveform = React.useCallback((nextMetering: number, isIdleDecay = false) => {
    const targetMetering = Math.min(1, Math.max(0, nextMetering));
    const previousMetering = smoothedMeteringRef.current;
    const smoothingAlpha =
      isIdleDecay
        ? WAVEFORM_IDLE_DECAY_ALPHA
        : targetMetering > previousMetering
        ? WAVEFORM_ATTACK_ALPHA
        : WAVEFORM_RELEASE_ALPHA;
    let smoothedMetering =
      previousMetering + (targetMetering - previousMetering) * smoothingAlpha;
    // When the sound is close, it is drawn directly to the end line, so that the trace remains do not cause the column to remain unattended for long periods of time.
    if (targetMetering === 0 && smoothedMetering < 0.015) {
      smoothedMetering = 0;
    }
    smoothedMeteringRef.current = smoothedMetering;

    animatedHeights.slice(0, rectCount).forEach((animatedHeight, index) => {
      const progress = index / (rectCount - 1);
      const centerWeight = Math.pow(
        Math.sin(progress * Math.PI),
        WAVEFORM_CENTER_WEIGHT_POWER,
      );
      const deterministicJitter = 0.92 + ((index * 17) % 7) * 0.025;
      const responsiveMetering =
        smoothedMetering <= 0
          ? 0
          : WAVEFORM_VISUAL_START_OFFSET +
            smoothedMetering * (1 - WAVEFORM_VISUAL_START_OFFSET);
      const targetHeight =
        MIN_WAVEFORM_RECT_HEIGHT +
        responsiveMetering *
          MAX_WAVEFORM_RECT_DELTA *
          centerWeight *
          deterministicJitter;

      Animated.spring(animatedHeight, {
        toValue: targetHeight,
        damping: WAVEFORM_SPRING_DAMPING,
        stiffness: WAVEFORM_SPRING_STIFFNESS,
        mass: 0.75,
        useNativeDriver: false,
      }).start();
    });
  }, [animatedHeights, rectCount]);

  React.useEffect(() => {
    lastMeteringUpdateAtRef.current = Date.now();
    animateWaveform(metering);
  }, [animateWaveform, metering]);

  React.useEffect(() => {
    const interval = setInterval(() => {
      const idleDuration = Date.now() - lastMeteringUpdateAtRef.current;
      if (
        idleDuration < WAVEFORM_IDLE_DECAY_DELAY_MS ||
        smoothedMeteringRef.current <= 0
      ) {
        return;
      }

      animateWaveform(0, true);
    }, WAVEFORM_IDLE_DECAY_INTERVAL_MS);

    return () => clearInterval(interval);
  }, [animateWaveform]);

  return (
    <View style={styles.overlay} pointerEvents="none">
      <LinearGradient
        colors={[transColor, baseColor]}
        style={StyleSheet.absoluteFill}
        start={{ x: 0, y: 0 }}
        end={{ x: 0, y: 0.5 }}
      />
      <View style={[styles.content, { width }]}>
        <View style={styles.textContainer}>
          <AppText style={[styles.text, { color: textColor }]}>{instructionText}</AppText>
        </View>
        <View style={[styles.waveformContainer, { width }]}>
          {rectData.map((_, index) => {
            const maxDistance = (rectCount - 1) / 2;
            const distanceFromCenter = Math.abs(index - maxDistance);

            // Opacity logic: 1 at center, 0.5 at edges (for normal state)
            const opacity = isCancelling 
              ? 1 
              : 1 - (distanceFromCenter / maxDistance) * 0.5;
            
            const rectBg = rectBgColor;

            return (
              <Animated.View
                key={index}
                style={[
                  styles.rect,
                  {
                    backgroundColor: rectBg,
                    opacity: opacity,
                    marginHorizontal: 1,
                    height: animatedHeights[index],
                  }
                ]}
              />
            );
          })}
        </View>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  overlay: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 0,
    height:220,
    justifyContent: 'center',
    alignItems: 'center',
    zIndex: 1000,
  },
  content: {
    alignItems: 'center',
    height: 220,
    justifyContent: 'center',
    marginTop: 80, // Offset to keep it visually balanced when growing from bottom
  },
  textContainer: {
    marginBottom: spacing.xl,
  },
  text: {
    ...getTypographyStyle('body'),
    fontWeight: '500',
  },
  waveformContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    height: 60,
  },
  rect: {
    width: 4,
    height: 6,
    borderRadius: 2,
  },
});
