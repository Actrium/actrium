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
  // iOS Whether to allow a return to the system vibration when the touch is not available and the default behaviour is by react-native-haptic-feedback Decision.
  enableVibrateFallback?: boolean;
  // Android Use RNHapticFeedback whether to ignore system touch feedback switches; current Android The branch does not go with the module.
  ignoreAndroidSystemSettings?: boolean;
};

type HapticModule = {
  trigger: (type: HapticFeedbackType, options?: HapticOptions) => void;
};

// The default time for long vibrations, only for Android (a) Single long-earthquake;iOS The side does not exactly follow the time frame.
const DEFAULT_LONG_VIBRATION_MS = 300;
// Keep shaking. pattern, Android Pass. repeat=true Keep looping until call stopVibration.
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
    // Load as needed to avoid the host's current exposure TurboModule I'm not sure if I'm going to be able to do this.
    // At the top of the screen, because of the top layer import Direct trigger getEnforcing Crash.
    // Android The host is not correctly exposed. RNHapticFeedback, so only allowed here iOS Side try to load the library.
    const requiredModule = require('react-native-haptic-feedback');
    cachedHapticModule = requiredModule?.default ?? requiredModule ?? null;
  } catch {
    cachedHapticModule = null;
  }

  return cachedHapticModule ?? null;
}

/**
 * Vibration
 * - impactLight / impactMedium / impactHeavyLight/Medium/Heavy impact feedback.
 */
export function triggerHaptic(
  type: HapticFeedbackType,
  options?: HapticOptions,
) {
  if (Platform.OS === 'android') {
    // Android Host is currently missing RNHapticFeedback TurboModule,
    // Directly back to system vibration capability to avoid a re-activation at running getEnforcing Crash.
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
 * Triggers a clear long-earthing feedback for needs comparison triggerHaptic('impactHeavy') More visible scenes.
 * React Native Official document description:Android Assignable duration, iOS It's... vibration duration Fixed, the milliseconds are not passed for the same length.
 */
export function triggerLongVibration(durationMs = DEFAULT_LONG_VIBRATION_MS) {
  if (Platform.OS === 'android') {
    // Android Support number durationHere's the real long shock.
    Vibration.vibrate(durationMs);
    return;
  }

  // iOS System vibration duration Fixed, in. durationMs Won't get real long tremors.
  Vibration.vibrate();
}

/**
 * Note: Starts to vibrate continuously.
 * Android Use repeat=true (a) Cyclical system vibration;iOS Current API Not supporting real continuous vibrations, so go straight back. false.
 */
export function startContinuousVibration() {
  if (Platform.OS !== 'android') {
    return false;
  }

  Vibration.vibrate(CONTINUOUS_VIBRATION_PATTERN_MS, true);
  return true;
}

/**
 * Chinese Comment: Stop Current Pass Vibration Started a continuous vibration.
 */
export function stopVibration() {
  Vibration.cancel();
}
