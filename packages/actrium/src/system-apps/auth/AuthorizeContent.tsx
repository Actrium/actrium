import React from 'react';
import {
  View,
  Text,
  Image,
  StyleSheet,
  TouchableOpacity,
  ActivityIndicator,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { Icon } from '../../components';
import { useColors } from '../../theme/useColors';
import type { ColorTokens } from '../../theme/colors';

type ThemeVariant = 'light' | 'dark';


interface AuthorizeContentProps {
  variant?: ThemeVariant;
  appLogo?: string;
  appName?: string;
  loading?: boolean;
  onAuthorize: () => void;
  onReject: () => void;
  fontSize?: {
    title?: number;
    appName?: number;
    button?: number;
  };
  padding?: number;
  maxWidth?: number;
  /** Whether to show the application information line (%1)logo + (name) header Display as false */
  showAppInfo?: boolean;
}

export const AuthorizeContent = ({
  variant = 'light',
  appLogo,
  appName,
  loading = false,
  onAuthorize,
  onReject,
  fontSize = {},
  padding = 24,
  maxWidth = 400,
  showAppInfo = true,
}: AuthorizeContentProps) => {
  const { t } = useTranslation();
  const sysColors = useColors();
  const colors = {
    title: sysColors.authTitle,
    tip: sysColors.authTip,
    appName: sysColors.authAppName,
    loadingIndicator: sysColors.authLoadingIndicator,
    loadingText: sysColors.authLoadingText,
    authButtonBg: sysColors.authPrimaryButtonBg,
    authButtonText: sysColors.authPrimaryButtonText,
    rejectButtonBg: sysColors.authRejectButtonBg,
    rejectButtonBorder: sysColors.authRejectButtonBorder,
    rejectButtonText: sysColors.authRejectButtonText,
  };
  const styles = React.useMemo(() => makeStyles(sysColors), [sysColors]);
  const userInfo = useUserStore((state) => state.userInfo);

  const {
    title: titleFontSize = 17,
    appName: appNameFontSize = 16,
    button: buttonFontSize = 16,
  } = fontSize;

  if (loading) {
    return (
      <View style={styles.loadingContainer}>
        <ActivityIndicator size="large" color={colors.loadingIndicator} />
        <Text style={[styles.loadingText, { color: colors.loadingText }]}>
          {t('actr.auth.authorizing')}
        </Text>
      </View>
    );
  }

  return (
    <View style={[styles.container, { padding, maxWidth }]}>
      {/* App Info */}
      {showAppInfo && (
        <View style={styles.appInfoRow}>
          {appLogo ? (
            <Image source={{ uri: appLogo }} style={styles.appLogo} />
          ) : (
            <View style={[styles.appLogo, styles.appLogoPlaceholder]} />
          )}
          <Text style={[styles.appName, { fontSize: appNameFontSize, color: colors.appName }]}>
            {appName || t('actr.app.officialService')}
          </Text>
        </View>
      )}

      {/* Permission Tip */}
      <Text style={[styles.title, { fontSize: titleFontSize, color: colors.tip, fontWeight: '600' }]}>
        {t('actr.auth.authTip')}
      </Text>
      <Text style={styles.authDetailText}>
        {t('actr.auth.authDetailTip')}
      </Text>

      {userInfo?.profiles?.length ? (
        <View style={styles.profileList}>
          {userInfo.profiles.map((profile, index) => (
            <View
              key={profile.id || profile.cn}
              style={[styles.profileItem, index < userInfo.profiles.length - 1 ? styles.profileItemSpacing : null]}
            >
              <View style={styles.profileLeft}>
                <View style={styles.profileLeftIconBg}>
                  <Icon type='user_item' size={14} disableTint />
                </View>
                <Text style={styles.profileName} numberOfLines={1} ellipsizeMode='tail'>
                  {profile.display_name || (profile as any).email || profile.mail}
                </Text>
              </View>
              <Icon type='user_select' size={18} disableTint />
            </View>
          ))}
        </View>
      ) : null}

      <View style={styles.buttonRow}>
        <TouchableOpacity
          style={[
            styles.rejectButton,
            { backgroundColor: colors.rejectButtonBg, borderColor: colors.rejectButtonBorder },
          ]}
          onPress={onReject}
          activeOpacity={0.7}
        >
          <Text style={[styles.rejectButtonText, { fontSize: buttonFontSize, color: colors.rejectButtonText }]}>
            {t('common.reject')}
          </Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={[styles.authorizeButton, { backgroundColor: colors.authButtonBg }]}
          onPress={onAuthorize}
          activeOpacity={0.8}
        >
          <Text style={[styles.authorizeButtonText, { fontSize: buttonFontSize, color: colors.authButtonText }]}>
            {t('common.allow')}
          </Text>
        </TouchableOpacity>
      </View>
    </View>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  container: {
    width: '100%',
    alignItems: 'center',
  },
  loadingContainer: {
    padding: 40,
    alignItems: 'center',
    justifyContent: 'center',
  },
  loadingText: {
    marginTop: 12,
    fontSize: 14,
  },
  appInfoRow: {
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 16,
    alignSelf: 'flex-start',
  },
  appLogo: {
    width: 44,
    height: 44,
    borderRadius: 10,
    marginRight: 10,
  },
  appLogoPlaceholder: {
    backgroundColor: '#E5E5E5',
  },
  appName: {
    fontWeight: '600',
  },
  title: {
    marginBottom: 0,
    alignSelf: 'flex-start',
  },
  authDetailText: {
    marginTop: 10,
    marginBottom: 34,
    fontSize: 17,
    color: colors.textPrimary,
    alignSelf: 'flex-start',
  },
  profileList: {
    width: '100%',
    marginBottom: 24,
  },
  profileItem: {
    width: '100%',
    height: 52,
    borderRadius: 26,
    backgroundColor: colors.authProfileBg,
    paddingHorizontal: 24,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  profileItemSpacing: {
    marginBottom: 8,
  },
  profileLeft: {
    flexDirection: 'row',
    alignItems: 'center',
    flex: 1,
    marginRight: 10,
  },
  profileLeftIconBg: {
    width: 28,
    height: 28,
    borderRadius: 14,
    backgroundColor: colors.authProfileIconBg,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: 8,
  },
  profileName: {
    flex: 1,
    fontSize: 16,
    color: colors.authProfileName,
  },
  buttonRow: {
    width: '100%',
    flexDirection: 'row',
  },
  authorizeButton: {
    flex: 1,
    height: 48,
    borderRadius: 24,
    justifyContent: 'center',
    alignItems: 'center',
  },
  authorizeButtonText: {
    fontWeight: '500',
  },
  rejectButton: {
    flex: 1,
    height: 48,
    borderRadius: 24,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#FFFFFF',
    borderWidth: 0,
    marginRight: 10,
  },
  rejectButtonText: {
    fontWeight: '500',
  },
});
