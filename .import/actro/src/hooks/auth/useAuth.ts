import { useAppStore } from '../../actr-host/store/useAppStore';
import { useAuthTaskStore } from '../../actr-host/store/useAuthTaskStore';
import { useUserStore } from '../../actr-host/store/useUserStore';

export type AuthStatus = 'authorized' | 'not_logged_in' | 'not_authorized';

interface AuthCheckResult {
  status: AuthStatus;
  clientId: string;
  appId: string;
}

const useAuth = (appId: string) => {
  const appInfo = useAppStore((state) => state.apps[appId]);

  /**
   * 检查授权状态，返回状态结果，由调用方决定如何处理
   */
  const checkAuth = (): AuthCheckResult => {
    const clientId = appInfo?.client_id || '';

    // 允许匿名访问
    if (appInfo?.allow_anonymous_access) {
      return { status: 'authorized', clientId, appId };
    }

    const isLoggedIn = !!useUserStore.getState().userInfo?.user?.cn;

    if (!isLoggedIn) {
      return { status: 'not_logged_in', clientId, appId };
    }

    const isAuthorized = useAuthTaskStore
      .getState()
      .alreadyAuthorizedAppIds.has(appId);

    if (true || !isAuthorized) {
      return { status: 'not_authorized', clientId, appId };
    }

    return { status: 'authorized', clientId, appId };
  };

  return { checkAuth };
};

export default useAuth;
