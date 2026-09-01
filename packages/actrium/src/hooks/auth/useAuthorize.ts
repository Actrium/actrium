import { useState, useCallback } from 'react';
import { Alert, NativeModules } from 'react-native';
import { authApi } from '../../api/auth';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { useAuthTaskStore } from '../../actr-host/store/useAuthTaskStore';
import { useTabIdMapStore } from '../../actr-host/store/useTabIdMapStore';
import { useTranslation } from 'react-i18next';
import { GlobalAlert } from '../../components';
import {
  AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT,
  authAccessControlStatus,
} from '../../actr-host/coreContract';

export type AuthorizeStep = 'idle' | 'authorizing' | 'authorized' | 'failed';

interface UseAuthorizeOptions {
  clientId: string;
  appId: string;
  /** Modal Mode: Not implemented Tab Toggle Logic */
  isModal?: boolean;
  /** Current Tab IDfor Tab Toggle Logic */
  currentTabId?: string;
}

interface UseAuthorizeResult {
  step: AuthorizeStep;
  loading: boolean;
  authorize: () => Promise<boolean>;
  reset: () => void;
}

/**
 * Pure authorization hook - Only the delegation of authority logic is processed, provided that it is logged in
 */
export const useAuthorize = ({
  clientId,
  appId,
  isModal = false,
  currentTabId,
}: UseAuthorizeOptions): UseAuthorizeResult => {
  const { t } = useTranslation();
  const [step, setStep] = useState<AuthorizeStep>('idle');
  const [loading, setLoading] = useState(false);

  const { userInfo, sessionToken } = useUserStore();

  const reset = useCallback(() => {
    setStep('idle');
    setLoading(false);
  }, []);

  /**
   * Completed after mandate: recorded authorized status, processed Tab Switch
   */
  const handleFinalizeAuth = useCallback(
    async (tempCode: string): Promise<boolean> => {
      if (!appId) {
        return true;
      }

      try {
        // 1. Check for authorization.
        const isAuthorized = await authApi.getIsAuthorized({
          appId,
          userId: userInfo?.user?.cn || '',
        });
        const authStatus = authAccessControlStatus(isAuthorized.success, isAuthorized.code);

        if (authStatus === AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized) {
          // Authorized
        } else {
          // 2. Unauthorized, temporary code for official Access Token
          const tokenRes = await authApi.getAccessTokenWithTempCode(
            { code: tempCode, appId },
            sessionToken!,
          );
          if (!tokenRes.success) {
            GlobalAlert.alert(t('actr.auth.activationFailed'), tokenRes.message);
            return false;
          }
        }

        // Will appId Other Organiser
        useAuthTaskStore.getState().addAuthorizedAppId(appId);

        // Modal Not implemented under Mode Tab Toggle Logic
        if (isModal) return true;

        // 3. Mission is working. Prepare to switch. Tab (Only Tab Mode)
        if (currentTabId) {
          const tabIdMap = useTabIdMapStore.getState().tabIdMap;
          const targetTabId = Object.keys(tabIdMap).find(
            (id) => tabIdMap[id] === appId,
          );

          if (targetTabId && NativeModules.TabModule?.selectTab) {
            NativeModules.TabModule.selectTab(targetTabId);

            if (NativeModules.TabModule?.closeTab) {
              setTimeout(() => {
                NativeModules.TabModule.closeTab(currentTabId);
              }, 500);
            }
          }
        }
        return true;
      } catch (e) {
        GlobalAlert.alert(t('common.error'), t('actr.auth.activationFailed'));
        return false;
      }
    },
    [appId, userInfo, sessionToken, isModal, currentTabId],
  );

  /**
   * Launching of the delegation process
   */
  const authorize = useCallback(async (): Promise<boolean> => {
    if (!sessionToken || !userInfo) {
      setStep('failed');
      return false;
    }

    if (!clientId || !appId) {
      // Mandate relies on clientId/appId, intercept directly when parameters are not in place
      // Avoid sending empty parameter requests to the backend, trigger“Application Error”Such misleading tips.
      GlobalAlert.alert(t('common.error'), t('actr.auth.authorizationFailed'));
      console.error(`clientId: ${clientId} or appId: ${appId} not exist`)
      setStep('failed');
      return false;
    }

    setLoading(true);
    setStep('authorizing');

    try {
      const loginHint = userInfo.user?.cn || '';
      const res = await authApi.authorize(clientId, loginHint, sessionToken);

      if (res.success && res.data) {
        const success = await handleFinalizeAuth(res.data.code);
        if (success) {
          setStep('authorized');
          return true;
        }
      } else {
        GlobalAlert.alert(t('actr.auth.authorizationFailed'), res.message);
      }
      setStep('failed');
      return false;
    } catch {
      GlobalAlert.alert(t('common.error'), t('actr.auth.authorizationError'));
      setStep('failed');
      return false;
    } finally {
      setLoading(false);
    }
  }, [clientId, appId, sessionToken, userInfo, handleFinalizeAuth]);

  return {
    step,
    loading,
    authorize,
    reset,
  };
};
