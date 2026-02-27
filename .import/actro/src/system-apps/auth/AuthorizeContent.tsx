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

type ThemeVariant = 'light' | 'dark';

const themeColors = {
  light: {
    title: '#333',
    tip: '#666',
    appName: '#333',
    loadingIndicator: '#1C69FF',
    loadingText: '#666',
    authButtonBg: '#1C69FF',
    authButtonText: '#fff',
    rejectButtonBg: '#F5F5F5',
    rejectButtonBorder: '#E5E5E5',
    rejectButtonText: '#333',
  },
  dark: {
    title: '#fff',
    tip: '#ccc',
    appName: '#fff',
    loadingIndicator: '#fff',
    loadingText: '#fff',
    authButtonBg: '#1C69FF',
    authButtonText: '#fff',
    rejectButtonBg: 'rgba(255, 255, 255, 0.15)',
    rejectButtonBorder: 'rgba(255, 255, 255, 0.1)',
    rejectButtonText: '#fff',
  },
};

interface AuthorizeContentProps {
  variant?: ThemeVariant;
  appLogo?: string;
  appName?: string;
  loading?: boolean;
  onAuthorize: () => void;
  onReject?: () => void;
  fontSize?: {
    title?: number;
    appName?: number;
    button?: number;
  };
  padding?: number;
  maxWidth?: number;
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
}: AuthorizeContentProps) => {
  const { t } = useTranslation();
  const colors = themeColors[variant];

  const {
    title: titleFontSize = 18,
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

      {/* Permission Tip */}
      <Text style={[styles.title, { fontSize: titleFontSize, color: colors.tip }]}>
        {t('actr.auth.authTip')}
      </Text>

      {/* Authorize Button */}
      <TouchableOpacity
        style={[styles.authorizeButton, { backgroundColor: colors.authButtonBg }]}
        onPress={onAuthorize}
        activeOpacity={0.8}
      >
        <Text style={[styles.authorizeButtonText, { fontSize: buttonFontSize, color: colors.authButtonText }]}>
          {t('actr.auth.agreeAuth')}
        </Text>
      </TouchableOpacity>

      {/* Reject Button */}
      {onReject && (
        <TouchableOpacity
          style={[
            styles.rejectButton,
            { backgroundColor: colors.rejectButtonBg, borderColor: colors.rejectButtonBorder },
          ]}
          onPress={onReject}
          activeOpacity={0.7}
        >
          <Text style={[styles.rejectButtonText, { fontSize: buttonFontSize, color: colors.rejectButtonText }]}>
            {t('common.cancel')}
          </Text>
        </TouchableOpacity>
      )}
    </View>
  );
};

const styles = StyleSheet.create({
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
    width: 40,
    height: 40,
    borderRadius: 8,
    marginRight: 12,
  },
  appLogoPlaceholder: {
    backgroundColor: '#E5E5E5',
  },
  appName: {
    fontWeight: '600',
  },
  title: {
    marginBottom: 24,
    alignSelf: 'flex-start',
  },
  authorizeButton: {
    width: '100%',
    height: 48,
    borderRadius: 24,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 12,
  },
  authorizeButtonText: {
    fontWeight: '600',
  },
  rejectButton: {
    width: '100%',
    height: 48,
    borderRadius: 24,
    justifyContent: 'center',
    alignItems: 'center',
    borderWidth: 1,
  },
  rejectButtonText: {
    fontWeight: '500',
  },
});
