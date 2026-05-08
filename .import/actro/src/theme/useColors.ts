import { useColorScheme } from 'react-native';
import { lightColors, darkColors, ColorTokens } from './colors';

export function useColors(): ColorTokens {
  const scheme = useColorScheme();
  return scheme === 'dark' ? darkColors : lightColors;
}
