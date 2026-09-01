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

// Harmonized to contain the values associated with the structure at the bottom of the chat page, and maintain Chat2 The list, input bar, empty form and attachment panels use the same set of Platform rules.
export function useChatBottomInsets(isTabletLayout: boolean): ChatBottomInsets {
  const { keyboardInset, isKeyboardVisible, dismissKeyboard } =
    useKeyboardInset();
  const insets = useSafeAreaInsets();

  // iOS Phone Keyboard state maintains a smaller bottom breathing position, equal to the original AppInputMobileView The security zone shrinks.
  const iosPhoneKeyboardBottomPadding = Math.max(insets.bottom / 2, 10);
  // Android Phone Keep the white keyboard stable at the bottom and avoid it. adjustResize Later. padding Change the rejump.
  const androidStableBottomPadding = Math.max(
    insets.bottom,
    selectionActionBar.phoneMinBottomPadding,
  );

  return useMemo(() => {
    // List Only iOS Phone (i) The height of the keyboard shielding while the keyboard covers the page;Android Host adjustResize Indented.
    const listKeyboardPadding =
      Platform.OS === 'ios' && !isTabletLayout && isKeyboardVisible
        ? keyboardInset
        : 0;
    // Pad Do Not Enable KeyboardStickyView, iOS Pad Through the Input Bar marginBottom Follow the keyboard,Android Pad Still. adjustResize.
    const tabletKeyboardMarginBottom =
      Platform.OS === 'ios' && isTabletLayout ? keyboardInset : 0;
    // iOS Phone Empty fills the buttons from the input bar safe bottom, avoids the centering of content when it goes off the keyboard.
    const emptyStateBottomCompensation =
      Platform.OS === 'ios' && !isTabletLayout && isKeyboardVisible
        ? Math.max(insets.bottom - iosPhoneKeyboardBottomPadding, 0)
        : 0;
    // Mobile Enter the original rule to leave the bottom of the column:iOS Keyboard contraction,Android Always use the stable bottom to leave white.
    const mobileInputBottomPadding =
      isKeyboardVisible && Platform.OS !== 'android'
        ? iosPhoneKeyboardBottomPadding
        : Platform.OS === 'android'
          ? androidStableBottomPadding
          : insets.bottom;
    // Mobile Enter the location of the bottom of the bar to follow the final text of the prompt text padding Count, keep it as it is. iOS/Android Visual offset formula.
    const mobileFloatingTextBottom =
      Platform.OS === 'ios'
        ? (mobileInputBottomPadding - 24 + 10) / 2
        : (mobileInputBottomPadding -
            selectionActionBar.phoneMinBottomPadding) /
          2;
    // Attachment panel height used same safe bottom Enter, secure the outer animation and internal panel padding Unanimously.
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
