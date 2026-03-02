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
import { useUserStore } from '../../actr-host/store/useUserStore';

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
          paddingHorizontal: isTablet ? 40 : 24,
        },
      ]}
      showsVerticalScrollIndicator={false}
    >
      {/* Back Button */}
      <TouchableOpacity
        style={styles.backButton}
        onPress={onBack}
        activeOpacity={0.7}
      >
        <Text style={styles.backButtonText}>‹</Text>
      </TouchableOpacity>

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
          <Text style={styles.detailLabel}>{t('actr.settings.nickname')}</Text>
          <Text style={styles.detailValue} numberOfLines={1} ellipsizeMode="tail">
            {userInfo?.user.display_name || userInfo?.user.cn || '-'}
          </Text>
        </View>
        <View style={styles.divider} />
        <View style={styles.detailRow}>
          <Text style={styles.detailLabel}>{t('actr.settings.email')}</Text>
          <Text style={styles.detailValue} numberOfLines={1} ellipsizeMode="tail">
            {userInfo?.user.mail || '-'}
          </Text>
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
            <Text style={styles.destructiveText}>
              {t('actr.settings.deleteAccount')}
            </Text>
          </TouchableOpacity>
        </View>
        <View style={styles.divider} />
        <View style={styles.dangerRow}>
          <TouchableOpacity
            style={styles.dangerButton}
            onPress={onLogout}
            activeOpacity={0.7}
          >
            <Text style={styles.destructiveText}>
              {t('actr.settings.logoutAccount')}
            </Text>
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
  backButton: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: '#3a3a3c',
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 32,
  },
  backButtonText: {
    fontSize: 32,
    color: '#fff',
    fontWeight: '300',
    marginTop: -4,
  },
  avatarContainer: {
    alignItems: 'center',
    marginBottom: 32,
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
    paddingHorizontal: 20,
    height: 52,
  },
  detailLabel: {
    fontSize: 17,
    color: '#FFFFFF',
    fontWeight: '400',
  },
  detailValue: {
    fontSize: 17,
    color: '#8C8C8C',
    fontWeight: '400',
    flex: 1,
    textAlign: 'right',
    marginLeft: 16,
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
    paddingHorizontal: 20,
    height: 52,
    justifyContent: 'center',
  },
  dangerButton: {
    flex: 1,
    justifyContent: 'center',
  },
  destructiveText: {
    fontSize: 17,
    color: '#FF684E',
  },
});
