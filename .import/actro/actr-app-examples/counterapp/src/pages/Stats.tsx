import { useState } from 'react';
import { ScrollView, View, Text, TouchableOpacity, StyleSheet } from 'rill/let';

interface StatsProps {}

interface LanguageOption {
  id: string;
  label: string;
  backgroundColor: string;
  color: string;
}

interface ActionOption {
  id: string;
  icon: string;
  label: string;
  filledIcon?: boolean;
}

const initialStatsData = {
  title: '慕田峪长城智能问答',
  favoriteCount: '1234',
  favoriteLabel: '收藏',
  usedCount: '12345678',
  usedLabel: '用过',
  creatorPrefix: '创建者：',
  creatorName: '慕田峪长城景区官方',
  shareLabel: '分享',
  introTitle: '介绍',
  intro:
    '我是Mu Ai，知无不答！依托前沿人工智能，融合个性化服务，为您定制特色行程，高效解答问题，全程相伴畅玩景区。',
  languageTitle: '选择语种',
  selectedLanguageId: 'en',
  supportedLanguages: [
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
  ],
  actions: [
    { id: 'clearHistory', icon: '⌫', label: '清除对话记录' },
    { id: 'feedback', icon: '☹', label: '吐个槽' },
    { id: 'about', icon: '!', label: '关于 MU AI', filledIcon: true },
  ],
};

function onLanguageChangePlaceholder(language: LanguageOption) {
  void language;
}

export default function Stats({}: StatsProps) {
  const [statsData, setStatsData] = useState(initialStatsData);

  const handleLanguagePress = (language: LanguageOption) => {
    setStatsData((current) => ({
      ...current,
      selectedLanguageId: language.id,
    }));
    onLanguageChangePlaceholder(language);
  };

  return (
    <View style={styles.guestContainer}>
      <ScrollView
        style={styles.container}
        contentContainerStyle={styles.contentContainer}
        id='sidebar'
      >
        <View style={styles.profileSection}>
          <View style={styles.logo}>
          </View>

          <Text style={styles.title}>{statsData.title}</Text>

          <View style={styles.statsRow}>
            <Text style={styles.metricValue}>{statsData.favoriteCount}</Text>
            <Text style={styles.metricLabel}>{statsData.favoriteLabel}</Text>
            <Text style={[styles.metricValue, styles.metricGap]}>
              {statsData.usedCount}
            </Text>
            <Text style={styles.metricLabel}>{statsData.usedLabel}</Text>
          </View>

          <Text style={styles.creator}>
            {statsData.creatorPrefix}
            {statsData.creatorName}
          </Text>

          <TouchableOpacity style={styles.shareButton} onPress={() => {}}>
            <Text style={styles.shareIcon}>↗</Text>
            <Text style={styles.shareText}>{statsData.shareLabel}</Text>
          </TouchableOpacity>
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>{statsData.introTitle}</Text>
          <Text style={styles.aboutText}>{statsData.intro}</Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>{statsData.languageTitle}</Text>
          <View style={styles.languageGrid}>
            {statsData.supportedLanguages.map((language) => {
              const isSelected = language.id === statsData.selectedLanguageId;

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
          {statsData.actions.map((action) => (
            <TouchableOpacity
              key={action.id}
              onPress={() => {}}
              style={styles.actionItem}
            >
              <Text
                style={[
                  styles.actionIcon,
                  action.filledIcon && styles.actionIconFilled,
                ]}
              >
                {action.icon}
              </Text>
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
  logo: {
    width: 66,
    height: 66,
    borderRadius: 33,
    backgroundColor: '#C5924F',
    overflow: 'hidden',
    marginBottom: 24,
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
    width: 17,
    color: '#202124',
    fontSize: 17,
    fontWeight: '500',
    lineHeight: 20,
    marginRight: 18,
    textAlign: 'center',
  },
  actionIconFilled: {
    width: 17,
    height: 17,
    borderRadius: 8.5,
    backgroundColor: '#000000',
    color: '#FFFFFF',
    fontSize: 12,
    lineHeight: 17,
    marginRight: 18,
  },
  actionText: {
    color: '#202124',
    fontSize: 18,
    lineHeight: 25,
  },
});
