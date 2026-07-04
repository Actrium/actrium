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

// Pad 端最多展示 50 根柱子，所有动画值按最大数量预创建以避免旋转时重建 Hook。
const MAX_WAVEFORM_RECT_COUNT = 50;

// Phone 端减少柱子数量，保证窄屏下柱子间距和可读性。
const PHONE_WAVEFORM_RECT_COUNT = 36;

// 波形柱子的基础高度，静音时保持可见底线。
const MIN_WAVEFORM_RECT_HEIGHT = 6;

// 波形柱子的最大增量高度，控制浮层中音波的视觉幅度。
const MAX_WAVEFORM_RECT_DELTA = 42;

// 音量上升时用较高系数，让波形对开口说话更及时。
const WAVEFORM_ATTACK_ALPHA = 0.48;

// 音量回落时保持一定速度，避免说完话后被平滑值拖住太久。
const WAVEFORM_RELEASE_ALPHA = 0.34;

// 没有收到新音量事件超过该时间后，主动把波形往静音底线收回。
const WAVEFORM_IDLE_DECAY_DELAY_MS = 180;

// 空闲衰减的检查间隔，避免原生静音时不再推送 volumechange 导致高度悬停。
const WAVEFORM_IDLE_DECAY_INTERVAL_MS = 80;

// 空闲衰减用更高系数，保证说完话后能明显回落。
const WAVEFORM_IDLE_DECAY_ALPHA = 0.55;

// 中心权重指数越大，两侧越陡，波形更集中在中间区域。
const WAVEFORM_CENTER_WEIGHT_POWER = 1.9;

// 非静音时给柱子一个很小的起跳量，避免轻声说话完全看不见。
const WAVEFORM_VISUAL_START_OFFSET = 0.05;

// 弹性动画阻尼，控制每根柱子追随目标高度时的回弹幅度。
const WAVEFORM_SPRING_DAMPING = 14;

// 弹性动画刚度，控制每根柱子追随目标高度的速度。
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
  // 每根柱子独立持有高度动画值，让音量变化时不是整组同步硬跳。
  const animatedHeights = React.useRef(
    Array.from({ length: MAX_WAVEFORM_RECT_COUNT }, () =>
      new Animated.Value(MIN_WAVEFORM_RECT_HEIGHT),
    ),
  ).current;
  // 保存上一帧平滑后的音量，用非对称低通滤波处理音量抖动。
  const smoothedMeteringRef = React.useRef(0);
  // 记录最近一次真实音量更新时间，用于判断是否需要主动空闲回落。
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
    // 接近静音时直接吸附到底线，避免小数残留让柱子长时间不归位。
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
