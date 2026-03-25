import React, { useEffect, useMemo, useState } from 'react';
import {
  View,
  Text,
  ScrollView,
  StyleSheet,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { WebView } from 'react-native-webview';
import { Icon } from '../../actr-host/components/Icon';
import { SettingsBackHeader } from './SettingsBackHeader';

interface AboutAskAwayProps {
  onBack: () => void;
  isTablet: boolean;
  initialPage?: AboutPageKey;
}

export type AboutPageKey = 'aboutApp' | 'userAgreement' | 'privacyPolicy' | 'icp';

const ABOUT_PAGE_CONFIG: Record<AboutPageKey, { path: string }> = {
  aboutApp: { path: 'AskAway.html' },
  userAgreement: { path: 'user.html' },
  privacyPolicy: { path: 'privacy.html' },
  icp: { path: 'icp.html' },
};

const getLanguageSegment = (language: string): 'zh-Hans' | 'en' => {
  return language.toLowerCase().startsWith('zh') ? 'zh-Hans' : 'en';
};

const getAboutPageUrl = (page: AboutPageKey, language: string): string => {
  const languageSegment = getLanguageSegment(language);
  return `https://www.askaway.chat/corp/${languageSegment}/${ABOUT_PAGE_CONFIG[page].path}`;
};

export const AboutAskAway = ({ onBack, isTablet, initialPage }: AboutAskAwayProps) => {
  const { t, i18n } = useTranslation();
  const { top: topInset, bottom: bottomInset } = useSafeAreaInsets();
  const [currentPage, setCurrentPage] = useState<AboutPageKey | null>(null);

  useEffect(() => {
    setCurrentPage(initialPage || null);
  }, [initialPage]);

  const pageUrl = useMemo(() => {
    if (!currentPage) return '';
    return getAboutPageUrl(currentPage, i18n.language || 'zh-Hans');
  }, [currentPage, i18n.language]);

  if (currentPage && pageUrl) {
    return (
      <View style={[styles.webviewContainer, { paddingBottom: bottomInset }]}>
        <View
          style={[
            styles.webviewHeader,
            {
              height: topInset + 44,
              paddingTop: topInset,
            },
          ]}
        >
          <SettingsBackHeader onBack={() => setCurrentPage(null)} marginBottom={0} />
        </View>
        <WebView
          source={{ uri: pageUrl }}
          style={styles.webview}
          forceDarkOn={false}
          contentInsetAdjustmentBehavior='never'
        />
      </View>
    );
  }

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
      <SettingsBackHeader onBack={onBack} title={t('actr.settings.aboutAskAway')} marginBottom={24} />

      <View style={styles.groupCard}>
        <AboutItem
          label={t('actr.settings.aboutAskAway')}
          onPress={() => setCurrentPage('aboutApp')}
        />
        <View style={styles.divider} />
        <AboutItem
          label={t('actr.settings.userAgreement')}
          onPress={() => setCurrentPage('userAgreement')}
        />
        <View style={styles.divider} />
        <AboutItem
          label={t('actr.settings.privacyPolicy')}
          onPress={() => setCurrentPage('privacyPolicy')}
        />
        <View style={styles.divider} />
        <AboutItem
          label={t('actr.settings.icp')}
          onPress={() => setCurrentPage('icp')}
        />
      </View>
    </ScrollView>
  );
};

interface AboutItemProps {
  label: string;
  onPress: () => void;
}

const AboutItem = ({ label, onPress }: AboutItemProps) => {
  return (
    <TouchableOpacity style={styles.itemContainer} activeOpacity={0.7} onPress={onPress}>
      <Text style={styles.itemLabel}>{label}</Text>
      <Icon type='next' size={16} color='#fff' />
    </TouchableOpacity>
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
  webviewContainer: {
    flex: 1,
    backgroundColor: '#FFFFFF',
  },
  webviewHeader: {
    backgroundColor: '#FFFFFF',
    justifyContent: 'center',
    paddingHorizontal: 16,
  },
  webview: {
    flex: 1,
    backgroundColor: '#FFFFFF',
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
    paddingHorizontal: 20,
    height: 52,
  },
  itemLabel: {
    fontSize: 17,
    color: '#FFFFFF',
    fontWeight: '400',
  },
  divider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: 20,
    marginRight: 12,
  },
});
