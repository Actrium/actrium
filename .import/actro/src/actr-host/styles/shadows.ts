import { Platform, type ViewStyle } from 'react-native';

function createHostShadow(params: {
  offsetY: number;
  blurRadius: number;
  opacity: number;
}): ViewStyle {
  const { offsetY, blurRadius, opacity } = params;
  return Platform.select<ViewStyle>({
    ios: {
      shadowColor: '#CCC',
      shadowOffset: { width: 0, height: offsetY },
      shadowOpacity: opacity,
      shadowRadius: blurRadius,
    },
    android: {
      // 中文注释：Android 统一走 boxShadow，保持复用入口一致，同时避开 elevation 的下坠感。
      boxShadow: `0px ${offsetY}px ${blurRadius}px rgba(204, 204, 204, ${opacity})`,
    },
    default: {
      shadowColor: '#CCC',
      shadowOffset: { width: 0, height: offsetY },
      shadowOpacity: opacity,
      shadowRadius: blurRadius,
    },
  }) as ViewStyle;
}

export const HOST_SHADOWS = {
  inputPill: createHostShadow({ offsetY: 2, blurRadius: 10, opacity: 1 }),
  headerButton: createHostShadow({ offsetY: 2, blurRadius: 10, opacity: 1 }),
  floatingButton: createHostShadow({ offsetY: 2, blurRadius: 10, opacity: 1 }),
  compactFloatingButton: createHostShadow({ offsetY: 2, blurRadius: 10, opacity: 1 }),
  menuSurface: createHostShadow({ offsetY: 8, blurRadius: 16, opacity: 0.12 }),
  modalSurface: createHostShadow({ offsetY: 10, blurRadius: 20, opacity: 0.1 }),
  softSurface: createHostShadow({ offsetY: 0, blurRadius: 8, opacity: 0.1 }),
  mapSurface: createHostShadow({ offsetY: 2, blurRadius: 10, opacity: 0.25 }),
  mapActionButton: createHostShadow({ offsetY: 2, blurRadius: 4, opacity: 0.2 }),
} as const;
