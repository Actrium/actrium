import { useState, useCallback } from 'react';
import { Alert, NativeModules } from 'react-native';
import { authApi } from '../../api/auth';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { useAuthTaskStore } from '../../actr-host/store/useAuthTaskStore';
import { useTabIdMapStore } from '../../actr-host/store/useTabIdMapStore';
import { useTranslation } from 'react-i18next';
import { GlobalAlert } from '../../components';

export type AuthorizeStep = 'idle' | 'authorizing' | 'authorized' | 'failed';

interface UseAuthorizeOptions {
  clientId: string;
  appId: string;
  /** Modal 模式：不执行 Tab 切换逻辑 */
  isModal?: boolean;
  /** 当前 Tab ID，用于 Tab 切换逻辑 */
  currentTabId?: string;
}

interface UseAuthorizeResult {
  step: AuthorizeStep;
  loading: boolean;
  authorize: () => Promise<boolean>;
  reset: () => void;
}

/**
 * 纯授权 hook - 仅处理授权逻辑，前提是已登录
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
   * 完成授权后的处理：记录已授权状态，处理 Tab 切换
   */
  const handleFinalizeAuth = useCallback(
    async (tempCode: string): Promise<boolean> => {
      if (!appId) {
        return true;
      }

      try {
        // 1. 检查是否已授权
        const isAuthorized = await authApi.getIsAuthorized({
          appId,
          userId: userInfo?.user?.cn || '',
        });

        if (isAuthorized.success && isAuthorized.code === 0) {
          // 已授权
        } else {
          // 2. 未授权，用临时码换取正式 Access Token
          const tokenRes = await authApi.getAccessTokenWithTempCode(
            { code: tempCode, appId },
            sessionToken!,
          );
          if (!tokenRes.success) {
            GlobalAlert.alert(t('actr.auth.activationFailed'), tokenRes.message);
            return false;
          }
        }

        // 将 appId 放到已授权列表
        useAuthTaskStore.getState().addAuthorizedAppId(appId);

        // Modal 模式下不执行 Tab 切换逻辑
        if (isModal) return true;

        // 3. 授权成功，准备切换 Tab (仅 Tab 模式)
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
   * 发起授权流程
   */
  const authorize = useCallback(async (): Promise<boolean> => {
    if (!sessionToken || !userInfo) {
      setStep('failed');
      return false;
    }

    if (!clientId || !appId) {
      // 授权依赖 clientId/appId，参数未就绪时直接拦截
      // 避免向后端发送空参数请求，触发“应用错误”这类误导性提示。
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
