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
  const doLogout = () => {
    clearUser();
    clearAuthorizationCache();
    AuthRepository.clearToken(CURRENT_ENV);
    onAfterLogout?.();
  };

  return () => {
    GlobalAlert.alert(t('actr.settings.logout'), '', [
      { text: t('common.cancel'), style: 'cancel' },
      { text: t('common.confirm'), style: 'destructive', onPress: doLogout },
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
    const res = await authApi.deleteAccount(sessionToken);
    if (res.success) {
      clearUser();
      clearAuthorizationCache();
      AuthRepository.clearToken(CURRENT_ENV);
    } else {
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
