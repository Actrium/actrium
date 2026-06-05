import React from 'react';
import {
  Platform,
  ScrollView,
  StyleSheet,
  Switch,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import LinearGradient from 'react-native-linear-gradient';
import { AppText } from '../../components';
import { getTypographyStyle, spacing, controlHeight } from '../../theme/tokens';
import { SettingsBackHeader } from './SettingsBackHeader';
import { useSettingsStore } from '../../store/useSettingsStore';
import type { NotificationKey, NotificationSettings } from '../../store/useSettingsStore';

interface NotificationSettingsProps {
  onBack: () => void;
  isTablet: boolean;
}

interface NotificationItem {
  key: NotificationKey;
  labelKey: string;
}

const NOTIFICATION_ITEMS: NotificationItem[] = [
  { key: 'task', labelKey: 'actr.settings.notificationTask' },
  // { key: 'accountSecurity', labelKey: 'actr.settings.notificationAccountSecurity' },
  // { key: 'systemUpdate', labelKey: 'actr.settings.notificationSystemUpdate' },
  // { key: 'operation', labelKey: 'actr.settings.notificationOperation' },
];

export const NotificationSettingsPage = ({
  onBack,
  isTablet,
}: NotificationSettingsProps) => {
  const { t } = useTranslation();
  const insets = useSafeAreaInsets();
  const notification = useSettingsStore((state) => state.settings.notification);
  const updateNotificationSetting = useSettingsStore(
    (state) => state.updateNotificationSetting,
  );
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
          { paddingTop: contentTopPadding },
          { paddingHorizontal: isTablet ? 40 : 16 },
        ]}
        showsVerticalScrollIndicator={false}
        scrollIndicatorInsets={isIOS ? { top: insets.top + 66 } : undefined}
      >
        {!isIOS ? (
          <SettingsBackHeader
            onBack={onBack}
            title={t('actr.settings.notification')}
          />
        ) : null}

        <View style={styles.card}>
          {NOTIFICATION_ITEMS.map((item, index) => (
            <React.Fragment key={item.key}>
              {index > 0 && <View style={styles.divider} />}
              <View style={styles.row}>
                <AppText style={styles.label}>
                  {t(item.labelKey as any)}
                </AppText>
                <Switch
                  value={notification[item.key as keyof NotificationSettings]}
                  onValueChange={(value: boolean) =>
                    updateNotificationSetting(item.key, value)
                  }
                  trackColor={{ false: '#3A3A3C', true: '#34C759' }}
                />
              </View>
            </React.Fragment>
          ))}
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
          <SettingsBackHeader
            onBack={onBack}
            title={t('actr.settings.notification')}
            marginBottom={0}
          />
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
  card: {
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    overflow: 'hidden',
    marginBottom: 18,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: spacing.xl,
    height: controlHeight.xl,
  },
  label: {
    ...getTypographyStyle('title3'),
    color: '#FFFFFF',
    fontWeight: '400',
    flex: 1,
  },
  divider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: spacing.xl,
    marginRight: spacing.xl,
  },
});