import { useState, useEffect, useCallback } from 'react';
import { Alert } from 'react-native';
import { authApi, CURRENT_ENV } from '../../api/auth';
import { AuthRepository } from '../../repository/AuthRepository';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { useAuthTaskStore } from '../../actr-host/store/useAuthTaskStore';
import { AccountUserInfo } from '../../api/types';
import { useTranslation } from 'react-i18next';
import { LoginBridge, LOGIN_RESULT_KEYS, LOGIN_TYPES } from '../../native-bridge';
import { GlobalAlert } from '../../components';

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
 * Pure Login hook - Only handle login logic, not including authorization
 */
export const useLogin = (): UseLoginResult => {
  const { t } = useTranslation();
  const [step, setStep] = useState<LoginStep>('idle');
  const [loading, setLoading] = useState(false);

  const { userInfo, sessionToken, setUserInfo, setSessionToken, clearUser } =
    useUserStore();
  const clearAuthorizationCache = useAuthTaskStore(
    (state) => state.clearAuthorizationCache,
  );

  const logout = useCallback(() => {
    clearUser();
    clearAuthorizationCache();
    AuthRepository.clearToken(CURRENT_ENV);
    setStep('login_options');
  }, [clearAuthorizationCache, clearUser]);

  const fetchUserInfo = useCallback(
    async (token: string): Promise<boolean> => {
      try {
        const res = await authApi.getUserInfo(token);
        if (res.success && res.data) {
          const accountData = res.data as AccountUserInfo;
          const prevUserId = userInfo?.user?.cn || '';
          const nextUserId = accountData.user?.cn || '';
          if (prevUserId && nextUserId && prevUserId !== nextUserId) {
            // Clear the authorized cache when user switches to avoid reusing the old account authorization status
            clearAuthorizationCache();
          }
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
    [clearAuthorizationCache, logout, setUserInfo, setSessionToken, userInfo],
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
    // If there is one, sessionToken and userInfo, directly marked as login
    if (sessionToken && userInfo) {
      setStep('logged_in');
      return;
    }

    // Try to retrieve from local storage token
    const savedToken = await AuthRepository.getToken(CURRENT_ENV);
    if (savedToken) {
      await checkSession(savedToken);
    } else if (sessionToken) {
      await checkSession(sessionToken);
    } else {
      setStep('login_options');
    }
  }, [sessionToken, userInfo, checkSession]);

  // Intercept original login results
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
          // Watch out for the raw bullet windows here.
          Alert.alert(t('actr.auth.loginFailed'), loginRes.message || t('common.unknownError'));
        }
      } catch {
        setLoading(false);
        setStep('login_options'); 
        // Watch out for the raw bullet windows here.
        Alert.alert(t('common.error'), t('actr.auth.loginFailed'));
      }
    };

    const unsubscribe = LoginBridge.onLoginResult(async (res) => {
      if (res[LOGIN_RESULT_KEYS.error]) {
        setLoading(false);
        setStep('login_options');
        // Watch out for the raw bullet windows here.
        Alert.alert(t('actr.auth.loginError'), res[LOGIN_RESULT_KEYS.message] || t('common.unknownError'));
        return;
      }

      const loginType = res[LOGIN_RESULT_KEYS.type];
      const code = res[LOGIN_RESULT_KEYS.code];
      const idToken = res[LOGIN_RESULT_KEYS.idToken];

      if (loginType === LOGIN_TYPES.wechat && code) {
        await handleNativeLogin(authApi.loginWithWechat(code));
      } else if (loginType === LOGIN_TYPES.apple && idToken) {
        await handleNativeLogin(authApi.loginWithApple(idToken));
      } else if (loginType === LOGIN_TYPES.google && idToken) {
        await handleNativeLogin(authApi.loginWithGoogle(idToken));
      }
    });

    return () => unsubscribe();
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
