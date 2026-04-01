import React, { useEffect, useState } from 'react';
import {
  View,
  ScrollView,
  StyleSheet,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from '../../actr-host/components/Icon';
import { AppText } from '../../components';
import { getTypographyStyle, spacing, controlHeight } from '../../theme/tokens';
import { SettingsBackHeader } from './SettingsBackHeader';
import { AboutPageKey } from './aboutPages';
import { AboutWebViewModal } from './AboutWebViewModal';

export type { AboutPageKey } from './aboutPages';

interface AboutAskAwayProps {
  onBack: () => void;
  isTablet: boolean;
  initialPage?: AboutPageKey;
}


export const AboutAskAway = ({ onBack, isTablet, initialPage }: AboutAskAwayProps) => {
  const { t } = useTranslation();
  const [activePage, setActivePage] = useState<AboutPageKey | null>(null);

  useEffect(() => {
    setActivePage(initialPage || null);
  }, [initialPage]);

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
        <SettingsBackHeader onBack={onBack} title={t('actr.settings.aboutAskAway')} marginBottom={24} />

        <View style={styles.groupCard}>
          <AboutItem
            label={t('actr.settings.aboutAskAway')}
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
