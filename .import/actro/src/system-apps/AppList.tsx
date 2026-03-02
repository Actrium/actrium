/**
 * Bookmarks System App - Built-in app similar to chrome://bookmarks
 */
import React, { useState } from 'react';
import {
  View,
  Text,
  TextInput,
  ScrollView,
  StyleSheet,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { MyTouchableOpacity } from 'askit';

/**
 * Mock structured data for bookmark cards
 */
const BOOKMARK_CARDS = [
  {
    titleKey: 'actr.app.mockTitle1',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.officialService',
    labelIcon: '🏔️',
  },
  {
    titleKey: 'actr.app.mockTitle2',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.guide',
    labelIcon: '📍',
  },
  {
    titleKey: 'actr.app.mockTitle3',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.realtime',
    labelIcon: '🛰️',
  },
  {
    titleKey: 'actr.app.mockTitle4',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.parentChild',
    labelIcon: '🎡',
  },
  {
    titleKey: 'actr.app.mockTitle5',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.culture',
    labelIcon: '📜',
  },
  {
    titleKey: 'actr.app.mockTitle1',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.officialService',
    labelIcon: '📍',
  },
  {
    titleKey: 'actr.app.mockTitle1',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.officialService',
    labelIcon: '📍',
  },
  {
    titleKey: 'actr.app.mockTitle1',
    referenceKey: 'actr.app.reference',
    labelKey: 'actr.app.officialService',
    labelIcon: '📍',
  },
];

export default function BookmarksApp({ bookmarks = [] }: { bookmarks?: any[] }) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState('');

  return (
    <View style={styles.container}>
      {/* 顶部标题栏可以根据需要保留或移除，这里为了简洁先移除原有逻辑 */}
      
      {/* 卡片网格 (Card Grid) */}
      <ScrollView 
        style={styles.content} 
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.grid}>
          {BOOKMARK_CARDS.map((card, index) => (
            <View key={index} style={styles.cardWrapper}>
              <View style={styles.card}>
                <Text style={styles.cardTitle} numberOfLines={3}>
                  {t(card.titleKey as any)}
                </Text>
                
                <View style={styles.referenceRow}>
                  <Text style={styles.referenceLabel}>{t(card.referenceKey as any)}</Text>
                  <Text style={styles.chevronIcon}>▼</Text>
                </View>

                <View style={styles.cardFooter}>
                  <View style={styles.labelGroup}>
                    <Text style={styles.labelIcon}>{card.labelIcon}</Text>
                    <Text style={styles.labelName}>{t(card.labelKey as any)}</Text>
                  </View>
                  
                  <View style={styles.actionGroup}>
                    <MyTouchableOpacity style={styles.starAction}>
                      <Text style={styles.starIcon}>☆</Text>
                    </MyTouchableOpacity>
                    <MyTouchableOpacity style={styles.closeAction}>
                      <Text style={styles.closeIcon}>×</Text>
                    </MyTouchableOpacity>
                  </View>
                </View>
              </View>
            </View>
          ))}
        </View>
      </ScrollView>

      {/* 底部搜索框 (Bottom Search Box) */}
      <View style={styles.searchOverlay}>
        <View style={styles.searchBar}>
          <Text style={styles.searchIcon}>🔍</Text>
          <TextInput
            style={styles.searchInput}
            placeholder={t('actr.app.searchAppPlaceholder')}
            placeholderTextColor="rgba(255, 255, 255, 0.5)"
            value={searchQuery}
            onChangeText={setSearchQuery}
            underlineColorAndroid="transparent"
          />
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000', // bg-black
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    padding: 24,
    paddingTop: 32,
    paddingBottom: 120, // 底部搜索框留白
  },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    marginHorizontal: -12,
  },
  cardWrapper: {
    width: '25%', // 4 columns
    padding: 12,
  },
  card: {
    backgroundColor: 'rgba(255, 255, 255, 0.95)', // from-white/95 to-white/90
    borderRadius: 24,
    padding: 24,
    minHeight: 180,
    justifyContent: 'space-between',
    // Shadow properties commented out as requested
    /*
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 10 },
    shadowOpacity: 0.1,
    shadowRadius: 20,
    elevation: 5,
    */
  },
  cardTitle: {
    fontSize: 14,
    fontWeight: '400',
    color: '#364153',
    lineHeight: 20,
    marginBottom: 8,
  },
  referenceRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
    marginBottom: 20,
  },
  referenceLabel: {
    fontSize: 12,
    color: '#99A1AF',
  },
  chevronIcon: {
    fontSize: 10,
    color: '#99A1AF',
  },
  cardFooter: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginTop: 'auto',
  },
  labelGroup: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  labelIcon: {
    fontSize: 16,
  },
  labelName: {
    fontSize: 12,
    color: '#364153',
  },
  actionGroup: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  starAction: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: '#d1d5dc',
    alignItems: 'center',
    justifyContent: 'center',
  },
  starIcon: {
    fontSize: 12,
    color: '#364153',
    marginTop: -2,
  },
  closeAction: {
    width: 24,
    height: 24,
    borderRadius: 12,
    backgroundColor: '#1e2939',
    alignItems: 'center',
    justifyContent: 'center',
  },
  closeIcon: {
    fontSize: 14,
    color: '#fff',
    marginTop: -1,
  },
  searchOverlay: {
    position: 'absolute',
    bottom: 0,
    left: 0,
    right: 0,
    height: 120,
    paddingHorizontal: 48,
    justifyContent: 'center',
    // Gradient replacement
    backgroundColor: 'rgba(0, 0, 0, 0.8)',
  },
  searchBar: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: 'rgba(255, 255, 255, 0.1)',
    borderRadius: 30,
    height: 56,
    paddingHorizontal: 20,
    borderWidth: 1,
    borderColor: 'rgba(255, 255, 255, 0.2)',
    maxWidth: 800,
    width: '100%',
    alignSelf: 'center',
  },
  searchIcon: {
    fontSize: 18,
    color: 'rgba(255, 255, 255, 0.5)',
    marginRight: 12,
  },
  searchInput: {
    flex: 1,
    color: '#fff',
    fontSize: 16,
    paddingVertical: 0,
  },
});
