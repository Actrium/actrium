import { Platform, Vibration } from 'react-native';

type HapticModule = {
  trigger: (
    type: string,
    options?: {
      enableVibrateFallback?: boolean;
      ignoreAndroidSystemSettings?: boolean;
    },
  ) => void;
};

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

export function triggerHaptic(
  type: string,
  options?: {
    enableVibrateFallback?: boolean;
    ignoreAndroidSystemSettings?: boolean;
  },
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
