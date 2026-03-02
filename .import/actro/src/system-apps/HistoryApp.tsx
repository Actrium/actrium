/**
 * History System App - Built-in app similar to chrome://history
 */
import React, { useState, useMemo, useCallback } from 'react';
import {
  View,
  Text,
  TextInput,
  ScrollView,
  TouchableOpacity,
  StyleSheet,
  NativeModules,
  useWindowDimensions,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import type { HistoryItemData, HistoryGroupData } from '../actr-host/types';

// Native module for history actions
const HistoryModule = NativeModules.HistoryModule;

interface HistoryAppProps {
  items: HistoryItemData[];
  groups: HistoryGroupData[];
}

export default function HistoryApp({ items, groups }: HistoryAppProps) {
  const { t, i18n } = useTranslation();
  
  // Group items by date
  const groupByDate = useCallback((items: HistoryItemData[]): { date: string; items: HistoryItemData[] }[] => {
    const grouped: Record<string, HistoryItemData[]> = {};

    items.forEach(item => {
      const date = new Date(item.visitTime);
      const today = new Date();
      const yesterday = new Date(today);
      yesterday.setDate(yesterday.getDate() - 1);

      let dateKey: string;
      if (date.toDateString() === today.toDateString()) {
        dateKey = t('actr.history.today');
      } else if (date.toDateString() === yesterday.toDateString()) {
        dateKey = t('actr.history.yesterday');
      } else {
        dateKey = date.toLocaleDateString(i18n.language === 'zh-Hans' ? 'zh-CN' : 'en-US', {
          weekday: 'long',
          year: 'numeric',
          month: 'long',
          day: 'numeric'
        });
      }

      if (!grouped[dateKey]) {
        grouped[dateKey] = [];
      }
      grouped[dateKey].push(item);
    });

    return Object.entries(grouped).map(([date, items]) => ({ date, items }));
  }, [t, i18n.language]);

  const formatTime = useCallback((timestamp: number): string => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString(i18n.language === 'zh-Hans' ? 'zh-CN' : 'en-US', {
      hour: '2-digit',
      minute: '2-digit',
      hour12: i18n.language !== 'zh-Hans'
    });
  }, [i18n.language]);

  const { width } = useWindowDimensions();
  const isPhone = width < 768;
  const [searchQuery, setSearchQuery] = useState('');

  // Filter items based on search
  const filteredItems = useMemo(() => {
    if (!searchQuery) return items;
    const query = searchQuery.toLowerCase();
    return items.filter(item =>
      item.title.toLowerCase().includes(query) ||
      item.appURL?.toLowerCase().includes(query)
    );
  }, [items, searchQuery]);

  const groupedItems = useMemo(() => groupByDate(filteredItems), [filteredItems]);

  const handleOpenItem = (item: HistoryItemData) => {
    HistoryModule?.openHistoryItem?.(item.id, item.appURL);
  };

  const handleDeleteItem = (item: HistoryItemData) => {
    HistoryModule?.deleteHistoryItem?.(item.id);
  };

  const handleRestoreGroup = (group: HistoryGroupData) => {
    HistoryModule?.restoreGroup?.(group.id);
  };

  const handleClearHistory = () => {
    HistoryModule?.clearHistory?.();
  };

  return (
    <View style={styles.container}>
      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.title}>{t('actr.history.title')}</Text>
        <View style={[styles.searchContainer, isPhone && styles.searchContainerPhone]}>
          <Text style={styles.searchIcon}>🔍</Text>
          <TextInput
            style={styles.searchInput}
            placeholder={t('actr.history.searchPlaceholder')}
            placeholderTextColor="#888"
            value={searchQuery}
            onChangeText={setSearchQuery}
          />
        </View>
      </View>

      <View style={styles.divider} />

      {/* Content */}
      <ScrollView style={styles.content} contentContainerStyle={styles.contentContainer} showsHorizontalScrollIndicator={false}>
        {/* Closed Groups Section */}
        {groups.length > 0 && (
          <View style={styles.section}>
            <Text style={styles.sectionTitle}>{t('actr.history.recentlyClosed')}</Text>
            {groups.map(group => (
              <TouchableOpacity
                key={group.id}
                style={styles.groupRow}
                onPress={() => handleRestoreGroup(group as any)}
              >
                <View style={[styles.groupIcon, { backgroundColor: group.color + '33' }]}>
                  <Text style={styles.groupIconText}>📁</Text>
                </View>
                <View style={styles.groupInfo}>
                  <View style={styles.groupNameRow}>
                    <Text style={styles.groupName}>
                      {group.name || t('actr.history.unnamedGroup')}
                    </Text>
                    <View style={[styles.colorDot, { backgroundColor: group.color }]} />
                  </View>
                  <Text style={styles.groupTabs}>
                    {t('actr.history.tabsCount', { count: group.tabs.length })}
                  </Text>
                </View>
                <TouchableOpacity
                  style={styles.restoreButton}
                  onPress={() => handleRestoreGroup(group as any)}
                >
                  <Text style={styles.restoreButtonText}>{t('actr.history.restore')}</Text>
                </TouchableOpacity>
              </TouchableOpacity>
            ))}
          </View>
        )}

        {/* History Items */}
        {groupedItems.length === 0 ? (
          <View style={styles.emptyState}>
            <Text style={styles.emptyIcon}>🕐</Text>
            <Text style={styles.emptyTitle}>{t('actr.history.noHistory')}</Text>
            <Text style={styles.emptySubtitle}>
              {searchQuery
                ? t('actr.history.noResult', { query: searchQuery })
                : t('actr.history.visitEmptyTip')}
            </Text>
          </View>
        ) : (
          groupedItems.map((group: any) => (
            <View key={group.date} style={styles.section}>
              <Text style={styles.sectionTitle}>{group.date}</Text>
              {group.items.map((item: any) => (
                <TouchableOpacity
                  key={item.id}
                  style={styles.itemRow}
                  onPress={() => handleOpenItem(item as any)}
                >
                  <Text style={styles.itemIcon}>📄</Text>
                  <View style={styles.itemInfo}>
                    <Text style={styles.itemTitle} numberOfLines={1}>
                      {item.title}
                    </Text>
                    {item.appURL && (
                      <Text style={styles.itemUrl} numberOfLines={1}>
                        {item.appURL.split('/').pop()}
                      </Text>
                    )}
                  </View>
                  <Text style={styles.itemTime}>{formatTime(item.visitTime)}</Text>
                </TouchableOpacity>
              ))}
            </View>
          ))
        )}
      </ScrollView>

      <View style={styles.divider} />

      {/* Footer */}
      <View style={styles.footer}>
        <Text style={styles.footerText}>{t('actr.history.itemsCount', { count: items.length })}</Text>
        <TouchableOpacity
          style={[styles.clearButton, items.length === 0 && styles.clearButtonDisabled]}
          onPress={handleClearHistory}
          disabled={items.length === 0}
        >
          <Text style={[styles.clearButtonText, items.length === 0 && styles.clearButtonTextDisabled]}>
            {t('actr.history.clearHistory')}
          </Text>
        </TouchableOpacity>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#1e1e1e',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: 16,
    gap: 12,
  },
  title: {
    fontSize: 20,
    fontWeight: '600',
    color: '#fff',
  },
  searchContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#2d2d2d',
    borderRadius: 6,
    paddingHorizontal: 8,
    paddingVertical: 4,
    flex: 1,
    maxWidth: 420,
    minWidth: 0,
  },
  searchContainerPhone: {
    maxWidth: '100%',
  },
  searchIcon: {
    fontSize: 14,
    marginRight: 6,
  },
  searchInput: {
    flex: 1,
    fontSize: 14,
    color: '#fff',
    padding: 0,
    minWidth: 0,
  },
  divider: {
    height: 1,
    backgroundColor: '#3d3d3d',
  },
  content: {
    flex: 1,
  },
  contentContainer: {
    padding: 16,
  },
  section: {
    marginBottom: 24,
  },
  sectionTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#888',
    marginBottom: 12,
    textTransform: 'uppercase',
  },
  groupRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 8,
    paddingHorizontal: 12,
    backgroundColor: '#2d2d2d',
    borderRadius: 8,
    marginBottom: 8,
  },
  groupIcon: {
    width: 32,
    height: 32,
    borderRadius: 6,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: 12,
  },
  groupIconText: {
    fontSize: 16,
  },
  groupInfo: {
    flex: 1,
  },
  groupNameRow: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  groupName: {
    fontSize: 14,
    color: '#fff',
    marginRight: 8,
  },
  colorDot: {
    width: 8,
    height: 8,
    borderRadius: 4,
  },
  groupTabs: {
    fontSize: 12,
    color: '#888',
    marginTop: 2,
  },
  restoreButton: {
    backgroundColor: '#0a84ff',
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 4,
  },
  restoreButtonText: {
    fontSize: 12,
    color: '#fff',
    fontWeight: '500',
  },
  itemRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 10,
    paddingHorizontal: 12,
    backgroundColor: '#2d2d2d',
    borderRadius: 8,
    marginBottom: 4,
  },
  itemIcon: {
    fontSize: 18,
    marginRight: 12,
  },
  itemInfo: {
    flex: 1,
  },
  itemTitle: {
    fontSize: 14,
    color: '#fff',
  },
  itemUrl: {
    fontSize: 12,
    color: '#888',
    marginTop: 2,
  },
  itemTime: {
    fontSize: 12,
    color: '#888',
  },
  emptyState: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 60,
  },
  emptyIcon: {
    fontSize: 48,
    marginBottom: 16,
  },
  emptyTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#fff',
    marginBottom: 8,
  },
  emptySubtitle: {
    fontSize: 14,
    color: '#888',
  },
  footer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: 16,
  },
  footerText: {
    fontSize: 12,
    color: '#888',
  },
  clearButton: {
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  clearButtonDisabled: {
    opacity: 0.5,
  },
  clearButtonText: {
    fontSize: 13,
    color: '#0a84ff',
  },
  clearButtonTextDisabled: {
    color: '#888',
  },
});
