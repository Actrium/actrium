import React from 'react';
import { Pressable, StyleProp, StyleSheet, ViewStyle } from 'react-native';
import { useAuthTaskStore } from '../../store/useAuthTaskStore';
import { useAppStore } from '../../store/useAppStore';
import { useUserStore } from '../../store/useUserStore';

interface AuthGateOverlayProps {
  appId: string;
  style?: StyleProp<ViewStyle>;
  /** 授权预检回调，未授权返回 false */
  checkInputAuth?: () => Promise<boolean>;
}

/**
 * 授权拦截层：覆盖在输入框上方，未授权时拦截触摸并触发授权检查。
 * 授权通过后自动移除，允许底层 TextInput 正常交互。
 * 退出登录时（userInfo 变为 null）自动恢复拦截。
 * Pad 端和 Mobile 端共用。
 */
export function AuthGateOverlay({
  appId,
  style,
  checkInputAuth,
}: AuthGateOverlayProps) {
  const userInfo = useUserStore((state) => state.userInfo);
  const appStatus = useAppStore(state => state.apps[appId]?.app_status)
  const allowAnonymousAccess = useAppStore(
    (state) => state.apps[appId]?.allow_anonymous_access,
  );
  const authStatus = useAuthTaskStore(
    (state) => state.alreadyAuthorizedAppIds[appId] || 'unknown',
  );
  const isLoggedIn = !!userInfo?.user?.cn;
  const shouldBlockInput =
    !allowAnonymousAccess && (!isLoggedIn || authStatus !== 'authorized') && appStatus === 'alive';

  const handlePress = React.useCallback(async () => {
    if (!checkInputAuth) {
      return;
    }
    await checkInputAuth();
  }, [checkInputAuth]);

  // overlay 显隐直接绑定登录态和授权缓存，状态更新后自动刷新 UI。
  if (!shouldBlockInput) return null;

  return (
    <Pressable
      style={[styles.overlay, style]}
      onPressIn={handlePress}
    />
  );
}

const styles = StyleSheet.create({
  // 透明覆盖层，拦截底层 TextInput 的触摸事件。
  // zIndex 需高于 Mobile 端的 mobileInputGestureOverlay (zIndex: 20)。
  overlay: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    bottom: 0,
    zIndex: 30,
  },
});
