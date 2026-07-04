import React, { useEffect, useState } from 'react';
import {
  Platform,
  View,
  ScrollView,
  StyleSheet,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import LinearGradient from 'react-native-linear-gradient';
import { Icon } from '../../components';
import { AppText } from '../../components';
import { getTypographyStyle, spacing, controlHeight } from '../../theme/tokens';
import { SettingsBackHeader } from './SettingsBackHeader';
import { AboutPageKey } from './aboutPages';
import { AboutWebViewModal } from './AboutWebViewModal';

export type { AboutPageKey } from './aboutPages';

interface AboutActroProps {
  onBack: () => void;
  isTablet: boolean;
  initialPage?: AboutPageKey;
}


export const AboutActro = ({ onBack, isTablet, initialPage }: AboutActroProps) => {
  const { t } = useTranslation();
  const insets = useSafeAreaInsets();
  const [activePage, setActivePage] = useState<AboutPageKey | null>(null);
  const isIOS = Platform.OS === 'ios';
  const headerHeight = isIOS ? insets.top + 56 : 0;
  const headerPaddingTop = isIOS ? insets.top + 12 : 0;
  const contentTopPadding = isIOS ? insets.top + 66 + 24 : 60;

  useEffect(() => {
    setActivePage(initialPage || null);
  }, [initialPage]);

  return (
    <View style={styles.container}>
      <ScrollView
        style={styles.content}
        contentContainerStyle={[
          styles.scrollContent,
          // 头部可视高度调整为 44 后，正文顶部仍保持原来的视觉起点，不跟着一起上移。
          { paddingTop: contentTopPadding },
          {
            paddingHorizontal: isTablet ? 40 : 16,
          },
        ]}
        showsVerticalScrollIndicator={false}
        scrollIndicatorInsets={isIOS ? { top: insets.top + 66 } : undefined}
      >
        {!isIOS ? (
          <SettingsBackHeader
            onBack={onBack}
            title={t('actr.settings.aboutActro')}
            marginBottom={24}
          />
        ) : null}

        <View style={styles.groupCard}>
          <AboutItem
            label={t('actr.settings.aboutActro')}
            onPress={() => setActivePage('aboutApp')}
          />
          <View style={styles.divider} />
          <AboutItem
            label={t('actr.settings.userAgreement')}
            onPress={() => setActivePage('userAgreement')}
          />
          <View style={styles.divider} />
          <AboutItem
            label={t('actr.settings.privacyPolicy')}
            onPress={() => setActivePage('privacyPolicy')}
          />
          <View style={styles.divider} />
          <AboutItem
            label={t('actr.settings.icp')}
            onPress={() => setActivePage('icp')}
          />
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
            title={t('actr.settings.aboutActro')}
            marginBottom={0}
          />
        </LinearGradient>
      ) : null}

      <AboutWebViewModal
        visible={!!activePage}
        page={activePage}
        onClose={() => setActivePage(null)}
      />
    </View>
  );
};

interface AboutItemProps {
  label: string;
  onPress: () => void;
}

const AboutItem = ({ label, onPress }: AboutItemProps) => {
  return (
    <TouchableOpacity style={styles.itemContainer} activeOpacity={0.7} onPress={onPress}>
      <AppText style={styles.itemLabel}>{label}</AppText>
      <Icon type='next' size={16} color='#fff' />
    </TouchableOpacity>
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
  groupCard: {
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    overflow: 'hidden',
  },
  itemContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: spacing.xl,
    height: controlHeight.xl,
  },
  itemLabel: {
    ...getTypographyStyle('title3'),
    color: '#FFFFFF',
    fontWeight: '400',
  },
  divider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: spacing.xl,
    marginRight: spacing.md,
  },
});
