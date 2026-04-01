import React from 'react';
import {
  View,
  ScrollView,
  StyleSheet,
  Image,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../components';
import { getTypographyStyle, spacing, controlHeight } from '../../theme/tokens';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { SettingsBackHeader } from './SettingsBackHeader';

interface AccountDetailProps {
  onBack: () => void;
  onDeleteAccount: () => void;
  onLogout: () => void;
  isTablet: boolean;
}

export const AccountDetail = ({
  onBack,
  onDeleteAccount,
  onLogout,
  isTablet,
}: AccountDetailProps) => {
  const { t } = useTranslation();
  const userInfo = useUserStore((state) => state.userInfo);

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
      <SettingsBackHeader onBack={onBack} />

      {/* Avatar */}
      <View style={styles.avatarContainer}>
        <Image
          source={
            userInfo?.user.avatar && userInfo.user.avatar !== 'none'
              ? { uri: userInfo.user.avatar }
              : require('../../assets/login/avatar_default.png')
          }
          style={styles.largeAvatar}
        />
      </View>

      {/* Account Info Card */}
      <View style={styles.detailCard}>
        <View style={styles.detailRow}>
          <AppText style={styles.detailLabel}>{t('actr.settings.nickname')}</AppText>
          <AppText style={styles.detailValue} numberOfLines={1} ellipsizeMode="tail">
            {userInfo?.user.display_name || userInfo?.user.cn || '-'}
          </AppText>
        </View>
        <View style={styles.divider} />
        <View style={styles.detailRow}>
          <AppText style={styles.detailLabel}>{t('actr.settings.email')}</AppText>
          <AppText style={styles.detailValue} numberOfLines={1} ellipsizeMode="tail">
            {userInfo?.user.mail || '-'}
          </AppText>
        </View>
      </View>

      {/* Danger Zone */}
      <View style={styles.dangerCard}>
        <View style={styles.dangerRow}>
          <TouchableOpacity
            style={styles.dangerButton}
            onPress={onDeleteAccount}
            activeOpacity={0.7}
          >
            <AppText style={styles.destructiveText}>
              {t('actr.settings.deleteAccount')}
            </AppText>
          </TouchableOpacity>
        </View>
        <View style={styles.divider} />
        <View style={styles.dangerRow}>
          <TouchableOpacity
            style={styles.dangerButton}
            onPress={onLogout}
            activeOpacity={0.7}
          >
            <AppText style={styles.destructiveText}>
              {t('actr.settings.logoutAccount')}
            </AppText>
          </TouchableOpacity>
        </View>
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
  avatarContainer: {
    alignItems: 'center',
    marginBottom: spacing.xxxl,
  },
  largeAvatar: {
    width: 120,
    height: 120,
    borderRadius: 60,
    backgroundColor: '#2C2C2E',
  },
  detailCard: {
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    overflow: 'hidden',
    marginBottom: 18,
  },
  detailRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: spacing.xl,
    height: controlHeight.xl,
  },
  detailLabel: {
    ...getTypographyStyle('title3'),
    color: '#FFFFFF',
    fontWeight: '400',
  },
  detailValue: {
    ...getTypographyStyle('title3'),
    color: '#8C8C8C',
    fontWeight: '400',
    flex: 1,
    textAlign: 'right',
    marginLeft: spacing.lg,
  },
  divider: {
    height: 1,
    backgroundColor: '#2C2C2E',
  },
  dangerCard: {
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    overflow: 'hidden',
    marginBottom: 18,
  },
  dangerRow: {
    paddingHorizontal: spacing.xl,
    height: controlHeight.xl,
    justifyContent: 'center',
  },
  dangerButton: {
    flex: 1,
    justifyContent: 'center',
  },
  destructiveText: {
    ...getTypographyStyle('title3'),
    color: '#FF684E',
  },
});
