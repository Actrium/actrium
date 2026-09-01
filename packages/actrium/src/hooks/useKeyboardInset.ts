import { useEffect, useState } from 'react';
import { Dimensions, Keyboard, KeyboardEvent, Platform } from 'react-native';

export interface UseKeyboardInsetResult {
  keyboardInset: number;
  isKeyboardVisible: boolean;
  dismissKeyboard: () => void;
}

export function useKeyboardInset(): UseKeyboardInsetResult {
  const [keyboardInset, setKeyboardInset] = useState(0);

  useEffect(() => {
    // iOS The keyboard top is in the back. screenY, needs to be converted“Bottom is blocked.”.
    const getOverlap = (screenY: number) => {
      const windowHeight = Dimensions.get('window').height;
      return Math.max(0, windowHeight - screenY);
    };

    const onKeyboardChange = (e: KeyboardEvent) => {
      if (!e?.endCoordinates) return;
      // iOS Use frame Change more precisely (include floating)/Separation keyboard slot shift;Android Use the keyboard height directly.
      if (Platform.OS === 'ios') {
        setKeyboardInset(getOverlap(e.endCoordinates.screenY));
      } else {
        setKeyboardInset(Math.max(0, e.endCoordinates.height ?? 0));
      }
    };

    const onKeyboardHide = () => setKeyboardInset(0);

    const showEvent =
      Platform.OS === 'ios' ? 'keyboardWillChangeFrame' : 'keyboardDidShow';
    const hideEvent =
      Platform.OS === 'ios' ? 'keyboardWillHide' : 'keyboardDidHide';

    // Unified Subscriptions/Clear to avoid post-interrupt-to-delay residuals inset Unusual.
    const showSub = Keyboard.addListener(showEvent, onKeyboardChange);
    const hideSub = Keyboard.addListener(hideEvent, onKeyboardHide);

    return () => {
      showSub.remove();
      hideSub.remove();
    };
  }, []);

  return {
    keyboardInset,
    // By inset (c) Inducing visibility and avoiding reliance on additional status.
    isKeyboardVisible: keyboardInset > 0,
    dismissKeyboard: Keyboard.dismiss,
  };
}
