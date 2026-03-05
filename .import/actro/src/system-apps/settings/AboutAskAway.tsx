import React, { useEffect, useMemo, useState } from 'react';
import {
  View,
  Text,
  ScrollView,
  StyleSheet,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { WebView } from 'react-native-webview';
import { Icon } from '../../actr-host/components/Icon';
import { SettingsBackHeader } from './SettingsBackHeader';

interface AboutAskAwayProps {
  onBack: () => void;
  isTablet: boolean;
  initialPage?: AboutPageKey;
}

export type AboutPageKey = 'aboutApp' | 'userAgreement' | 'privacyPolicy' | 'icp';

const ABOUT_PAGE_CONFIG: Record<AboutPageKey, { label: string; path: string }> = {
  aboutApp: { label: '关于 AskAway', path: 'AskAway.html' },
  userAgreement: { label: '用户协议', path: 'user.html' },
  privacyPolicy: { label: '隐私政策', path: 'privacy.html' },
  icp: { label: 'ICP 备案', path: 'icp.html' },
};

const getLanguageSegment = (language: string): 'zh-Hans' | 'en' => {
  return language.toLowerCase().startsWith('zh') ? 'zh-Hans' : 'en';
};

const getAboutPageUrl = (page: AboutPageKey, language: string): string => {
  const languageSegment = getLanguageSegment(language);
  return `https://www.actrium.example/corp/${languageSegment}/${ABOUT_PAGE_CONFIG[page].path}`;
};

export const AboutAskAway = ({ onBack, isTablet, initialPage }: AboutAskAwayProps) => {
  const { i18n } = useTranslation();
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
      <View style={styles.webviewContainer}>
        <WebView source={{ uri: pageUrl }} style={styles.webview} forceDarkOn={false} />
        <TouchableOpacity
          style={styles.floatingBackButton}
          onPress={() => setCurrentPage(null)}
          activeOpacity={0.7}
        >
          <Text style={styles.floatingBackButtonText}>‹</Text>
        </TouchableOpacity>
      </View>
    );
  }

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
      <SettingsBackHeader onBack={onBack} title='关于 AskAway' marginBottom={24} />

      <View style={styles.groupCard}>
        <AboutItem
          label={ABOUT_PAGE_CONFIG.aboutApp.label}
          onPress={() => setCurrentPage('aboutApp')}
        />
        <View style={styles.divider} />
        <AboutItem
          label={ABOUT_PAGE_CONFIG.userAgreement.label}
          onPress={() => setCurrentPage('userAgreement')}
        />
        <View style={styles.divider} />
        <AboutItem
          label={ABOUT_PAGE_CONFIG.privacyPolicy.label}
          onPress={() => setCurrentPage('privacyPolicy')}
        />
        <View style={styles.divider} />
        <AboutItem
          label={ABOUT_PAGE_CONFIG.icp.label}
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
  webview: {
    flex: 1,
    backgroundColor: '#FFFFFF',
  },
  floatingBackButton: {
    position: 'absolute',
    top: 56,
    left: 16,
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: 'rgba(58, 58, 60, 0.88)',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 10,
  },
  floatingBackButtonText: {
    fontSize: 30,
    color: '#fff',
    fontWeight: '300',
    marginTop: -4,
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
