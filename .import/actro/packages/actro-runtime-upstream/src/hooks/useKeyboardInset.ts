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
    // iOS 回调里给的是键盘顶部 screenY，需转成“底部被遮挡高度”。
    const getOverlap = (screenY: number) => {
      const windowHeight = Dimensions.get('window').height;
      return Math.max(0, windowHeight - screenY);
    };

    const onKeyboardChange = (e: KeyboardEvent) => {
      if (!e?.endCoordinates) return;
      // iOS 用 frame 变化更精确（包含浮动/分离键盘位移）；Android 直接使用键盘高度。
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

    // 统一订阅/清理，避免页面切换后监听残留导致 inset 异常。
    const showSub = Keyboard.addListener(showEvent, onKeyboardChange);
    const hideSub = Keyboard.addListener(hideEvent, onKeyboardHide);

    return () => {
      showSub.remove();
      hideSub.remove();
    };
  }, []);

  return {
    keyboardInset,
    // 由 inset 推导可见性，避免依赖额外状态。
    isKeyboardVisible: keyboardInset > 0,
    dismissKeyboard: Keyboard.dismiss,
  };
}
