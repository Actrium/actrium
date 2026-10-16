import {
  ScrollView,
  StyleSheet,
  Image,
  Platform,
  StatusBar,
  TouchableOpacity,
  View,
} from 'react-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Icon } from '../../components';
import { AppText } from '../../components';
import { getTypographyStyle, spacing } from '../../theme/tokens';
import { useUserStore } from '../../runtime-host/store/useUserStore';
import { SettingsItem } from './SettingsItem';
import { PhoneNavigationBar } from '../../components/PhoneNavigationBar';
import {
  connectBrowserSyncLocalFolder,
  getBrowserSyncProviderStatus,
  getBrowserSyncStatus,
  hasBrowserSyncModule,
  hasBrowserSyncProviderModule,
  syncBrowserDataNow,
  type BrowserSyncProviderStatus,
  type BrowserSyncStatus,
} from '../../native-bridge/browser-data/BrowserDataModule';

interface SettingsMainProps {
  onAccountPress: () => void;
  onAboutPress: () => void;
  onNotificationPress: () => void;
  onFeedbackPress: () => void;
  onLoginPress: () => void;
  onLogout: () => void;
  appVersionText: string;
  isTablet: boolean;
}

export const SettingsMain = ({
  onAccountPress,
  onAboutPress,
  onNotificationPress,
  onFeedbackPress,
  onLoginPress,
  onLogout,
  appVersionText,
  isTablet,
}: SettingsMainProps) => {
  const { t } = useTranslation();
  const insets = useSafeAreaInsets();
  const userInfo = useUserStore((state) => state.userInfo);
  const isLoggedIn = !!userInfo;
  const hasNativeNavigationBar = !!PhoneNavigationBar;
  const isBrowserSyncAvailable = hasBrowserSyncModule();
  const isBrowserSyncProviderAvailable = hasBrowserSyncProviderModule();
  const [browserSyncStatus, setBrowserSyncStatus] =
    useState<BrowserSyncStatus | null>(null);
  const [isBrowserSyncRunning, setIsBrowserSyncRunning] = useState(false);
  const [browserSyncProviderStatus, setBrowserSyncProviderStatus] =
    useState<BrowserSyncProviderStatus | null>(null);
  const [isConnectingBrowserSyncFolder, setIsConnectingBrowserSyncFolder] =
    useState(false);
  // Android pad Go, go, go! insets.top Yes 0, the header of the original navigation column is blocked by the system status bar,
  // Use StatusBar.currentHeight Compensation.Android phone and iOS Not affected.
  const isAndroidPad = Platform.OS === 'android' && isTablet;
  const statusBarHeight = isAndroidPad ? (StatusBar.currentHeight ?? 0) : 0;
  const navigationBarHeight = hasNativeNavigationBar
    ? (isAndroidPad ? statusBarHeight : insets.top) + 66
    : 0;
  const loadBrowserSyncStatus = useCallback(async () => {
    if (!isBrowserSyncAvailable) {
      setBrowserSyncStatus(null);
      return;
    }

    setBrowserSyncStatus(await getBrowserSyncStatus());
  }, [isBrowserSyncAvailable]);
  const browserSyncStatusText = useMemo(() => {
    if (isBrowserSyncRunning) {
      return t('actr.settings.browserSyncRunning');
    }

    if (!browserSyncStatus) {
      return t('actr.settings.browserSyncUnavailable');
    }

    switch (browserSyncStatus.status) {
      case 'idle':
        return t('actr.settings.browserSyncIdle');
      case 'pending':
        return t('actr.settings.browserSyncPending');
      case 'retryScheduled':
        return browserSyncStatus.nextRunDelaySeconds
          ? t('actr.settings.browserSyncRetryScheduledIn', {
              seconds: browserSyncStatus.nextRunDelaySeconds,
            })
          : t('actr.settings.browserSyncRetryScheduled');
      case 'waitingForCredentials':
        return t('actr.settings.browserSyncWaitingForCredentials');
      case 'requiresUserAction':
        if (browserSyncStatus.lastClassification === 'clientUpgradeRequired') {
          return t('actr.settings.browserSyncUpgradeRequired');
        }
        return t('actr.settings.browserSyncRequiresUserAction');
      default:
        return t('actr.settings.browserSyncUnavailable');
    }
  }, [browserSyncStatus, isBrowserSyncRunning, t]);

  useEffect(() => {
    void loadBrowserSyncStatus();
  }, [loadBrowserSyncStatus]);

  const handleBrowserSyncNow = useCallback(async () => {
    if (!isBrowserSyncAvailable || isBrowserSyncRunning) {
      return;
    }

    setIsBrowserSyncRunning(true);
    try {
      await syncBrowserDataNow();
    } finally {
      await loadBrowserSyncStatus();
      setIsBrowserSyncRunning(false);
    }
  }, [isBrowserSyncAvailable, isBrowserSyncRunning, loadBrowserSyncStatus]);

  const loadBrowserSyncProviderStatus = useCallback(async () => {
    if (!isBrowserSyncProviderAvailable) {
      setBrowserSyncProviderStatus(null);
      return;
    }

    setBrowserSyncProviderStatus(await getBrowserSyncProviderStatus());
  }, [isBrowserSyncProviderAvailable]);

  useEffect(() => {
    void loadBrowserSyncProviderStatus();
  }, [loadBrowserSyncProviderStatus]);

  const browserSyncFolderValueText = useMemo(() => {
    if (isConnectingBrowserSyncFolder) {
      return t('actr.settings.browserSyncConnecting');
    }
    if (browserSyncProviderStatus?.configured) {
      return t('actr.settings.browserSyncFolderConnected');
    }
    return t('actr.settings.browserSyncFolderConnect');
  }, [browserSyncProviderStatus, isConnectingBrowserSyncFolder, t]);

  // Minimal folder entry: the native side owns the folder path, the
  // provider-settings persistence, and the Rust preflight gate; here we only
  // trigger the connect and refresh the displayed provider + sync status.
  const handleConnectBrowserSyncFolder = useCallback(async () => {
    if (!isBrowserSyncProviderAvailable || isConnectingBrowserSyncFolder) {
      return;
    }

    setIsConnectingBrowserSyncFolder(true);
    try {
      setBrowserSyncProviderStatus(await connectBrowserSyncLocalFolder());
    } finally {
      await loadBrowserSyncStatus();
      setIsConnectingBrowserSyncFolder(false);
    }
  }, [
    isBrowserSyncProviderAvailable,
    isConnectingBrowserSyncFolder,
    loadBrowserSyncStatus,
  ]);

  return (
    <View style={styles.container}>
      {/* Page Title */}
      {hasNativeNavigationBar ? (
        <PhoneNavigationBar
          title="Actrium"
          showLargeTitle={true}
          style={[
            styles.headerOverlay,
            { height: navigationBarHeight },
            // Android pad The upper native navigation bar requires a top-state bar space to be released under the status bar.
            isAndroidPad ? { paddingTop: statusBarHeight } : {},
          ]}
        />
      ) : null}

      <ScrollView
        style={styles.content}
        contentContainerStyle={[
          styles.scrollContent,
          // iOS When the original navigator is replaced with a laminate, the top of the readable area of the first screen needs to be moved to the inside of the scrolling content.
          { paddingTop: hasNativeNavigationBar ? navigationBarHeight + 24 : 0 },
          { paddingHorizontal: isTablet ? 40 : 16 },
        ]}
        showsVerticalScrollIndicator={false}
        scrollIndicatorInsets={
          hasNativeNavigationBar ? { top: navigationBarHeight } : undefined
        }
      >
        {/* Account Section */}
        <TouchableOpacity
          style={styles.accountCard}
          activeOpacity={0.8}
          onPress={isLoggedIn ? onAccountPress : onLoginPress}
        >
          <Image
            source={
              isLoggedIn &&
              userInfo.user.avatar &&
              userInfo.user.avatar !== 'none'
                ? { uri: userInfo.user.avatar }
                : require('../../assets/login/avatar_default.png')
            }
            style={styles.avatar}
          />
          <View style={styles.accountInfo}>
            <AppText
              style={styles.accountName}
              numberOfLines={1}
              ellipsizeMode="tail"
            >
              {isLoggedIn
                ? userInfo.user.display_name || t('actr.settings.account')
                : t('actr.auth.login')}
            </AppText>
          </View>
          <Icon type="next" size={16} color="#fff" />
        </TouchableOpacity>

        {/* Settings Groups */}
        <View style={styles.groupCard}>
          <SettingsItem
            icon="feedback"
            label={t('actr.settings.feedback')}
            onPress={onFeedbackPress}
          />
          <View style={styles.itemDivider} />
          <SettingsItem
            icon="actrium_gray"
            label={t('actr.settings.aboutActrium')}
            value={appVersionText}
            onPress={onAboutPress}
          />
          <View style={styles.itemDivider} />
          <SettingsItem
            icon="feedback"
            label={t('actr.settings.notification')}
            onPress={onNotificationPress}
          />
        </View>

        {isBrowserSyncAvailable && (
          <View style={styles.groupCard}>
            <SettingsItem
              icon="actrium_gray"
              label={t('actr.settings.browserSync')}
              value={browserSyncStatusText}
              showChevron={false}
              onPress={loadBrowserSyncStatus}
            />
            {isBrowserSyncProviderAvailable && (
              <>
                <View style={styles.itemDivider} />
                <SettingsItem
                  icon="feedback"
                  label={t('actr.settings.browserSyncFolder')}
                  value={browserSyncFolderValueText}
                  showChevron={false}
                  onPress={
                    isConnectingBrowserSyncFolder
                      ? undefined
                      : handleConnectBrowserSyncFolder
                  }
                />
              </>
            )}
            <View style={styles.itemDivider} />
            <SettingsItem
              icon="feedback"
              label={t('actr.settings.browserSyncNow')}
              value={
                isBrowserSyncRunning
                  ? t('actr.settings.browserSyncRunning')
                  : undefined
              }
              showChevron={false}
              onPress={isBrowserSyncRunning ? undefined : handleBrowserSyncNow}
            />
          </View>
        )}

        {/* Logout Section - Show only after login */}
        {isLoggedIn && (
          <View style={styles.logoutCard}>
            <SettingsItem
              icon="logout"
              iconColor="#FF383C"
              label={t('actr.settings.logout')}
              showChevron={false}
              destructive={true}
              onPress={onLogout}
            />
          </View>
        )}
      </ScrollView>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  headerOverlay: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    width: '100%',
    zIndex: 10,
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingBottom: 40,
  },
  accountCard: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    paddingHorizontal: spacing.xl,
    paddingVertical: 10,
    marginBottom: 18,
  },
  avatar: {
    width: 60,
    height: 60,
    borderRadius: 32,
    backgroundColor: '#2C2C2E',
    overflow: 'hidden',
  },
  accountInfo: {
    flex: 1,
    marginLeft: spacing.lg,
    marginRight: spacing.xl,
  },
  accountName: {
    ...getTypographyStyle('title2'),
    color: '#FFFFFF',
  },
  groupCard: {
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    overflow: 'hidden',
    marginBottom: 18,
  },
  logoutCard: {
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    overflow: 'hidden',
    marginBottom: 18,
  },
  itemDivider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: 55,
    marginRight: spacing.md,
  },
});
