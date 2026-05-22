import { Platform, Vibration } from 'react-native';

export type HapticFeedbackType =
  | 'selection'
  | 'impactLight'
  | 'impactMedium'
  | 'impactHeavy'
  | 'rigid'
  | 'soft'
  | 'notificationSuccess'
  | 'notificationWarning'
  | 'notificationError'
  | 'clockTick'
  | 'contextClick'
  | 'keyboardPress'
  | 'keyboardRelease'
  | 'keyboardTap'
  | 'longPress'
  | 'textHandleMove'
  | 'virtualKey'
  | 'virtualKeyRelease'
  | 'effectClick'
  | 'effectDoubleClick'
  | 'effectHeavyClick'
  | 'effectTick';

type HapticOptions = {
  // iOS 触觉不可用时是否允许退回到系统震动，默认行为由 react-native-haptic-feedback 决定。
  enableVibrateFallback?: boolean;
  // Android 使用 RNHapticFeedback 时是否忽略系统触觉反馈开关；当前 Android 分支不走该模块。
  ignoreAndroidSystemSettings?: boolean;
};

type HapticModule = {
  trigger: (type: HapticFeedbackType, options?: HapticOptions) => void;
};

// 长震动默认时长，仅用于 Android 单次长震；iOS 侧该时长不会被精确遵守。
const DEFAULT_LONG_VIBRATION_MS = 300;
// 持续震动 pattern，Android 通过 repeat=true 一直循环，直到调用 stopVibration。
const CONTINUOUS_VIBRATION_PATTERN_MS = [0, 1000];

let cachedHapticModule: HapticModule | null | undefined;

function getHapticModule(): HapticModule | null {
  if (cachedHapticModule !== undefined) {
    return cachedHapticModule;
  }

  if (Platform.OS === 'android') {
    cachedHapticModule = null;
    return null;
  }

  try {
    // 中文注释：改成运行时按需加载，避免宿主当前未暴露该 TurboModule 时，
    // 在首屏渲染阶段因为顶层 import 直接触发 getEnforcing 崩溃。
    // Android 当前宿主没有正确暴露 RNHapticFeedback，因此这里只允许 iOS 侧尝试加载该库。
    const requiredModule = require('react-native-haptic-feedback');
    cachedHapticModule = requiredModule?.default ?? requiredModule ?? null;
  } catch {
    cachedHapticModule = null;
  }

  return cachedHapticModule ?? null;
}

/**
 * 震动
 * - impactLight / impactMedium / impactHeavy：轻/中/重力度的碰撞反馈。
 */
export function triggerHaptic(
  type: HapticFeedbackType,
  options?: HapticOptions,
) {
  if (Platform.OS === 'android') {
    // 中文注释：Android 宿主当前缺少 RNHapticFeedback TurboModule，
    // 直接退回到系统振动能力，避免运行时再次触发 getEnforcing 崩溃。
    Vibration.vibrate(30);
    return;
  }

  const hapticModule = getHapticModule();
  if (!hapticModule) {
    return;
  }

  hapticModule.trigger(type, options);
}

/**
 * 触发明确的长震动反馈，用于需要比 triggerHaptic('impactHeavy') 更明显的场景。
 * React Native 官方文档说明：Android 可指定 duration，iOS 的 vibration duration 固定，传入毫秒值不会得到同等时长。
 */
export function triggerLongVibration(durationMs = DEFAULT_LONG_VIBRATION_MS) {
  if (Platform.OS === 'android') {
    // 中文注释：Android 支持 number duration，这里是真正的长震。
    Vibration.vibrate(durationMs);
    return;
  }

  // 中文注释：iOS 的系统 vibration duration 固定，传入 durationMs 不会得到真正的长震。
  Vibration.vibrate();
}

/**
 * 中文注释：开始持续震动。
 * Android 使用 repeat=true 循环系统震动；iOS 当前这套 API 不支持真正连续震动，因此直接返回 false。
 */
export function startContinuousVibration() {
  if (Platform.OS !== 'android') {
    return false;
  }

  Vibration.vibrate(CONTINUOUS_VIBRATION_PATTERN_MS, true);
  return true;
}

/**
 * 中文注释：停止当前通过 Vibration 启动的持续震动。
 */
export function stopVibration() {
  Vibration.cancel();
}
