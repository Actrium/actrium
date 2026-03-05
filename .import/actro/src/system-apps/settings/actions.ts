import { Linking } from 'react-native';
import { TFunction } from 'i18next';
import { AuthRepository } from '../../repository/AuthRepository';
import { CURRENT_ENV } from '../../api/auth';
import { GlobalAlert } from '../../components';

export const createFeedbackAction = () => {
  return () => {
    const subject = encodeURIComponent('#吐个槽关于');
    const mailto = `mailto:support@actrium.example?subject=${subject}`;
    Linking.openURL(mailto);
  };
};

export const createLogoutAction = ({
  t,
  clearUser,
  onAfterLogout,
}: {
  t: TFunction;
  clearUser: () => void;
  onAfterLogout?: () => void;
}) => {
  const doLogout = () => {
    clearUser();
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
