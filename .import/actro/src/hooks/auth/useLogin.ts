import { useState, useEffect, useCallback } from 'react';
import { NativeModules, NativeEventEmitter, Alert } from 'react-native';
import { authApi, CURRENT_ENV } from '../../api/auth';
import { AuthRepository } from '../../repository/AuthRepository';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { AccountUserInfo } from '../../api/types';
import { useTranslation } from 'react-i18next';

const UserInfoModule = NativeModules.UserInfoModule;
const UserInfoEvents = new NativeEventEmitter(UserInfoModule);

export type LoginStep =
  | 'idle'
  | 'checking'
  | 'login_options'
  | 'logging_in'
  | 'logged_in';

interface UseLoginResult {
  step: LoginStep;
  loading: boolean;
  userInfo: AccountUserInfo | null;
  sessionToken: string | null;
  logout: () => void;
  checkLoginStatus: () => Promise<void>;
}

/**
 * 纯登录 hook - 仅处理登录逻辑，不包含授权
 */
export const useLogin = (): UseLoginResult => {
  const { t } = useTranslation();
  const [step, setStep] = useState<LoginStep>('idle');
  const [loading, setLoading] = useState(false);

  const { userInfo, sessionToken, setUserInfo, setSessionToken, clearUser } =
    useUserStore();

  const logout = useCallback(() => {
    clearUser();
    AuthRepository.clearToken(CURRENT_ENV);
    setStep('login_options');
  }, [clearUser]);

  const fetchUserInfo = useCallback(
    async (token: string): Promise<boolean> => {
      try {
        const res = await authApi.getUserInfo(token);
        if (res.success && res.data) {
          const accountData = res.data as AccountUserInfo;
          setUserInfo(accountData);
          setSessionToken(token);
          setStep('logged_in');
          return true;
        } else if (res.code === 10403) {
          logout();
          return false;
        } else {
          setStep('login_options');
          return false;
        }
      } catch {
        setStep('login_options');
        return false;
      }
    },
    [logout, setUserInfo, setSessionToken],
  );

  const checkSession = useCallback(
    async (token: string): Promise<boolean> => {
      setStep('checking');
      try {
        const res = await authApi.checkSession(token);
        if (res.success && res.data === 'online') {
          await AuthRepository.saveToken(CURRENT_ENV, token);
          return await fetchUserInfo(token);
        } else if (res.code === 10403) {
          logout();
          return false;
        } else {
          setStep('login_options');
          return false;
        }
      } catch {
        setStep('login_options');
        return false;
      }
    },
    [fetchUserInfo, logout],
  );

  const checkLoginStatus = useCallback(async () => {
    // 如果已有 sessionToken 且 userInfo，直接标记为已登录
    if (sessionToken && userInfo) {
      setStep('logged_in');
      return;
    }

    // 尝试从本地存储获取 token
    const savedToken = await AuthRepository.getToken(CURRENT_ENV);
    if (savedToken) {
      await checkSession(savedToken);
    } else if (sessionToken) {
      await checkSession(sessionToken);
    } else {
      setStep('login_options');
    }
  }, [sessionToken, userInfo, checkSession]);

  // 监听原生登录结果
  useEffect(() => {
    const handleNativeLogin = async (loginPromise: Promise<any>) => {
      setLoading(true);
      setStep('logging_in');
      try {
        const loginRes = await loginPromise;
        setLoading(false);
        if (loginRes.success && loginRes.data) {
          await checkSession(loginRes.data);
        } else {
          setStep('login_options');
          Alert.alert(t('actr.auth.loginFailed'), loginRes.message || t('common.unknownError'));
        }
      } catch {
        setLoading(false);
        setStep('login_options');
        Alert.alert(t('common.error'), t('actr.auth.loginFailed'));
      }
    };

    const sub = UserInfoEvents.addListener('onLoginResult', async (res) => {
      if (res.error) {
        setLoading(false);
        setStep('login_options');
        Alert.alert(t('actr.auth.loginError'), res.message || t('common.unknownError'));
        return;
      }

      if (res.type === 'wechat' && res.code) {
        await handleNativeLogin(authApi.loginWithWechat(res.code));
      } else if (res.type === 'apple' && res.id_token) {
        await handleNativeLogin(authApi.loginWithApple(res.id_token));
      }
    });

    return () => sub.remove();
  }, [checkSession]);

  return {
    step,
    loading,
    userInfo,
    sessionToken,
    logout,
    checkLoginStatus,
  };
};
