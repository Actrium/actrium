import React from 'react';
import {
  View,
  Text,
  ScrollView,
  StyleSheet,
  Image,
  TouchableOpacity,
  Linking,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from '../../actr-host/components/Icon';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { SettingsItem } from './SettingsItem';

interface SettingsMainProps {
  onAccountPress: () => void;
  onLoginPress: () => void;
  onLogout: () => void;
  isTablet: boolean;
}

export const SettingsMain = ({
  onAccountPress,
  onLoginPress,
  onLogout,
  isTablet,
}: SettingsMainProps) => {
  const { t } = useTranslation();
  const userInfo = useUserStore((state) => state.userInfo);
  const isLoggedIn = !!userInfo;

  const handleFeedback = () => {
    const subject = encodeURIComponent('#吐个槽关于');
    const mailto = `mailto:support@actrium.example?subject=${subject}`;
    Linking.openURL(mailto);
  };

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
      <Text style={styles.pageTitle}>{t('actr.settings.title')}</Text>

      {/* Account Section */}
      <TouchableOpacity
        style={styles.accountCard}
        activeOpacity={0.8}
        onPress={isLoggedIn ? onAccountPress : onLoginPress}
      >
        {isLoggedIn ? (
          <Image
            source={{
              uri: userInfo.user.avatar || 'https://images.unsplash.com/photo-1599566150163-29194dcaad36?ixlib=rb-4.0.3&auto=format&fit=crop&w=150&q=80',
            }}
            style={styles.avatar}
          />
        ) : (
          <Image
            source={require('../../assets/login/avatar_default.png')}
            style={styles.avatar}
          />
        )}
        <View style={styles.accountInfo}>
          <Text style={styles.accountName}>
            {isLoggedIn ? (userInfo.user.display_name || 'Account') : t('actr.auth.login')}
          </Text>
        </View>
        <Icon type='next' size={16} color='#fff' />
      </TouchableOpacity>

      {/* Settings Groups */}
      <View style={styles.groupCard}>
        <SettingsItem
          icon='feedback'
          label={t('actr.settings.feedback')}
          onPress={handleFeedback}
        />
      </View>

      {/* Logout Section */}
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
    fontWeight: '600',
    color: '#FFFFFF',
    marginBottom: 24,
    marginLeft: 4,
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
});
