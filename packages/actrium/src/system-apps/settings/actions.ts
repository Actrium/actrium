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
    const aboutActriumLabel = t('actr.settings.aboutActrium').trim();
    const subjectText = `#${feedbackLabel} ${aboutActriumLabel} #`;

    if (Platform.OS === 'android') {
      // Android Prefer to primary Intent The message is that the message is not a message.
      // Avoid Linking.openURL(mailto) Let's go. ACTION_VIEW is not consistent with the mail client.
      // If raw module is not available or equipment is not available App, and downgraded. mailto link.
      try {
        await EmailBridge.composeEmail({
          to: ['support@actrium.example'],
          subject: subjectText,
        });
        return;
      } catch (e) {
        console.error(e)
        // Downgrade to mailto
      }
    }

    const subject = encodeURIComponent(subjectText);
    const mailto = `mailto:support@actrium.example?subject=${subject}`;
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
    // Delete Local token, re-check memory login,
    // Even if we start running again, we can't read it. token, the user information will not be returned.
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
    // Deleting interface still uses the current token The request is made, but the local login is cleared immediately.
    // Even if we rerun the logic, we can't get it. token, naturally does not replace the user information.
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
