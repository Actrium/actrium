import { Platform } from 'react-native';
import DeviceInfo from 'react-native-device-info';

export const isIOS = Platform.OS === 'ios';
export const isAndroid = Platform.OS === 'android';

export const isTablet = DeviceInfo.isTablet();
export const isPhone = !DeviceInfo.isTablet();
