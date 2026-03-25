import React from 'react';
import {
  View,
  Text,
  ScrollView,
  StyleSheet,
  Image,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from '../../actr-host/components/Icon';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { SettingsItem } from './SettingsItem';

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
  const userInfo = useUserStore((state) => state.userInfo);
  const isLoggedIn = !!userInfo;

  return (
    <ScrollView
      style={styles.content}
      contentContainerStyle={[
        styles.scrollContent,
        {
          paddingHorizontal: isTablet ? 40 : 16,
        },
      ]}
      showsVerticalScrollIndicator={false}
    >
      {/* Page Title */}
      <Text style={styles.pageTitle}>AskAway</Text>

      {/* Account Section */}
      <TouchableOpacity
        style={styles.accountCard}
        activeOpacity={0.8}
        onPress={isLoggedIn ? onAccountPress : onLoginPress}
      >
        <Image
          source={
            isLoggedIn && userInfo.user.avatar && userInfo.user.avatar !== 'none'
              ? { uri: userInfo.user.avatar }
              : require('../../assets/login/avatar_default.png')
          }
          style={styles.avatar}
        />
        <View style={styles.accountInfo}>
          <Text style={styles.accountName} numberOfLines={1} ellipsizeMode="tail">
            {isLoggedIn ? (userInfo.user.display_name || t('actr.settings.account')) : t('actr.auth.login')}
          </Text>
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
  );
};

const styles = StyleSheet.create({
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingTop: 60,
    paddingBottom: 40,
  },
  pageTitle: {
    fontSize: 34,
    color: '#FFFFFF',
    marginBottom: 24,
    marginTop: 8
  },
  accountCard: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    paddingHorizontal: 20,
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
    marginLeft: 16,
  },
  accountName: {
    fontSize: 20,
    fontWeight: '600',
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
    marginRight: 12,
  },
});
