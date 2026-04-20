import {
  Platform,
  ScrollView,
  StyleSheet,
  Image,
  TouchableOpacity,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Icon } from '../../components';
import { AppText } from '../../components';
import { getTypographyStyle, spacing } from '../../theme/tokens';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { SettingsItem } from './SettingsItem';
import { isAndroid } from '../../utils/device';
import { PhoneNavigationBar } from '../../components/PhoneNavigationBar';

interface SettingsMainProps {
  onAccountPress: () => void;
  onAboutPress: () => void;
  onFeedbackPress: () => void;
  onLoginPress: () => void;
  onLogout: () => void;
  appVersionText: string;
  isTablet: boolean;
}

export const SettingsMain = ({
  onAccountPress,
  onAboutPress,
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
  const isIOSNativeNavigationBar = Platform.OS === 'ios' && !!PhoneNavigationBar;
  const navigationBarHeight = isIOSNativeNavigationBar ? insets.top + 66 : 0;

  return (
    <View
      style={[
        styles.container,
        isAndroid
          ? {
              paddingTop: insets.top + 8,
            }
          : {},
      ]}
    >
      {/* Page Title */}
      {isIOSNativeNavigationBar && PhoneNavigationBar ? (
        <PhoneNavigationBar
          title='AskAway'
          showLargeTitle={true}
          style={[styles.headerOverlay, { height: navigationBarHeight }]}
        />
      ) : (
        <AppText
          style={[
            styles.pageTitle,
            isAndroid ? { marginTop: 4 } : {},
            Platform.OS === 'android' && styles.androidTitleSpacing,
            { paddingHorizontal: isTablet ? 40 : 16 },
          ]}
        >
          AskAway
        </AppText>
      )}

      <ScrollView
        style={styles.content}
        contentContainerStyle={[
          styles.scrollContent,
          // 中文注释：iOS 原生导航条改为覆盖层后，需要把首屏可读区的顶部留白转移到滚动内容内部。
          { paddingTop: isIOSNativeNavigationBar ? navigationBarHeight + 24 : 0 },
          { paddingHorizontal: isTablet ? 40 : 16 },
        ]}
        showsVerticalScrollIndicator={false}
        scrollIndicatorInsets={
          isIOSNativeNavigationBar ? { top: navigationBarHeight } : undefined
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
              ellipsizeMode='tail'
            >
              {isLoggedIn
                ? userInfo.user.display_name || t('actr.settings.account')
                : t('actr.auth.login')}
            </AppText>
          </View>
          <Icon type='next' size={16} color='#fff' />
        </TouchableOpacity>

        {/* Settings Groups */}
        <View style={styles.groupCard}>
          <SettingsItem
            icon='feedback'
            label={t('actr.settings.feedback')}
            onPress={onFeedbackPress}
          />
          <View style={styles.itemDivider} />
          <SettingsItem
            icon='askaway_gery'
            label={t('actr.settings.aboutAskAway')}
            value={appVersionText}
            onPress={onAboutPress}
          />
        </View>

        {/* Logout Section - 仅登录后显示 */}
        {isLoggedIn && (
          <View style={styles.logoutCard}>
            <SettingsItem
              icon='logout'
              iconColor='#FF383C'
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
  pageTitle: {
    ...getTypographyStyle('pageTitle'),
    color: '#FFFFFF',
    marginBottom: 24,
  },
  androidTitleSpacing: {
    // 中文注释：仅 Android 追加字间距，对齐原生宿主标题的观感。
    letterSpacing: 0.3,
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
