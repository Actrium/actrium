import { GlobalAlert, useEventBridge } from 'askit';
import { useEffect, useState } from 'react';
import {
  ScrollView,
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  Image,
} from 'keel/guest';
import { HostToGuestEventPayloads } from 'askit/contracts';
import { welcomeLabel as welcomeLabelEn } from '../locales/en/stats';
import { welcomeLabel as welcomeLabelEs } from '../locales/es/stats';
import { welcomeLabel as welcomeLabelFr } from '../locales/fr/stats';
import { welcomeLabel as welcomeLabelJa } from '../locales/ja/stats';
import { welcomeLabel as welcomeLabelKo } from '../locales/ko/stats';
import { welcomeLabel as welcomeLabelRu } from '../locales/ru/stats';
import { welcomeLabel as welcomeLabelZhHans } from '../locales/zh-Hans/stats';
import { welcomeLabel as welcomeLabelZhHant } from '../locales/zh-Hant/stats';

const clearIconSource = require('../../assets/clear.png');
const feedbackIconSource = require('../../assets/feedback.png');
const aboutIconSource = require('../../assets/about.png');
const shareIconSource = require('../../assets/share.png');

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
  iconAspectRatio: number;
  label: string;
  handler?: () => void;
}

const supportedLanguages: LanguageOption[] = [
  {
    id: 'en',
    label: welcomeLabelEn,
    backgroundColor: '#F4ECEA',
    color: '#D95B4F',
  },
  {
    id: 'zh-Hans',
    label: welcomeLabelZhHans,
    backgroundColor: '#F5EDED',
    color: '#E45C4E',
  },
  {
    id: 'ja',
    label: welcomeLabelJa,
    backgroundColor: '#ECF3F8',
    color: '#2E7B8B',
  },
  {
    id: 'fr',
    label: welcomeLabelFr,
    backgroundColor: '#F8EEF4',
    color: '#E16586',
  },
  {
    id: 'ko',
    label: welcomeLabelKo,
    backgroundColor: '#EEF2FF',
    color: '#005DFF',
  },
  {
    id: 'zh-Hant',
    label: welcomeLabelZhHant,
    backgroundColor: '#F8F0F0',
    color: '#D95B4F',
  },
  {
    id: 'es',
    label: welcomeLabelEs,
    backgroundColor: '#F8F5F2',
    color: '#DB9432',
  },
  {
    id: 'ru',
    label: welcomeLabelRu,
    backgroundColor: '#F0EEF7',
    color: '#6B5BAA',
  },
];

export default function Stats({}: StatsProps) {
  const [lang, setLang] = useState<string>('zhHans' as any);
  const [languages, setLanguages] = useState<LanguageOption[]>([]);
  const [statsData, setStatsData] = useState<
    HostToGuestEventPayloads['SEND_APP_INFO']
  >({} as any);
  const [showClearAlert, setShowClearAlert] = useState(false);
  const { request: getAppInfo } = useEventBridge(
    'GET_APP_INFO',
    'SEND_APP_INFO',
  );
  const { request: clearChatHistory } = useEventBridge(
    'CLEAR_CHAT_HISTORY',
    'CLEAR_CHAT_HISTORY_RESULT',
  );
  const { request: closeExtension } = useEventBridge(
    'CLOSE_EXTENSION',
    'CLOSE_EXTENSION_RESULT',
  );
  const { request: getLanguages } = useEventBridge(
    'GET_LANGUAGE_LIST',
    'LANGUAGE_LIST',
  );
  const { request: setAppLanguage } = useEventBridge(
    'SET_APP_LANGUAGE',
    'SET_APP_LANGUAGE_RESULT',
  );
  const { request: sendEmail } = useEventBridge(
    'SEND_EMAIL',
    'SEND_EMAIL_RESULT',
  );

  const handleClearHistory = () => {
    setShowClearAlert(true);
  };

  const handleConfirmClear = () => {
    clearChatHistory({
      requestId: 'stats_page_clear_history_001',
    }).then((result) => {
      if (result.success) {
        setShowClearAlert(false);
        closeExtension({
          requestId: 'stats_page_close_extension_001',
        });
      }
    });
  };

  useEffect(() => {
    getAppInfo({
      requestId: 'stats_page_001',
    }).then((appInfo) => {
      setStatsData(appInfo);
    });

    getLanguages({
      requestId: 'stats_page_002',
    }).then((result) => {
      const languages = result.languages as string[];

      setLanguages(
        supportedLanguages.filter(({ id }) => languages.includes(id)),
      );
      setLang(result.current);
    });
  }, []);

  const handleLanguagePress = (language: LanguageOption) => {
    setStatsData((current) => ({
      ...current,
      selectedLanguageId: language.id,
    }));

    setLang(language.id);
    setAppLanguage({
      requestId: 'set-language',
      language: language.id,
    });
  };

  const handleFeedback = () => {
    sendEmail({
      requestId: 'send',
      to: 'support@actrium.chat',
      subject: 'actr feedback'
    });
  };

  const actions = [
    {
      id: 'clearHistory',
      iconSource: clearIconSource,
      iconAspectRatio: 56 / 70,
      label: 'Clear Dialogue Record',
      handler: handleClearHistory,
    },
    {
      id: 'feedback',
      iconSource: feedbackIconSource,
      iconAspectRatio: 1,
      label: 'Spit it out.',
      handler: handleFeedback,
    },
    // {
    //   id: 'about',
    //   iconSource: aboutIconSource,
    //   iconAspectRatio: 1,
    //   label: 'About MU AI',
    // },
  ] satisfies ActionOption[];

  return (
    <View style={styles.guestContainer}>
      <GlobalAlert
        visible={showClearAlert}
        title="Clear Dialogue Record"
        message="Determines that all records of the dialogue are cleared? This cannot be restored."
        buttons={[
          {
            text: 'Cancel',
            style: 'cancel',
            onPress: () => setShowClearAlert(false),
          },
          { text: 'Clear', style: 'destructive', onPress: handleConfirmClear },
        ]}
        onDismiss={() => setShowClearAlert(false)}
      />
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

          {/* <View style={styles.statsRow}>
            <Text style={styles.metricValue}>{statsData.favoriteCount}</Text>
            <Text style={styles.metricLabel}>Collection</Text>
            <Text style={[styles.metricValue, styles.metricGap]}>
              {statsData.usedCount}
            </Text>
            <Text style={styles.metricLabel}>Yes.</Text>
          </View> */}

          {/* <Text style={styles.creator}>
            Creator
            {statsData.author}
          </Text>

          <TouchableOpacity style={styles.shareButton} onPress={() => {}}>
            <Image
              source={shareIconSource}
              style={[styles.shareIcon, { aspectRatio: 1 }]}
            />

            <Text style={styles.shareText}>Share</Text>
          </TouchableOpacity> */}
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Introduction</Text>
          <Text style={styles.aboutText}>
            {statsData.languageContents?.[lang]?.description}
          </Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Select Language</Text>
          <View style={styles.languageGrid}>
            {languages.map((language) => {
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
                style={[
                  styles.actionIcon,
                  { aspectRatio: action.iconAspectRatio },
                ]}
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
    paddingHorizontal: 18,
    borderRadius: 14,
    backgroundColor: '#34A8FB',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
  },
  shareIcon: {
    color: '#FFFFFF',
    tintColor: '#fff',
    marginRight: 6,
    height: 16,
    resizeMode: 'contain',
    paddingHorizontal: 8,
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
    // Limit the height of the icon and automatically calculate the width of each resource by combining the width ratio
    height: 17,
    marginRight: 13,
    resizeMode: 'contain',
  },
  actionText: {
    color: '#202124',
    fontSize: 18,
    lineHeight: 25,
  },
});
