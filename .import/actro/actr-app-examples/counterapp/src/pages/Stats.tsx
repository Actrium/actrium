import { useState } from 'react';
import { ScrollView, View, Text, TouchableOpacity, StyleSheet } from 'rill/let';

interface StatsProps {}

const languages = [
  'いらっしゃいませ',
  'Welcome',
  'accueil',
  '환영',
  '歡迎',
  '中文',
  'bienvenido',
];

export default function Stats({}: StatsProps) {
  const [isLanguageDropdownOpen, setIsLanguageDropdownOpen] = useState(false);
  const [selectedLanguage, setSelectedLanguage] = useState('中文');

  return (
    <View style={styles.guestContainer}>
      <ScrollView
        style={styles.container}
        contentContainerStyle={styles.contentContainer}
        id='sidebar'
      >
        {/* 语言 Section */}
        <View style={[styles.section, { zIndex: 10 }]}>
          <Text style={styles.sectionTitle}>语言</Text>
          <TouchableOpacity
            onPress={() => setIsLanguageDropdownOpen(!isLanguageDropdownOpen)}
            style={styles.dropdownButton}
          >
            <Text style={styles.icon}>📄</Text>
            <Text style={styles.selectedLanguageText}>{selectedLanguage}</Text>
            <Text
              style={[
                styles.arrowIcon,
                {
                  transform: [
                    { rotate: isLanguageDropdownOpen ? '90deg' : '0deg' },
                  ],
                },
              ]}
            >
              ▶
            </Text>
          </TouchableOpacity>

          {isLanguageDropdownOpen && (
            <View style={styles.dropdownList}>
              {languages.map((lang, index) => (
                <TouchableOpacity
                  key={lang}
                  onPress={() => {
                    setSelectedLanguage(lang);
                    setIsLanguageDropdownOpen(false);
                  }}
                  style={[
                    styles.dropdownItem,
                    lang === selectedLanguage && styles.dropdownItemActive,
                    index === languages.length - 1 && { borderBottomWidth: 0 },
                  ]}
                >
                  <Text style={styles.icon}>📄</Text>
                  <Text
                    style={[
                      styles.dropdownItemText,
                      lang === selectedLanguage &&
                        styles.dropdownItemTextActive,
                    ]}
                  >
                    {lang}
                  </Text>
                </TouchableOpacity>
              ))}
            </View>
          )}
        </View>

        {/* 关于 Section */}
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>关于</Text>
          <Text style={styles.aboutText}>
            Mu
            AI是一款人工智能服务产品，为游客提供出发前规划至游览后反馈的全流程服务，愿景是成为游客景区游玩时不可或缺的智能伙伴，让游览更便捷、丰富。V0.9.6
          </Text>
        </View>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  guestContainer: {
    flex: 1,
    shadowColor: '#797979',
    shadowOpacity: 0.25,
    shadowRadius: 4.8,
    backgroundColor: '#fff',
    width: '100%',
    borderTopLeftRadius: 24,
    borderBottomLeftRadius: 24,
  },
  container: {
    flex: 1,
    width: '100%',
  },
  contentContainer: {
    padding: 24,
  },
  section: {
    marginBottom: 24,
  },
  sectionTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#364153',
    marginBottom: 12,
  },
  dropdownButton: {
    width: '100%',
    height: 44,
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: 'rgba(0,0,0,0.1)',
    borderRadius: 8,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 12,
  },
  icon: {
    fontSize: 16,
    marginRight: 8,
  },
  selectedLanguageText: {
    flex: 1,
    fontSize: 14,
    fontWeight: 'bold',
    color: '#0a0a0a',
  },
  arrowIcon: {
    fontSize: 12,
    color: '#364153',
  },
  dropdownList: {
    marginTop: 4,
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: 'rgba(0,0,0,0.1)',
    borderRadius: 8,
    overflow: 'hidden',
  },
  dropdownItem: {
    height: 44,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 12,
    borderBottomWidth: 1,
    borderBottomColor: 'rgba(0,0,0,0.05)',
  },
  dropdownItemActive: {
    backgroundColor: '#f9fafb',
  },
  dropdownItemText: {
    flex: 1,
    fontSize: 14,
    color: '#0a0a0a',
  },
  dropdownItemTextActive: {
    fontWeight: 'bold',
  },
  aboutText: {
    fontSize: 14,
    color: '#364153',
    lineHeight: 20,
    marginBottom: 16,
  },
});
