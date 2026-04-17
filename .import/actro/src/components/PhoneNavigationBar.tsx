import { Platform, requireNativeComponent } from 'react-native';
import type { StyleProp, ViewStyle } from 'react-native';

export interface PhoneNavigationBarProps {
  title: string;
  subtitle?: string;
  showLargeTitle?: boolean;
  style?: StyleProp<ViewStyle>;
}

export const PhoneNavigationBar =
  Platform.OS === 'ios'
    ? requireNativeComponent<PhoneNavigationBarProps>('PhoneNavigationBarView')
    : null;
