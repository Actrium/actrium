import { Linking } from 'react-native';
import { TFunction } from 'i18next';
import { AuthRepository } from '../../repository/AuthRepository';
import { CURRENT_ENV } from '../../api/auth';
import { GlobalAlert } from '../../components';
import { authApi } from '../../api/auth';

export const createFeedbackAction = ({ t }: { t: TFunction }) => {
  return () => {
    const subject = encodeURIComponent(t('actr.settings.feedbackSubject'));
    const mailto = `mailto:support@actrium.example?subject=${subject}`;
    Linking.openURL(mailto);
  };
};

export const createLogoutAction = ({
  t,
  clearUser,
  clearAuthorizationCache,
  onAfterLogout,
}: {
  t: TFunction;
  clearUser: () => void;
  clearAuthorizationCache: () => void;
  onAfterLogout?: () => void;
}) => {
  const doLogout = async () => {
    // 中文注释：退出登录先删本地 token，再清内存登录态，
    // 这样设置页即使重跑恢复逻辑，也读不到 token，不会把用户信息补回来。
    await AuthRepository.clearToken(CURRENT_ENV);
    clearUser();
    clearAuthorizationCache();
    onAfterLogout?.();
  };

  return () => {
    GlobalAlert.alert(t('actr.settings.logout'), '', [
      { text: t('common.cancel'), style: 'cancel' },
      { text: t('common.confirm'), style: 'destructive', onPress: () => { void doLogout(); } },
    ]);
  };
};

export const createDeleteAccountAction = ({
  t,
  sessionToken,
  clearUser,
  clearAuthorizationCache,
  onAfterDelete,
}: {
  t: TFunction;
  sessionToken: string | null;
  clearUser: () => void;
  clearAuthorizationCache: () => void;
  onAfterDelete?: () => void;
}) => {
  const doDelete = async () => {
    if (!sessionToken) return;
    // 中文注释：删号接口仍然使用当前 token 发请求，但本地登录态要先立刻清掉，
    // 这样设置页即使重跑恢复逻辑，也拿不到 token，自然不会把用户信息补回来。
    clearUser();
    clearAuthorizationCache();
    await AuthRepository.clearToken(CURRENT_ENV);

    const res = await authApi.deleteAccount(sessionToken);
    if (!res.success) {
      console.error(res);
    }
    onAfterDelete?.();
  };

  return () => {
    GlobalAlert.alert(t('actr.settings.deleteAccount'), t('actr.settings.deleteAccountConfirm'), [
      { text: t('common.cancel'), style: 'cancel' },
      { text: t('common.confirm'), style: 'destructive', onPress: () => { void doDelete(); } },
    ]);
  };
};
