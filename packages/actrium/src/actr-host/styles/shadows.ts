import { Platform, type ViewStyle } from 'react-native';
import type { ColorTokens } from '../../theme/colors';

const LIGHT_SHADOW_COLOR = '#CCCCCC';

function hexToRgba(hexColor: string, opacity: number) {
  // Android boxShadow The hexadecimal shading of the theme needs to be made transparent. rgba.
  const hex = hexColor.replace('#', '');
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${opacity})`;
}

function createHostShadow(params: {
  offsetY: number;
  blurRadius: number;
  opacity: number;
  shadowColor: string;
}): ViewStyle {
  const { offsetY, blurRadius, opacity, shadowColor } = params;
  return Platform.select<ViewStyle>({
    ios: {
      shadowColor,
      shadowOffset: { width: 0, height: offsetY },
      shadowOpacity: opacity,
      shadowRadius: blurRadius,
    },
    android: {
      // Android Unanimously. boxShadowKeep the re-entry line and avoid it. elevation The feeling of falling.
      boxShadow: `0px ${offsetY}px ${blurRadius}px ${hexToRgba(shadowColor, opacity)}`,
    },
    default: {
      shadowColor,
      shadowOffset: { width: 0, height: offsetY },
      shadowOpacity: opacity,
      shadowRadius: blurRadius,
    },
  }) as ViewStyle;
}

export function getHostShadows(colors: Pick<ColorTokens, 'shadowColor'>) {
  return {
    inputPill: createHostShadow({
      offsetY: 2,
      blurRadius: 10,
      opacity: 1,
      shadowColor: colors.shadowColor,
    }),
    headerButton: createHostShadow({
      offsetY: 2,
      blurRadius: 10,
      opacity: 1,
      shadowColor: colors.shadowColor,
    }),
    floatingButton: createHostShadow({
      offsetY: 2,
      blurRadius: 10,
      opacity: 1,
      shadowColor: colors.shadowColor,
    }),
    compactFloatingButton: createHostShadow({
      offsetY: 2,
      blurRadius: 10,
      opacity: 1,
      shadowColor: colors.shadowColor,
    }),
    menuSurface: createHostShadow({
      offsetY: 8,
      blurRadius: 16,
      opacity: 0.12,
      shadowColor: colors.shadowColor,
    }),
    modalSurface: createHostShadow({
      offsetY: 10,
      blurRadius: 20,
      opacity: 0.1,
      shadowColor: colors.shadowColor,
    }),
    softSurface: createHostShadow({
      offsetY: 0,
      blurRadius: 8,
      opacity: 0.1,
      shadowColor: colors.shadowColor,
    }),
    mapSurface: createHostShadow({
      offsetY: 2,
      blurRadius: 10,
      opacity: 0.25,
      shadowColor: colors.shadowColor,
    }),
    mapActionButton: createHostShadow({
      offsetY: 2,
      blurRadius: 4,
      opacity: 0.2,
      shadowColor: colors.shadowColor,
    }),
  } as const;
}

export const HOST_SHADOWS = getHostShadows({
  shadowColor: LIGHT_SHADOW_COLOR,
});
