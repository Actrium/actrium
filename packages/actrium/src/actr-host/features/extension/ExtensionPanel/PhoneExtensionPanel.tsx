import React, { useEffect, useState } from 'react';
import { View, StyleSheet } from 'react-native';
import Animated, {
  useAnimatedStyle,
  withTiming,
  Easing,
  runOnJS,
} from 'react-native-reanimated';
import { LAYOUT, CHAT_HEADER_HEIGHT } from '../../../constants';
import type { PhoneExtensionPanelProps } from './types';
import { extensionPanel } from '../../../../theme/tokens';
import { useColors } from '../../../../theme/useColors';
import type { ColorTokens } from '../../../../theme/colors';

/** End-of-the-art animation period */
const ANIM_DURATION = 250;

/**
 * Bones screen component - Place before content loads
 */
const SkeletonPlaceholder: React.FC<{ colors: ColorTokens }> = ({ colors }) => {
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  return (
    <View style={styles.skeletonContainer}>
      <View style={styles.skeletonHeader}>
        <View style={[styles.skeletonBlock, { width: 120, height: 24 }]} />
      </View>
      <View style={styles.skeletonContent}>
        <View
          style={[
            styles.skeletonBlock,
            { width: '100%', height: 60, marginBottom: 12 },
          ]}
        />
        <View
          style={[
            styles.skeletonBlock,
            { width: '80%', height: 40, marginBottom: 12 },
          ]}
        />
        <View style={[styles.skeletonBlock, { width: '90%', height: 40 }]} />
      </View>
    </View>
  );
};

/**
 * Phone Extension Panel
 * - The gesture is passed by the parent component. GestureDetector Control (useExtensionPanelGesture hook)
 * - This component is only responsible for: rendering the panel contents + Response button triggers visible prop Change
 * - Facilitating the intermediate dialogue window 80%Left 20%
 */
const PhoneExtensionPanel: React.FC<PhoneExtensionPanelProps> = ({
  visible,
  content,
  width,
  centerAnimValue,
  slideAnim,
  isGesturing,
}) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  // Panel Maximum Slide Distance
  const maxSlide = width * extensionPanel.phoneWidthRatio;

  // Control whether the panel is rendered (to be removed after animated, to avoid blinking)
  const [shouldRender, setShouldRender] = useState(visible);

  // ─── visible Ensure render status synchronized when changing ────────────────────
  // Whether the button triggers or the gesture triggers,visible=true Always replay the panel.
  useEffect(() => {
    if (visible) {
      setShouldRender(true);
    }
  }, [visible]);

  // ─── Button-activated animation sync (visible prop (in case of change) ────────
  // Skipping animated gestures while the gestures were in progress, avoiding gestures. onUpdate Conflict
  useEffect(() => {
    if (isGesturing.value) return;

    const target = visible ? maxSlide : 0;
    const timingConfig = {
      duration: ANIM_DURATION,
      easing: Easing.out(Easing.cubic),
    };

    slideAnim.value = withTiming(target, timingConfig);
    centerAnimValue.value = withTiming(target, timingConfig, (finished) => {
      // Remove rendering after closing animated animation
      if (finished && !visible) {
        runOnJS(setShouldRender)(false);
      }
    });
  }, [visible, maxSlide, slideAnim, centerAnimValue]);

  // ─── Panel Animation Styles ─────────────────────────────────────
  const panelAnimStyle = useAnimatedStyle(() => ({
    transform: [
      {
        // slideAnim [0, maxSlide] → translateX [-maxSlide, 0]
        // 0 → The panel is completely hidden outside the left screen.
        // maxSlide → Panel is fully visible
        translateX: slideAnim.value - maxSlide,
      },
    ],
  }));

  // Go back when the panel is not rendered null
  if (!shouldRender) {
    return null;
  }

  return (
    <Animated.View
      style={[
        styles.panel,
        { width: maxSlide, paddingTop: CHAT_HEADER_HEIGHT },
        panelAnimStyle,
      ]}
    >
      {content || <SkeletonPlaceholder colors={colors} />}
    </Animated.View>
  );
};

export default PhoneExtensionPanel;

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  panel: {
    position: 'absolute',
    left: 0,
    height: '100%',
    flexShrink: 1,
    backgroundColor: colors.bgChatPage,
    borderRightWidth: 1,
    borderRightColor: colors.extensionPanelBorder,
    overflow: 'hidden',
  },
  skeletonContainer: {
    flex: 1,
    backgroundColor: colors.bgChatPage,
    paddingTop: 80,
  },
  skeletonHeader: {
    paddingHorizontal: 20,
    marginBottom: 24,
  },
  skeletonContent: {
    paddingHorizontal: 20,
  },
  skeletonBlock: {
    backgroundColor: colors.extensionPanelSkeletonBlock,
    borderRadius: 8,
  },
});
