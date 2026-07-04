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
import { useUserStore } from '../../actr-host/store/useUserStore';
import { SettingsItem } from './SettingsItem';
import { PhoneNavigationBar } from '../../components/PhoneNavigationBar';
import {
  getBrowserSyncStatus,
  hasBrowserSyncModule,
  syncBrowserDataNow,
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
  const [browserSyncStatus, setBrowserSyncStatus] =
    useState<BrowserSyncStatus | null>(null);
  const [isBrowserSyncRunning, setIsBrowserSyncRunning] = useState(false);
  // Android pad 上 insets.top 为 0，原生导航栏标题被系统状态栏遮挡，
  // 用 StatusBar.currentHeight 补偿。Android phone 和 iOS 不受影响。
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

  return (
    <View style={styles.container}>
      {/* Page Title */}
      {hasNativeNavigationBar ? (
        <PhoneNavigationBar
          title="Actro"
          showLargeTitle={true}
          style={[
            styles.headerOverlay,
            { height: navigationBarHeight },
            // Android pad 上原生导航栏需要顶部留出状态栏空间，让标题渲染在状态栏下方。
            isAndroidPad ? { paddingTop: statusBarHeight } : {},
          ]}
        />
      ) : null}

      <ScrollView
        style={styles.content}
        contentContainerStyle={[
          styles.scrollContent,
          // iOS 原生导航条改为覆盖层后，需要把首屏可读区的顶部留白转移到滚动内容内部。
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
            icon="actro_gray"
            label={t('actr.settings.aboutActro')}
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
              icon="actro_gray"
              label={t('actr.settings.browserSync')}
              value={browserSyncStatusText}
              showChevron={false}
              onPress={loadBrowserSyncStatus}
            />
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

        {/* Logout Section - 仅登录后显示 */}
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
