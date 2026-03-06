import { Linking } from 'react-native';
import { TFunction } from 'i18next';
import { AuthRepository } from '../../repository/AuthRepository';
import { CURRENT_ENV } from '../../api/auth';
import { GlobalAlert } from '../../components';

export const createFeedbackAction = () => {
  return () => {
    const subject = encodeURIComponent('#吐个槽关于');
    const mailto = `mailto:support@askaway.chat?subject=${subject}`;
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
