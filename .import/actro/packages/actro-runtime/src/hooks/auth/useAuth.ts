import { useAppStore } from '../../actr-host/store/useAppStore';
import { useAuthTaskStore } from '../../actr-host/store/useAuthTaskStore';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { authApi } from '../../api/auth';
import {
  AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT,
  authAccessControlStatus,
} from '../../actr-host/coreContract';

export type AuthStatus = 'authorized' | 'not_logged_in' | 'not_authorized';

interface AuthCheckResult {
  status: AuthStatus;
  clientId: string;
  appId: string;
}

const pendingAuthStatusRequests = new Map<string, Promise<AuthStatus>>();

const useAuth = (appId: string) => {
  const appInfo = useAppStore((state) => state.apps[appId]);

  /**
   * 检查授权状态，返回状态结果，由调用方决定如何处理
   */
  const checkAuth = async (): Promise<AuthCheckResult> => {
    const clientId = appInfo?.client_id || '';

    // 允许匿名访问
    if (appInfo?.allow_anonymous_access) {
      // 匿名应用也写入已授权缓存，保证依赖本地授权态的 UI（如输入框 overlay）能立即放行。
      useAuthTaskStore.getState().addAuthorizedAppId(appId);
      return {
        status: AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized,
        clientId,
        appId,
      };
    }

    const isLoggedIn = !!useUserStore.getState().userInfo?.user?.cn;

    if (!isLoggedIn) {
      return { status: 'not_logged_in', clientId, appId };
    }

    const userId = useUserStore.getState().userInfo?.user?.cn || '';
    const authTaskStore = useAuthTaskStore.getState();
    const cachedStatus = authTaskStore.getAppAuthorizationStatus(appId);

    if (cachedStatus === AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized) {
      return {
        status: AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized,
        clientId,
        appId,
      };
    }

    if (cachedStatus === AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.notAuthorized) {
      return {
        status: AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.notAuthorized,
        clientId,
        appId,
      };
    }

    // 本地没有授权缓存时，先请求一次远端授权状态，再写回 Store
    const requestKey = `${userId}:${appId}`;
    let pendingRequest = pendingAuthStatusRequests.get(requestKey);

    if (!pendingRequest) {
      pendingRequest = (async () => {
        try {
          const authRes = await authApi.getIsAuthorized({ appId, userId });
          const authStatus = authAccessControlStatus(authRes.success, authRes.code);

          if (authStatus === AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized) {
            useAuthTaskStore.getState().addAuthorizedAppId(appId);
            return AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized;
          }

          if (authStatus === AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.notAuthorized) {
            useAuthTaskStore.getState().addUnauthorizedAppId(appId);
          }

          return AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.notAuthorized;
        } catch (error) {
          console.error('[useAuth] Failed to fetch authorization status:', error);
          return AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.notAuthorized;
        } finally {
          pendingAuthStatusRequests.delete(requestKey);
        }
      })();

      pendingAuthStatusRequests.set(requestKey, pendingRequest);
    }

    const remoteStatus = await pendingRequest;
    return { status: remoteStatus, clientId, appId };
  };

  return { checkAuth };
};

export default useAuth;
