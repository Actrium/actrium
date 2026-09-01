import { useMemo } from 'react';
import { Platform } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useKeyboardInset } from '../../hooks/useKeyboardInset';
import { selectionActionBar } from '../../theme/tokens';
import { getMobileAttachmentPanelHeight } from '../features/input/MobileAttachmentPanel';

export interface ChatBottomInsets {
  keyboardInset: number;
  isKeyboardVisible: boolean;
  dismissKeyboard: () => void;
  listKeyboardPadding: number;
  tabletKeyboardMarginBottom: number;
  emptyStateBottomCompensation: number;
  mobileInputBottomPadding: number;
  mobileFloatingTextBottom: number;
  attachmentPanelHeight: number;
}

// 统一收敛聊天页底部结构相关数值，保持 Chat2 列表、输入栏、空态和附件面板使用同一套平台规则。
export function useChatBottomInsets(isTabletLayout: boolean): ChatBottomInsets {
  const { keyboardInset, isKeyboardVisible, dismissKeyboard } =
    useKeyboardInset();
  const insets = useSafeAreaInsets();

  // iOS Phone 键盘态保留较小底部呼吸位，等价于原 ActrInputMobileView 的安全区收缩逻辑。
  const iosPhoneKeyboardBottomPadding = Math.max(insets.bottom / 2, 10);
  // Android Phone 底部留白键盘前后保持稳定，避免 adjustResize 后再因 padding 变化二次跳动。
  const androidStableBottomPadding = Math.max(
    insets.bottom,
    selectionActionBar.phoneMinBottomPadding,
  );

  return useMemo(() => {
    // 列表只在 iOS Phone 键盘覆盖页面时叠加键盘遮挡高度；Android 由宿主 adjustResize 缩容器。
    const listKeyboardPadding =
      Platform.OS === 'ios' && !isTabletLayout && isKeyboardVisible
        ? keyboardInset
        : 0;
    // Pad 不启用 KeyboardStickyView，iOS Pad 通过输入栏 marginBottom 跟随键盘，Android Pad 仍交给 adjustResize。
    const tabletKeyboardMarginBottom =
      Platform.OS === 'ios' && isTabletLayout ? keyboardInset : 0;
    // iOS Phone 空态补回输入栏键盘态缩掉的 safe bottom，避免居中内容在键盘弹起时下沉。
    const emptyStateBottomCompensation =
      Platform.OS === 'ios' && !isTabletLayout && isKeyboardVisible
        ? Math.max(insets.bottom - iosPhoneKeyboardBottomPadding, 0)
        : 0;
    // Mobile 输入栏底部留白保留原规则：iOS 键盘态收缩，Android 始终使用稳定底部留白。
    const mobileInputBottomPadding =
      isKeyboardVisible && Platform.OS !== 'android'
        ? iosPhoneKeyboardBottomPadding
        : Platform.OS === 'android'
          ? androidStableBottomPadding
          : insets.bottom;
    // Mobile 输入栏底部提示文案的位置跟随最终 padding 计算，保持原 iOS/Android 视觉偏移公式。
    const mobileFloatingTextBottom =
      Platform.OS === 'ios'
        ? (mobileInputBottomPadding - 24 + 10) / 2
        : (mobileInputBottomPadding -
            selectionActionBar.phoneMinBottomPadding) /
          2;
    // 附件面板高度使用同一个 safe bottom 输入，保证外层动画位移和面板内部 padding 一致。
    const attachmentPanelHeight = getMobileAttachmentPanelHeight(insets.bottom);

    return {
      keyboardInset,
      isKeyboardVisible,
      dismissKeyboard,
      listKeyboardPadding,
      tabletKeyboardMarginBottom,
      emptyStateBottomCompensation,
      mobileInputBottomPadding,
      mobileFloatingTextBottom,
      attachmentPanelHeight,
    };
  }, [
    androidStableBottomPadding,
    dismissKeyboard,
    insets.bottom,
    iosPhoneKeyboardBottomPadding,
    isKeyboardVisible,
    isTabletLayout,
    keyboardInset,
  ]);
}
