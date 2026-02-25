import { Platform } from 'react-native';

export const TABLET_MIN_WIDTH = 768;

export type WidthClass = 'phone' | 'tablet';

export const isIOS = Platform.OS === 'ios';
export const isAndroid = Platform.OS === 'android';

export function getWidthClass(width: number): WidthClass {
  return width >= TABLET_MIN_WIDTH ? 'tablet' : 'phone';
}

export function isTabletWidth(width: number): boolean {
  return getWidthClass(width) === 'tablet';
}
