import type { SharedValue } from 'react-native-reanimated';

/**
 * Phone Extension Panel props
 * - Use reanimated SharedValue Drive hand gestures and hand-imaging
 * - slideAnim By parent component (hookCreate and share, hand gestures and buttons move through the drive panel position
 */
export interface PhoneExtensionPanelProps {
  visible: boolean;
  content: React.ReactElement | null;
  width: number;
  /** Middle dialogue area offset painting value */
  centerAnimValue: SharedValue<number>;
  /** Panel Slider Position (Pilot)0 = Close,maxSlide = Open) by hook Create */
  slideAnim: SharedValue<number>;
  /** The gesture is marked as true Animated by skipping a button */
  isGesturing: SharedValue<boolean>;
}

/**
 * Tablet Extension Panel props
 * - No gestures interactive.centerAnimValue Always As 0
 */
export interface TabletExtensionPanelProps {
  visible: boolean;
  content: React.ReactElement | null;
  width: number;
  centerAnimValue: SharedValue<number>;
}

/**
 * External harmonization props(by) index.tsx by-pass distribution)
 */
export interface ExtensionPanelProps {
  visible: boolean;
  content: React.ReactElement | null;
  width: number;
  centerAnimValue: SharedValue<number>;
  /** Phone Panel Animation Value Required for Mode */
  slideAnim: SharedValue<number>;
  /** Gestures are in progress. */
  isGesturing: SharedValue<boolean>;
}
