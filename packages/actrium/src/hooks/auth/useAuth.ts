import { useAppStore } from '../../runtime-host/store/useAppStore';
import { useAuthTaskStore } from '../../runtime-host/store/useAuthTaskStore';
import { useUserStore } from '../../runtime-host/store/useUserStore';
import { authApi } from '../../api/auth';
import {
  AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT,
  authAccessControlStatus,
} from '../../runtime-host/coreContract';

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
   * Check for authorization, return to status, and decide how to handle it by the caller
   */
  const checkAuth = async (): Promise<AuthCheckResult> => {
    const clientId = appInfo?.client_id || '';

    // Allow anonymous access
    if (appInfo?.allow_anonymous_access) {
      // Anonymous applications are also written to authorized caches, which ensure reliance on local authorization UI(e.g. input box) overlayIt will be released immediately.
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

    // If local no authorized cache, request a remote authorization status and write back. Store
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
