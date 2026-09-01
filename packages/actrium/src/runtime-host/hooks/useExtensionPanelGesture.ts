import { useCallback } from 'react';
import { useWindowDimensions } from 'react-native';
import { Gesture } from 'react-native-gesture-handler';
import {
  useSharedValue,
  withTiming,
  Easing,
  runOnJS,
} from 'react-native-reanimated';
import type { SharedValue } from 'react-native-reanimated';
import { isTablet } from '../../utils/device';
import { extensionPanel } from '../../theme/tokens';

// ─── Gesture Configuration Constant ───────────────────────────────────────
/** The gesture is activated by a horizontal slide above this distance (avoid error) */
const GESTURE_ACTIVE_OFFSET_X = 15;
/** Slide vertically beyond this distance and drop the gesture (give to ScrollView) */
const GESTURE_FAIL_OFFSET_Y = 10;
/** The percentage of the slide distance above the maximum is considered to be completed/Off */
const SNAP_THRESHOLD_RATIO = 0.3;
/** Slide speed above this value, and drive directly./Off (%2)px/s) */
const VELOCITY_THRESHOLD = 500;
/** End-of-the-art animation period */
const ANIM_DURATION = 250;

/**
 * Limit Value to [min, max] Scope (%)worklet Compatible)
 */
function clamp(value: number, min: number, max: number): number {
  'worklet';
  return Math.min(Math.max(value, min), max);
}

interface UseExtensionPanelGestureParams {
  /** Broaden panel base width; to be consistent with button-trigger path */
  panelWidth: number;
  /** Panel Animation Value (Scruise)0 = Close,maxSlide = Open */
  slideAnim: SharedValue<number>;
  /** Middle dialogue area offset painting value */
  centerAnimValue: SharedValue<number>;
  /** Panel Visible State Change Echo */
  onVisibilityChange: (visible: boolean) => void;
}

/**
 * Expand Panel Gesture hook
 * - Only Phone Mode enabled,Tablet Return empty gestures
 * - Right slide open panel, left slide close panel
 * - Hands on the side, and then let go of the hand, and it's automatic adsorbing at speed and distance.
 */
export function useExtensionPanelGesture({
  panelWidth,
  slideAnim,
  centerAnimValue,
  onVisibilityChange,
}: UseExtensionPanelGestureParams) {
  // Aligns with the button trigger path: The gesture path is also based on the fixed panel width to calculate the actual width.
  const maxSlide = panelWidth * extensionPanel.phoneWidthRatio;
  // The starting point for the sign at the beginning
  const startX = useSharedValue(0);
  // Gestures are in progress (prevent useEffect in which the buttons are animated and hand gestures conflict)
  const isGesturing = useSharedValue(false);

  const handleOpen = useCallback(() => {
    onVisibilityChange(true);
  }, [onVisibilityChange]);

  const handleClose = useCallback(() => {
    onVisibilityChange(false);
  }, [onVisibilityChange]);

  // The hand gesture starts to ensure that the panel is rendered (user may slide right from the closed state)
  const handleGestureStart = useCallback(() => {
    onVisibilityChange(true);
  }, [onVisibilityChange]);

  // Tablet No sign of the pattern. Back to empty.
  if (isTablet) {
    return {
      gesture: Gesture.Manual(),
      maxSlide,
      isGesturing,
    };
  }

  const panGesture = Gesture.Pan()
    // Horizontal slide above the threshold to activate, avoid rolling conflict with vertical
    .activeOffsetX([-GESTURE_ACTIVE_OFFSET_X, GESTURE_ACTIVE_OFFSET_X])
    .failOffsetY([-GESTURE_FAIL_OFFSET_Y, GESTURE_FAIL_OFFSET_Y])
    .onStart(() => {
      'worklet';
      isGesturing.value = true;
      startX.value = slideAnim.value;
      // Ensure Panel Rendering (need to render panel before slide from right-hand closed)
      runOnJS(handleGestureStart)();
    })
    .onUpdate((e) => {
      'worklet';
      // The #SceneFinger: Move the finger to the panel position,clamp Within the limits of effectiveness
      const next = clamp(startX.value + e.translationX, 0, maxSlide);
      slideAnim.value = next;
      centerAnimValue.value = next;
    })
    .onEnd((e) => {
      'worklet';
      const currentPos = slideAnim.value;
      // The decision is whether or not to open it: fast enough. or Slide distance above threshold
      const shouldOpen =
        e.velocityX > VELOCITY_THRESHOLD ||
        (e.velocityX > -VELOCITY_THRESHOLD &&
          currentPos > maxSlide * SNAP_THRESHOLD_RATIO);

      const target = shouldOpen ? maxSlide : 0;
      const timingConfig = {
        duration: ANIM_DURATION,
        easing: Easing.out(Easing.cubic),
      };

      slideAnim.value = withTiming(target, timingConfig);
      centerAnimValue.value = withTiming(target, timingConfig, (finished) => {
        if (finished) {
          isGesturing.value = false;
          runOnJS(shouldOpen ? handleOpen : handleClose)();
        }
      });
    });

  return {
    gesture: panGesture,
    maxSlide,
    isGesturing,
  };
}
