import { Linking, Platform } from 'react-native';
import { TFunction } from 'i18next';
import { AuthRepository } from '../../repository/AuthRepository';
import { CURRENT_ENV } from '../../api/auth';
import { GlobalAlert } from '../../components';
import { authApi } from '../../api/auth';
import { EmailBridge } from '../../native-bridge';

export const createFeedbackAction = ({ t }: { t: TFunction }) => {
  return async () => {
    const feedbackLabel = t('actr.settings.feedback').trim();
    const aboutActroLabel = t('actr.settings.aboutActro').trim();
    const subjectText = `#${feedbackLabel} ${aboutActroLabel} #`;

    if (Platform.OS === 'android') {
      // Android 优先用原生 Intent 传递主题和收件人，
      // 避免 Linking.openURL(mailto) 走 ACTION_VIEW 时被不同邮件客户端不一致地解析。
      // 若原生模块不可用或设备无邮件 App，则降级走 mailto 链接。
      try {
        await EmailBridge.composeEmail({
          to: ['support@actro.chat'],
          subject: subjectText,
        });
        return;
      } catch (e) {
        console.error(e)
        // 降级到 mailto
      }
    }

    const subject = encodeURIComponent(subjectText);
    const mailto = `mailto:support@actro.chat?subject=${subject}`;
    await Linking.openURL(mailto);
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
    // 退出登录先删本地 token，再清内存登录态，
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
    // 删号接口仍然使用当前 token 发请求，但本地登录态要先立刻清掉，
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
