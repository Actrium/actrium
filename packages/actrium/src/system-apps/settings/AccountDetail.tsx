import React from 'react';
import {
  Platform,
  View,
  ScrollView,
  StyleSheet,
  Image,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import LinearGradient from 'react-native-linear-gradient';
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
  const insets = useSafeAreaInsets();
  const userInfo = useUserStore((state) => state.userInfo);
  const isIOS = Platform.OS === 'ios';
  const headerHeight = isIOS ? insets.top + 56 : 0;
  const headerPaddingTop = isIOS ? insets.top + 12 : 0;
  const contentTopPadding = isIOS ? insets.top + 66 + 24 : 60;

  return (
    <View style={styles.container}>
      <ScrollView
        style={styles.content}
        contentContainerStyle={[
          styles.scrollContent,
          // The head can be considered as high as 44 Then the top of the body remains the original visual starting point and does not move up with it.
          { paddingTop: contentTopPadding },
          {
            paddingHorizontal: isTablet ? 40 : 16,
          },
        ]}
        showsVerticalScrollIndicator={false}
        scrollIndicatorInsets={isIOS ? { top: insets.top + 66 } : undefined}
      >
        {!isIOS ? <SettingsBackHeader onBack={onBack} /> : null}

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

      {isIOS ? (
        <LinearGradient
          style={[
            styles.iosHeader,
            styles.headerOverlay,
            { height: headerHeight, paddingTop: headerPaddingTop },
          ]}
          colors={['rgba(0, 0, 0, 1)', 'rgba(0, 0, 0, 0)']}
          start={{ x: 0.5, y: 0.45 }}
          end={{ x: 0.5, y: 1 }}
        >
          <SettingsBackHeader onBack={onBack} marginBottom={0} />
        </LinearGradient>
      ) : null}
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingTop: 60,
    paddingBottom: 40,
  },
  iosHeader: {
    paddingHorizontal: 16,
    justifyContent: 'center',
  },
  headerOverlay: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    zIndex: 10,
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
