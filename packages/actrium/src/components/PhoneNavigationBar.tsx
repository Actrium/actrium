import { requireNativeComponent } from 'react-native';
import type { StyleProp, ViewStyle } from 'react-native';

export interface PhoneNavigationBarProps {
  title: string;
  subtitle?: string;
  showLargeTitle?: boolean;
  style?: StyleProp<ViewStyle>;
}

// iOS and Android It's all from the original. PhoneNavigationBarView
export const PhoneNavigationBar =
  requireNativeComponent<PhoneNavigationBarProps>('PhoneNavigationBarView');
