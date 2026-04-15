import { Platform, requireNativeComponent } from 'react-native';
import type { ViewStyle } from 'react-native';

export interface PhoneNavigationBarProps {
  title: string;
  subtitle?: string;
  showLargeTitle?: boolean;
  style?: ViewStyle;
}

export const PhoneNavigationBar =
  Platform.OS === 'ios'
    ? requireNativeComponent<PhoneNavigationBarProps>('PhoneNavigationBarView')
    : null;
