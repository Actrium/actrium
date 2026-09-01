import { requireNativeComponent } from 'react-native';
import type { StyleProp, ViewStyle } from 'react-native';

export interface PhoneNavigationBarProps {
  title: string;
  subtitle?: string;
  showLargeTitle?: boolean;
  style?: StyleProp<ViewStyle>;
}

// iOS 和 Android 均使用原生导出的 PhoneNavigationBarView
export const PhoneNavigationBar =
  requireNativeComponent<PhoneNavigationBarProps>('PhoneNavigationBarView');
