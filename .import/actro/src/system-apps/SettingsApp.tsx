/**
 * Settings System App - Built-in settings page
 */
import React, { useMemo } from 'react';
import {
  View,
  Text,
  ScrollView,
  StyleSheet,
  Image,
  useWindowDimensions,
  Platform,
  Linking,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { MyTouchableOpacity } from 'askit';

const AskAwayLogo = require('../assets/login/login_logo.png');

interface SettingsItemProps {
  icon: string | any;
  iconBgColor?: string;
  label: string;
  value?: string;
  showChevron?: boolean;
  onPress?: () => void;
  destructive?: boolean;
}

const SettingsItem = ({
  icon,
  iconBgColor = '#3A3A3C',
  label,
  value,
  showChevron = true,
  onPress,
  destructive = false,
}: SettingsItemProps) => {
  return (
    <MyTouchableOpacity
      style={styles.itemContainer}
      onPress={onPress}
      activeOpacity={0.7}
    >
      <View style={[styles.itemIconWrapper, { backgroundColor: iconBgColor }]}>
        {typeof icon === 'string' ? (
          <Text style={styles.itemIconText}>{icon}</Text>
        ) : (
          <Image source={icon} style={styles.itemIconImage} />
        )}
      </View>
      <View style={styles.itemContent}>
        <Text style={[styles.itemLabel, destructive && styles.destructiveText]}>
          {label}
        </Text>
        {value && <Text style={styles.itemValue}>{value}</Text>}
        {showChevron && (
          <Text style={styles.chevron}>{'〉'}</Text>
        )}
      </View>
    </MyTouchableOpacity>
  );
};

export default function SettingsApp() {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const isTablet = width >= 768;

  const handleLogout = () => {
    // TODO: Implement logout logic
    console.log('Logout pressed');
  };

  const handleFeedback = () => {
    const subject = encodeURIComponent('#吐个槽关于');
    const mailto = `mailto:support@askaway.chat?subject=${subject}`;
    Linking.openURL(mailto);
  };

  return (
    <View style={styles.container}>
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
        <Text style={styles.pageTitle}>{t('settings.title', '设置')}</Text>

        {/* Account Section */}
        <MyTouchableOpacity style={styles.accountCard} activeOpacity={0.8}>
          <Image
            source={{ uri: 'https://images.unsplash.com/photo-1599566150163-29194dcaad36?ixlib=rb-4.0.3&auto=format&fit=crop&w=150&q=80' }}
            style={styles.avatar}
          />
          <View style={styles.accountInfo}>
            <Text style={styles.accountName}>Account</Text>
          </View>
          <Text style={styles.chevron}>{'〉'}</Text>
        </MyTouchableOpacity>

        {/* Settings Groups */}
        <View style={styles.groupCard}>
          {/* <SettingsItem
            icon="🌓"
            iconBgColor="#5856D6"
            label={t('settings.appearance', '外观')}
            onPress={() => {}}
          />
          <View style={styles.divider} />
          <SettingsItem
            icon="🌐"
            iconBgColor="#007AFF"
            label={t('settings.language', '语言')}
            onPress={() => {}}
          />
          <View style={styles.divider} />
          <SettingsItem
            icon="🛡️"
            iconBgColor="#34C759"
            label={t('settings.privacy', '隐私与安全')}
            onPress={() => {}}
          />
          <View style={styles.divider} />
          <SettingsItem
            icon="❓"
            iconBgColor="#8E8E93"
            label={t('settings.help', '帮助')}
            onPress={() => {}}
          />
          <View style={styles.divider} /> */}
          <SettingsItem
            icon="💬"
            iconBgColor="#FF9500"
            label={t('settings.feedback', '吐个槽')}
            onPress={handleFeedback}
          />
          <View style={styles.divider} />
          <SettingsItem
            icon={AskAwayLogo}
            iconBgColor="#000000"
            label="AskAway"
            value="V 0.0.0"
            onPress={() => {}}
          />
        </View>

        {/* Logout Section */}
        <View style={styles.logoutCard}>
          <SettingsItem
            icon="🚪"
            iconBgColor="#FF453A"
            label={t('settings.logout', '退出登录')}
            showChevron={false}
            destructive={true}
            onPress={handleLogout}
          />
        </View>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000000',
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingTop: Platform.OS === 'ios' ? 60 : 40,
    paddingBottom: 40,
  },
  pageTitle: {
    fontSize: 40,
    fontWeight: '700',
    color: '#FFFFFF',
    marginBottom: 24,
    marginLeft: 4,
  },
  accountCard: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#1C1C1E',
    borderRadius: 24,
    padding: 16,
    marginBottom: 24,
  },
  avatar: {
    width: 64,
    height: 64,
    borderRadius: 32,
    backgroundColor: '#2C2C2E',
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
    borderRadius: 24,
    overflow: 'hidden',
    marginBottom: 24,
  },
  logoutCard: {
    backgroundColor: '#1C1C1E',
    borderRadius: 24,
    overflow: 'hidden',
    marginBottom: 24,
  },
  itemContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 12,
    paddingLeft: 16,
    height: 60,
  },
  itemIconWrapper: {
    width: 32,
    height: 32,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  itemIconText: {
    fontSize: 18,
  },
  itemIconImage: {
    width: 20,
    height: 20,
    resizeMode: 'contain',
  },
  itemContent: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    marginLeft: 16,
    paddingRight: 4,
  },
  itemLabel: {
    flex: 1,
    fontSize: 17,
    color: '#FFFFFF',
    fontWeight: '400',
  },
  itemValue: {
    fontSize: 17,
    color: '#8E8E93',
    marginRight: 8,
  },
  chevron: {
    fontSize: 16,
    color: '#3A3A3C',
    fontWeight: '600',
  },
  divider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: 64, // Align with text
  },
  destructiveText: {
    color: '#FF453A',
  },
});
