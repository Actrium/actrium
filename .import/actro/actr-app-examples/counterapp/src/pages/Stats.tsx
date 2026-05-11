import { useEventBridge, useHttp } from 'askit';
import { useEffect, useState } from 'react';
import {
  ScrollView,
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  Image,
} from 'rill/let';
import { HostToGuestEventPayloads } from 'askit/contracts';

const clearIconSource = require('../../assets/clear.png');
const feedbackIconSource = require('../../assets/feedback.png');
const aboutIconSource = require('../../assets/about.png');

interface StatsProps {}

interface LanguageOption {
  id: string;
  label: string;
  backgroundColor: string;
  color: string;
}

interface ActionOption {
  id: string;
  iconSource: any;
  label: string;
  handler?: () => void;
}

const supportedLanguages: LanguageOption[] = [
  {
    id: 'en',
    label: 'Welcome',
    backgroundColor: '#F4ECEA',
    color: '#D95B4F',
  },
  {
    id: 'zhHans',
    label: '欢迎',
    backgroundColor: '#F5EDED',
    color: '#E45C4E',
  },
  {
    id: 'ja',
    label: 'いらっしゃいませ',
    backgroundColor: '#ECF3F8',
    color: '#2E7B8B',
  },
  {
    id: 'fr',
    label: 'accfaueillir',
    backgroundColor: '#F8EEF4',
    color: '#E16586',
  },
  {
    id: 'ko',
    label: '환영',
    backgroundColor: '#EEF2FF',
    color: '#005DFF',
  },
  {
    id: 'zhHant',
    label: '歡迎',
    backgroundColor: '#F8F0F0',
    color: '#D95B4F',
  },
  {
    id: 'es',
    label: 'bienvenido',
    backgroundColor: '#F8F5F2',
    color: '#DB9432',
  },
  {
    id: 'ru',
    label: 'добропожаловать',
    backgroundColor: '#F0EEF7',
    color: '#6B5BAA',
  },
];

function onLanguageChangePlaceholder(language: LanguageOption) {
  void language;
}

export default function Stats({}: StatsProps) {
  const [lang, setLang] = useState<string>('zhHans' as any);
  const [statsData, setStatsData] = useState<
    HostToGuestEventPayloads['SEND_APP_INFO']
  >({} as any);
  const { request: getAppInfo } = useEventBridge(
    'GET_APP_INFO',
    'SEND_APP_INFO',
  );
  const { request: clearChatHistory } = useEventBridge(
    'CLEAR_CHAT_HISTORY',
    'CLEAR_CHAT_HISTORY_RESULT',
  );

  const handleClearHistory = () => {
    clearChatHistory({
      requestId: 'stats_page_clear_history_001',
    }).then((result) => {
      if (result.success) {
        console.log('聊天记录已清除');
      }
    });
  };

  const actions = [
    {
      id: 'clearHistory',
      iconSource: clearIconSource,
      label: '清除对话记录',
      handler: handleClearHistory,
    },
    { id: 'feedback', iconSource: feedbackIconSource, label: '吐个槽' },
    { id: 'about', iconSource: aboutIconSource, label: '关于 MU AI' },
  ] satisfies ActionOption[];

  useEffect(() => {
    getAppInfo({
      requestId: 'stats_page_001',
    }).then((appInfo) => {
      setStatsData(appInfo);
    });
  }, []);

  const handleLanguagePress = (language: LanguageOption) => {
    setStatsData((current) => ({
      ...current,
      selectedLanguageId: language.id,
    }));
    onLanguageChangePlaceholder(language);
    setLang(language.id);
  };

  return (
    <View style={styles.guestContainer}>
      <ScrollView
        style={styles.container}
        contentContainerStyle={styles.contentContainer}
        id="sidebar"
      >
        <View style={styles.profileSection}>
          <View style={styles.logoBox}>
            <Image source={{ uri: statsData.logo }} style={styles.logo} />
          </View>

          <Text style={styles.title}>{statsData.appName}</Text>

          <View style={styles.statsRow}>
            <Text style={styles.metricValue}>{statsData.favoriteCount}</Text>
            <Text style={styles.metricLabel}>收藏</Text>
            <Text style={[styles.metricValue, styles.metricGap]}>
              {statsData.usedCount}
            </Text>
            <Text style={styles.metricLabel}>用过</Text>
          </View>

          <Text style={styles.creator}>
            创建者
            {statsData.author}
          </Text>

          <TouchableOpacity style={styles.shareButton} onPress={() => {}}>
            <Text style={styles.shareIcon}>↗</Text>
            <Text style={styles.shareText}>分享</Text>
          </TouchableOpacity>
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>介绍</Text>
          <Text style={styles.aboutText}>123</Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>选择语种</Text>
          <View style={styles.languageGrid}>
            {supportedLanguages.map((language) => {
              const isSelected = language.id === lang;

              return (
                <TouchableOpacity
                  key={language.id}
                  onPress={() => handleLanguagePress(language)}
                  style={[
                    styles.languagePill,
                    {
                      backgroundColor: isSelected
                        ? '#E35C49'
                        : language.backgroundColor,
                    },
                  ]}
                >
                  <Text
                    style={[
                      styles.languageText,
                      { color: isSelected ? '#FFFFFF' : language.color },
                    ]}
                  >
                    {language.label}
                  </Text>
                </TouchableOpacity>
              );
            })}
          </View>
        </View>

        <View style={styles.actionList}>
          {actions.map((action) => (
            <TouchableOpacity
              key={action.id}
              onPress={action.handler || (() => {})}
              style={styles.actionItem}
            >
              <Image
                source={action.iconSource}
                style={styles.actionIcon}
              />
              <Text style={styles.actionText}>{action.label}</Text>
            </TouchableOpacity>
          ))}
        </View>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  guestContainer: {
    flex: 1,
    width: '100%',
  },
  container: {
    flex: 1,
    width: '100%',
  },
  contentContainer: {
    paddingTop: 30,
    paddingRight: 12,
    paddingBottom: 28,
    paddingLeft: 20,
  },
  profileSection: {
    alignItems: 'center',
    marginBottom: 52,
  },
  logoBox: {
    borderRadius: 33,
    // backgroundColor: '#C5924F',
    overflow: 'hidden',
    marginBottom: 24,
  },
  logo: {
    width: 66,
    height: 66,
  },
  title: {
    color: '#252525',
    fontSize: 18,
    fontWeight: '500',
    lineHeight: 28,
    marginBottom: 8,
  },
  statsRow: {
    flexDirection: 'row',
    alignItems: 'baseline',
    justifyContent: 'center',
    marginBottom: 10,
  },
  metricValue: {
    color: '#2A2A2A',
    fontSize: 17,
    fontWeight: '500',
    lineHeight: 24,
  },
  metricLabel: {
    color: '#A7A7A7',
    fontSize: 16,
    lineHeight: 22,
    marginLeft: 6,
  },
  metricGap: {
    marginLeft: 28,
  },
  creator: {
    color: '#9E9E9E',
    fontSize: 14,
    lineHeight: 22,
    marginBottom: 19,
  },
  shareButton: {
    height: 42,
    paddingHorizontal: 24,
    borderRadius: 14,
    backgroundColor: '#34A8FB',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
  },
  shareIcon: {
    color: '#FFFFFF',
    fontSize: 27,
    fontWeight: '700',
    marginRight: 8,
    marginTop: 2,
  },
  shareText: {
    color: '#FFFFFF',
    fontSize: 18,
    fontWeight: '600',
  },
  section: {
    marginBottom: 40,
  },
  sectionTitle: {
    color: '#252525',
    fontSize: 18,
    fontWeight: '500',
    lineHeight: 28,
    marginBottom: 9,
  },
  aboutText: {
    color: '#9E9E9E',
    fontSize: 16,
    lineHeight: 24,
  },
  languageGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    alignItems: 'flex-start',
  },
  languagePill: {
    borderRadius: 12,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: 24,
    paddingVertical: 8,
    marginRight: 16,
    marginBottom: 16,
  },
  languageText: {
    fontSize: 18,
    fontWeight: '700',
    lineHeight: 25,
  },
  actionList: {
    marginTop: -16,
  },
  actionItem: {
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 21,
  },
  actionIcon: {
    width: 20,
    height: 20,
    marginRight: 18,
  },
  actionText: {
    color: '#202124',
    fontSize: 18,
    lineHeight: 25,
  },
});
