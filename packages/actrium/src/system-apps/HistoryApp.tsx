/**
 * History System App - Built-in app similar to chrome://history
 */
import React, { useState, useMemo, useCallback, useEffect } from 'react';
import {
  View,
  TextInput,
  ScrollView,
  TouchableOpacity,
  StyleSheet,
  NativeModules,
  useWindowDimensions,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import type { HistoryItemData, HistoryGroupData } from '../runtime-host/types';
import { AppText } from '../components';
import {
  clearBrowserHistory,
  deleteBrowserHistoryItem,
  hasBrowserDataModule,
  loadRecentBrowserHistory,
} from '../native-bridge/browser-data/BrowserDataModule';
import { openTab } from '../native-bridge/tab/TabModule';
import { getTypographyStyle, spacing, radius } from '../theme/tokens';

// Native module for history actions
const HistoryModule = NativeModules.HistoryModule;

interface HistoryAppProps {
  items?: HistoryItemData[];
  groups?: HistoryGroupData[];
}

const EMPTY_HISTORY_ITEMS: HistoryItemData[] = [];
const EMPTY_HISTORY_GROUPS: HistoryGroupData[] = [];

export default function HistoryApp({
  items = EMPTY_HISTORY_ITEMS,
  groups = EMPTY_HISTORY_GROUPS,
}: HistoryAppProps) {
  const { t, i18n } = useTranslation();
  const isNativeBrowserDataAvailable = hasBrowserDataModule();

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
  const [nativeItems, setNativeItems] = useState<HistoryItemData[]>([]);
  const [hiddenItemIds, setHiddenItemIds] = useState<Set<string>>(new Set());
  const [isLocallyCleared, setIsLocallyCleared] = useState(false);
  const usesProvidedItems = items.length > 0;
  const sourceItems = usesProvidedItems ? items : nativeItems;
  const displayItems = useMemo(
    () => (
      isLocallyCleared
        ? []
        : sourceItems.filter((item) => !hiddenItemIds.has(item.id))
    ),
    [hiddenItemIds, isLocallyCleared, sourceItems],
  );

  useEffect(() => {
    setHiddenItemIds(new Set());
    setIsLocallyCleared(false);
  }, [items, nativeItems]);

  useEffect(() => {
    if (usesProvidedItems || !isNativeBrowserDataAvailable) return;

    let isCancelled = false;
    loadRecentBrowserHistory().then((historyItems) => {
      if (!isCancelled) {
        setNativeItems(historyItems);
      }
    });

    return () => {
      isCancelled = true;
    };
  }, [usesProvidedItems, isNativeBrowserDataAvailable]);

  // Filter items based on search
  const filteredItems = useMemo(() => {
    if (!searchQuery) return displayItems;
    const query = searchQuery.toLowerCase();
    return displayItems.filter(item =>
      item.title.toLowerCase().includes(query) ||
      item.appURL?.toLowerCase().includes(query)
    );
  }, [displayItems, searchQuery]);

  const groupedItems = useMemo(() => groupByDate(filteredItems), [filteredItems]);

  const handleOpenItem = async (item: HistoryItemData) => {
    if (isBrowserPageUrl(item.appURL)) {
      await openTab({ appUrl: item.appURL });
      return;
    }
    if (HistoryModule?.openHistoryItem) {
      HistoryModule.openHistoryItem(item.id, item.appURL);
      return;
    }
    if (item.appURL) {
      await openTab({ appUrl: item.appURL });
    }
  };

  const handleDeleteItem = async (item: HistoryItemData) => {
    if (usesProvidedItems && HistoryModule?.deleteHistoryItem) {
      HistoryModule.deleteHistoryItem(item.id);
      setHiddenItemIds((current) => new Set(current).add(item.id));
      return;
    }
    if (isNativeBrowserDataAvailable && isBrowserPageUrl(item.appURL)) {
      if (await deleteBrowserHistoryItem(item.appURL)) {
        setNativeItems((currentItems) => currentItems.filter((current) => current.id !== item.id));
      }
      return;
    }
    if (HistoryModule?.deleteHistoryItem) {
      HistoryModule.deleteHistoryItem(item.id);
      return;
    }
    if (await deleteBrowserHistoryItem(item.appURL)) {
      setNativeItems((currentItems) => currentItems.filter((current) => current.id !== item.id));
    }
  };

  const handleRestoreGroup = async (group: HistoryGroupData) => {
    HistoryModule?.restoreGroup?.(group.id);
  };

  const handleClearHistory = async () => {
    if (usesProvidedItems && HistoryModule?.clearHistory) {
      HistoryModule.clearHistory();
      setIsLocallyCleared(true);
      return;
    }
    if (isNativeBrowserDataAvailable) {
      if (await clearBrowserHistory()) {
        setNativeItems([]);
      }
      return;
    }
    if (HistoryModule?.clearHistory) {
      HistoryModule.clearHistory();
      return;
    }
    if (await clearBrowserHistory()) {
      setNativeItems([]);
    }
  };

  return (
    <View style={styles.container}>
      {/* Header */}
      <View style={styles.header}>
        <AppText style={styles.title}>{t('actr.history.title')}</AppText>
        <View style={[styles.searchContainer, isPhone && styles.searchContainerPhone]}>
          <AppText style={styles.searchIcon}>🔍</AppText>
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
            <AppText style={styles.sectionTitle}>{t('actr.history.recentlyClosed')}</AppText>
            {groups.map(group => (
              <TouchableOpacity
                key={group.id}
                style={styles.groupRow}
                onPress={() => handleRestoreGroup(group as any)}
              >
                <View style={[styles.groupIcon, { backgroundColor: group.color + '33' }]}>
                  <AppText style={styles.groupIconText}>📁</AppText>
                </View>
                <View style={styles.groupInfo}>
                  <View style={styles.groupNameRow}>
                    <AppText style={styles.groupName}>
                      {group.name || t('actr.history.unnamedGroup')}
                    </AppText>
                    <View style={[styles.colorDot, { backgroundColor: group.color }]} />
                  </View>
                  <AppText style={styles.groupTabs}>
                    {t('actr.history.tabsCount', { count: group.tabs.length })}
                  </AppText>
                </View>
                <TouchableOpacity
                  style={styles.restoreButton}
                  onPress={() => handleRestoreGroup(group as any)}
                >
                  <AppText style={styles.restoreButtonText}>{t('actr.history.restore')}</AppText>
                </TouchableOpacity>
              </TouchableOpacity>
            ))}
          </View>
        )}

        {/* History Items */}
        {groupedItems.length === 0 ? (
          <View style={styles.emptyState}>
            <AppText style={styles.emptyIcon}>🕐</AppText>
            <AppText style={styles.emptyTitle}>{t('actr.history.noHistory')}</AppText>
            <AppText style={styles.emptySubtitle}>
              {searchQuery
                ? t('actr.history.noResult', { query: searchQuery })
                : t('actr.history.visitEmptyTip')}
            </AppText>
          </View>
        ) : (
          groupedItems.map((group: any) => (
            <View key={group.date} style={styles.section}>
              <AppText style={styles.sectionTitle}>{group.date}</AppText>
              {group.items.map((item: any) => (
                <TouchableOpacity
                  key={item.id}
                  style={styles.itemRow}
                  onPress={() => handleOpenItem(item as any)}
                >
                  <AppText style={styles.itemIcon}>📄</AppText>
                  <View style={styles.itemInfo}>
                    <AppText style={styles.itemTitle} numberOfLines={1}>
                      {item.title}
                    </AppText>
                    {item.appURL && (
                      <AppText style={styles.itemUrl} numberOfLines={1}>
                        {item.appURL.split('/').pop()}
                      </AppText>
                    )}
                  </View>
                  <AppText style={styles.itemTime}>{formatTime(item.visitTime)}</AppText>
                </TouchableOpacity>
              ))}
            </View>
          ))
        )}
      </ScrollView>

      <View style={styles.divider} />

      {/* Footer */}
      <View style={styles.footer}>
        <AppText style={styles.footerText}>{t('actr.history.itemsCount', { count: displayItems.length })}</AppText>
        <TouchableOpacity
          style={[styles.clearButton, displayItems.length === 0 && styles.clearButtonDisabled]}
          onPress={handleClearHistory}
          disabled={displayItems.length === 0}
        >
          <AppText style={[styles.clearButtonText, displayItems.length === 0 && styles.clearButtonTextDisabled]}>
            {t('actr.history.clearHistory')}
          </AppText>
        </TouchableOpacity>
      </View>
    </View>
  );
}

const isBrowserPageUrl = (url?: string): url is string => {
  if (!url) return false;
  const normalized = url.trim().toLowerCase();
  return normalized.startsWith('http://') || normalized.startsWith('https://');
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#1e1e1e',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: spacing.lg,
    gap: spacing.md,
  },
  title: {
    ...getTypographyStyle('title2'),
    color: '#fff',
  },
  searchContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#2d2d2d',
    borderRadius: 6,
    paddingHorizontal: spacing.sm,
    paddingVertical: spacing.xxs,
    flex: 1,
    maxWidth: 420,
    minWidth: 0,
  },
  searchContainerPhone: {
    maxWidth: '100%',
  },
  searchIcon: {
    ...getTypographyStyle('body'),
    marginRight: spacing.xs,
  },
  searchInput: {
    flex: 1,
    ...getTypographyStyle('body'),
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
    padding: spacing.lg,
  },
  section: {
    marginBottom: spacing.xxl,
  },
  sectionTitle: {
    ...getTypographyStyle('bodyStrong'),
    fontWeight: '600',
    color: '#888',
    marginBottom: spacing.md,
    textTransform: 'uppercase',
  },
  groupRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: spacing.sm,
    paddingHorizontal: spacing.md,
    backgroundColor: '#2d2d2d',
    borderRadius: radius.sm,
    marginBottom: spacing.sm,
  },
  groupIcon: {
    width: 32,
    height: 32,
    borderRadius: 6,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: spacing.md,
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
    ...getTypographyStyle('body'),
    color: '#fff',
    marginRight: spacing.sm,
  },
  colorDot: {
    width: 8,
    height: 8,
    borderRadius: 4,
  },
  groupTabs: {
    ...getTypographyStyle('footnote'),
    color: '#888',
    marginTop: 2,
  },
  restoreButton: {
    backgroundColor: '#0a84ff',
    paddingHorizontal: spacing.md,
    paddingVertical: spacing.xs,
    borderRadius: radius.xs,
  },
  restoreButtonText: {
    ...getTypographyStyle('footnote'),
    color: '#fff',
    fontWeight: '500',
  },
  itemRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 10,
    paddingHorizontal: spacing.md,
    backgroundColor: '#2d2d2d',
    borderRadius: radius.sm,
    marginBottom: spacing.xxs,
  },
  itemIcon: {
    fontSize: 18,
    marginRight: 12,
  },
  itemInfo: {
    flex: 1,
  },
  itemTitle: {
    ...getTypographyStyle('body'),
    color: '#fff',
  },
  itemUrl: {
    ...getTypographyStyle('footnote'),
    color: '#888',
    marginTop: 2,
  },
  itemTime: {
    ...getTypographyStyle('footnote'),
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
    ...getTypographyStyle('title3'),
    color: '#fff',
    marginBottom: spacing.sm,
  },
  emptySubtitle: {
    ...getTypographyStyle('body'),
    color: '#888',
  },
  footer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: spacing.lg,
  },
  footerText: {
    ...getTypographyStyle('footnote'),
    color: '#888',
  },
  clearButton: {
    paddingHorizontal: spacing.md,
    paddingVertical: spacing.xs,
  },
  clearButtonDisabled: {
    opacity: 0.5,
  },
  clearButtonText: {
    ...getTypographyStyle('body'),
    color: '#0a84ff',
  },
  clearButtonTextDisabled: {
    color: '#888',
  },
});
