import React from 'react';
import { View, StyleSheet, useWindowDimensions } from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';

interface VoiceRecordingOverlayProps {
  isCancelling: boolean;
  metering?: number;
}

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

  // 50 rectangles logic
  const rectCount = isPhone ? 36 : 50;
  const rectData = Array.from({ length: rectCount });

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
            
            // Weight Factor: 1 at center, 0 at edges
            const weight = Math.max(0, 1 - distanceFromCenter / maxDistance);
            
            // Opacity logic: 1 at center, 0.5 at edges (for normal state)
            const opacity = isCancelling 
              ? 1 
              : 1 - (distanceFromCenter / maxDistance) * 0.5;
            
            const rectBg = rectBgColor;

            // Calculate Dynamic Height (Scheme A)
            // minHeight=6, maxDelta=30 -> height range [6, 36] based on volume & weight
            const dynamicHeight = 6 + (metering * 40 * weight);

            return (
              <View
                key={index}
                style={[
                  styles.rect,
                  {
                    backgroundColor: rectBg,
                    opacity: opacity,
                    marginHorizontal: 1,
                    height: dynamicHeight,
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
