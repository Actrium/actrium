import { useCallback, useEffect, useRef, useState } from 'react';
import { NativeModules, Platform } from 'react-native';

export type TNetworkPermissionState =
  | 'idle'
  | 'requesting'
  | 'granted'
  | 'denied'
  | 'unsupported';

type TNetworkPermissionModule = {
  requestPermission?: () => Promise<boolean> | boolean;
  requestLocalNetworkPermission?: () => Promise<boolean> | boolean;
};

function getNetworkPermissionModule(): TNetworkPermissionModule | null {
  return (
    (NativeModules as any).NetworkPermissionModule ??
    (NativeModules as any).LocalNetworkPermissionModule ??
    (NativeModules as any).LocalNetworkModule ??
    null
  );
}

export function useNetworkPermission() {
  const [networkPermissionState, setNetworkPermissionState] =
    useState<TNetworkPermissionState>('idle');
  const permissionRequestRef = useRef<Promise<boolean> | null>(null);

  const ensureNetworkPermission = useCallback(async (): Promise<boolean> => {
    try {
      if (Platform.OS === 'android') {
        // Android 常规网络访问权限是安装时权限，这里统一视为无需额外门控。
        setNetworkPermissionState('unsupported');
        return true;
      }

      if (Platform.OS !== 'ios') {
        setNetworkPermissionState('unsupported');
        return true;
      }

      if (networkPermissionState === 'granted') {
        return true;
      }

      if (networkPermissionState === 'denied') {
        return false;
      }

      if (permissionRequestRef.current) {
        return permissionRequestRef.current;
      }

      const networkPermissionModule = getNetworkPermissionModule();
      if (
        !networkPermissionModule?.requestPermission &&
        !networkPermissionModule?.requestLocalNetworkPermission
      ) {
        console.log(
          '[useNetworkPermission] Local network permission module is unavailable, skip active request.',
        );
        setNetworkPermissionState('unsupported');
        return true;
      }

      setNetworkPermissionState('requesting');

      const requestPromise = (async () => {
        const requestResult = networkPermissionModule?.requestPermission
          ? await Promise.resolve(networkPermissionModule.requestPermission())
          : await Promise.resolve(
              networkPermissionModule!.requestLocalNetworkPermission!(),
            );
        const granted = requestResult !== false;
        setNetworkPermissionState(granted ? 'granted' : 'denied');
        return granted;
      })();

      permissionRequestRef.current = requestPromise;

      try {
        return await requestPromise;
      } finally {
        permissionRequestRef.current = null;
      }
    } catch (error) {
      console.warn(
        '[useNetworkPermission] Failed to request network permission:',
        error,
      );
      setNetworkPermissionState('denied');
      return false;
    }
  }, [networkPermissionState]);

  useEffect(() => {
    // 预热权限状态，避免真正的首个业务请求撞上系统弹窗。
    void ensureNetworkPermission();
  }, [ensureNetworkPermission]);

  return {
    ensureNetworkPermission,
    networkPermissionState,
  };
}
