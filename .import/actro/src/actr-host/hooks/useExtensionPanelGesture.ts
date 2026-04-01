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

// ─── 手势配置常量 ───────────────────────────────────────
/** 水平滑动超过此距离才激活手势（避免误触） */
const GESTURE_ACTIVE_OFFSET_X = 15;
/** 垂直滑动超过此距离则放弃手势（让给 ScrollView） */
const GESTURE_FAIL_OFFSET_Y = 10;
/** 滑动距离超过最大距离的此比例，视为完成开/关 */
const SNAP_THRESHOLD_RATIO = 0.3;
/** 滑动速度超过此值，直接完成开/关（px/s） */
const VELOCITY_THRESHOLD = 500;
/** 收尾动画时长 */
const ANIM_DURATION = 250;

/**
 * 将值限制在 [min, max] 范围内（worklet 兼容）
 */
function clamp(value: number, min: number, max: number): number {
  'worklet';
  return Math.min(Math.max(value, min), max);
}

interface UseExtensionPanelGestureParams {
  /** 扩展面板基准宽度；需与按钮触发路径保持一致 */
  panelWidth: number;
  /** 面板滑动动画值（0 = 关闭，maxSlide = 打开） */
  slideAnim: SharedValue<number>;
  /** 中间对话区域偏移动画值 */
  centerAnimValue: SharedValue<number>;
  /** 面板可见状态变化回调 */
  onVisibilityChange: (visible: boolean) => void;
}

/**
 * 扩展面板手势 hook
 * - 仅 Phone 模式启用，Tablet 返回空手势
 * - 右滑打开面板，左滑关闭面板
 * - 手势跟手，松手后根据速度和距离自动吸附
 */
export function useExtensionPanelGesture({
  panelWidth,
  slideAnim,
  centerAnimValue,
  onVisibilityChange,
}: UseExtensionPanelGestureParams) {
  // 与按钮触发路径保持一致：手势路径也基于固定面板宽度计算实际展开宽度。
  const maxSlide = panelWidth * extensionPanel.phoneWidthRatio;
  // 手势开始时记录的起始位置
  const startX = useSharedValue(0);
  // 手势进行中标记（防止 useEffect 中的按钮动画和手势冲突）
  const isGesturing = useSharedValue(false);

  const handleOpen = useCallback(() => {
    onVisibilityChange(true);
  }, [onVisibilityChange]);

  const handleClose = useCallback(() => {
    onVisibilityChange(false);
  }, [onVisibilityChange]);

  // 手势开始时确保面板渲染（用户可能从关闭状态右滑打开）
  const handleGestureStart = useCallback(() => {
    onVisibilityChange(true);
  }, [onVisibilityChange]);

  // Tablet 模式不需要手势，返回空手势
  if (isTablet) {
    return {
      gesture: Gesture.Manual(),
      maxSlide,
      isGesturing,
    };
  }

  const panGesture = Gesture.Pan()
    // 水平滑动超过阈值才激活，避免和垂直滚动冲突
    .activeOffsetX([-GESTURE_ACTIVE_OFFSET_X, GESTURE_ACTIVE_OFFSET_X])
    .failOffsetY([-GESTURE_FAIL_OFFSET_Y, GESTURE_FAIL_OFFSET_Y])
    .onStart(() => {
      'worklet';
      isGesturing.value = true;
      startX.value = slideAnim.value;
      // 确保面板渲染（从关闭状态右滑时需要先渲染面板）
      runOnJS(handleGestureStart)();
    })
    .onUpdate((e) => {
      'worklet';
      // 跟手：将手指位移映射到面板位置，clamp 在有效范围内
      const next = clamp(startX.value + e.translationX, 0, maxSlide);
      slideAnim.value = next;
      centerAnimValue.value = next;
    })
    .onEnd((e) => {
      'worklet';
      const currentPos = slideAnim.value;
      // 判断是否应该打开：速度够快 或 滑动距离超过阈值
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
