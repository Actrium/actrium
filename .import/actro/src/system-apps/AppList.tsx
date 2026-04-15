/**
 * Bookmarks System App - Built-in app similar to chrome://bookmarks
 */
import React, { useState } from 'react';
import {
  View,
  TextInput,
  ScrollView,
  StyleSheet,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { MyTouchableOpacity } from 'askit';
import { AppText } from '../components';
import { getTypographyStyle, spacing, radius } from '../theme/tokens';

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
                <AppText style={styles.cardTitle} numberOfLines={3}>
                  {t(card.titleKey as any)}
                </AppText>
                
                <View style={styles.referenceRow}>
                  <AppText style={styles.referenceLabel}>{t(card.referenceKey as any)}</AppText>
                  <AppText style={styles.chevronIcon}>▼</AppText>
                </View>

                <View style={styles.cardFooter}>
                  <View style={styles.labelGroup}>
                    <AppText style={styles.labelIcon}>{card.labelIcon}</AppText>
                    <AppText style={styles.labelName}>{t(card.labelKey as any)}</AppText>
                  </View>
                  
                  <View style={styles.actionGroup}>
                    <MyTouchableOpacity style={styles.starAction}>
                      <AppText style={styles.starIcon}>☆</AppText>
                    </MyTouchableOpacity>
                    <MyTouchableOpacity style={styles.closeAction}>
                      <AppText style={styles.closeIcon}>×</AppText>
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
          <AppText style={styles.searchIcon}>🔍</AppText>
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
    padding: spacing.xxl,
    paddingTop: spacing.xxxl,
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
    padding: spacing.xxl,
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
    ...getTypographyStyle('body'),
    fontWeight: '400',
    color: '#364153',
    marginBottom: spacing.sm,
  },
  referenceRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xxs,
    marginBottom: spacing.xl,
  },
  referenceLabel: {
    ...getTypographyStyle('footnote'),
    color: '#99A1AF',
  },
  chevronIcon: {
    ...getTypographyStyle('caption2'),
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
    gap: spacing.xs,
  },
  labelIcon: {
    ...getTypographyStyle('bodyInput'),
  },
  labelName: {
    ...getTypographyStyle('footnote'),
    color: '#364153',
  },
  actionGroup: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
  },
  starAction: {
    width: 24,
    height: 24,
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: '#d1d5dc',
    alignItems: 'center',
    justifyContent: 'center',
  },
  starIcon: {
    ...getTypographyStyle('footnote'),
    color: '#364153',
    marginTop: -2,
  },
  closeAction: {
    width: 24,
    height: 24,
    borderRadius: radius.md,
    backgroundColor: '#1e2939',
    alignItems: 'center',
    justifyContent: 'center',
  },
  closeIcon: {
    ...getTypographyStyle('body'),
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
    paddingHorizontal: spacing.xl,
    borderWidth: 1,
    borderColor: 'rgba(255, 255, 255, 0.2)',
    maxWidth: 800,
    width: '100%',
    alignSelf: 'center',
  },
  searchIcon: {
    fontSize: 18,
    color: 'rgba(255, 255, 255, 0.5)',
    marginRight: spacing.md,
  },
  searchInput: {
    flex: 1,
    color: '#fff',
    ...getTypographyStyle('bodyInput'),
    paddingVertical: 0,
  },
});
