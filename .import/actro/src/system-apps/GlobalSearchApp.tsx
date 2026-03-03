/**
 * Global Search App - 全局搜索页面
 */
import React, {
  useState,
  useRef,
  useEffect,
  useMemo,
  useCallback,
} from 'react';
import {
  View,
  Text,
  TextInput,
  ScrollView,
  StyleSheet,
  Image,
  TouchableWithoutFeedback,
  useWindowDimensions,
  Linking,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from '../actr-host/components/Icon';
import SafeBottomView from '../actr-host/components/SafeBottomView';
import { useKeyboardInset } from '../hooks/useKeyboardInset';
import { HOST_SHADOWS } from '../actr-host/styles/shadows';
import { useAppStore } from '../actr-host/store/useAppStore';
import {
  BookmarkRepository,
  TBookmarkItem,
} from '../repository/BookmarkRepository';
import { openTab, exitSearch } from '../native-bridge/tab/TabModule';

// 搜索结果类型
type SearchResultType = 'active' | 'bookmark' | 'setting';

interface SearchResultItem {
  id: string;
  type: SearchResultType;
  name: string;
  // 应用类型
  logo?: string;
  appId?: string;
  tabId?: string;
  url?: string;
  // 设置类型
  icon?: string;
  action?: () => void;
}

interface SearchResultGroup {
  type: SearchResultType;
  title: string;
  items: SearchResultItem[];
}

export default function GlobalSearchApp() {
  const { t } = useTranslation();
  const [inputText, setInputText] = useState('');
  const [searchQuery, setSearchQuery] = useState(''); // 回车后才更新
  const [bookmarks, setBookmarks] = useState<TBookmarkItem[]>([]);
  const inputRef = useRef<TextInput>(null);
  const { keyboardInset, dismissKeyboard } = useKeyboardInset();
  const { width: screenWidth } = useWindowDimensions();
  const apps = useAppStore((s) => s.apps);

  const isPhone = screenWidth < 600;
  const inputWidth = isPhone ? screenWidth * 0.85 : 500;
  const resultsAreaSize = isPhone ? 'auto' : 500;
  const containerGap = isPhone ? 24 : 36;
  const inputBorderRadius = 24;

  // 自动聚焦输入框
  useEffect(() => {
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, 200);
    return () => clearTimeout(timer);
  }, []);

  // 加载书签数据
  useEffect(() => {
    BookmarkRepository.getBookmarksHierarchy().then((groups) => {
      const allItems = groups.flatMap((g) => g.items);
      setBookmarks(allItems);
    });
  }, []);

  // 可搜索的设置项
  const searchableSettings: SearchResultItem[] = useMemo(() => {
    const handleFeedback = () => {
      const subject = encodeURIComponent('#吐个槽关于');
      const mailto = `mailto:support@askaway.chat?subject=${subject}`;
      Linking.openURL(mailto);
    };

    return [
      {
        id: 'setting-feedback',
        type: 'setting',
        name: t('actr.settings.feedback'),
        icon: 'feedback',
        action: handleFeedback,
      },
      {
        id: 'setting-logout',
        type: 'setting',
        name: t('actr.settings.logout'),
        icon: 'logout',
      },
    ];
  }, [t]);

  // 搜索结果
  const searchResults: SearchResultGroup[] = useMemo(() => {
    if (!searchQuery.trim()) return [];
    const q = searchQuery.toLowerCase();

    // 1. 搜索活跃应用
    const activeResults: SearchResultItem[] = Object.values(apps)
      .filter((app) => app.app_name?.toLowerCase().includes(q))
      .map((app) => ({
        id: `active-${app.app_id}`,
        type: 'active' as const,
        name: app.app_name,
        logo: app.info?.logo,
        appId: app.app_id,
      }));

    // 2. 搜索收藏
    const bookmarkResults: SearchResultItem[] = bookmarks
      .filter((b) => b.app_name?.toLowerCase().includes(q))
      .map((b) => ({
        id: `bookmark-${b.id}`,
        type: 'bookmark' as const,
        name: b.app_name || '',
        logo: b.logo || undefined,
        appId: b.app_id,
        tabId: b.tab_id || undefined,
        url: b.url || undefined,
      }));

    // 3. 搜索设置
    const settingResults = searchableSettings.filter((s) =>
      s.name.toLowerCase().includes(q),
    );

    const groups: SearchResultGroup[] = [];
    if (activeResults.length > 0) {
      groups.push({
        type: 'active',
        title: t('actr.search.active'),
        items: activeResults,
      });
    }
    if (bookmarkResults.length > 0) {
      groups.push({
        type: 'bookmark',
        title: t('actr.search.bookmarks'),
        items: bookmarkResults,
      });
    }
    if (settingResults.length > 0) {
      groups.push({
        type: 'setting',
        title: t('actr.search.settings'),
        items: settingResults,
      });
    }

    return groups;
  }, [searchQuery, apps, bookmarks, searchableSettings, t]);

  // 回车触发搜索
  const handleSubmit = useCallback(() => {
    setSearchQuery(inputText.trim());
    dismissKeyboard();
  }, [inputText, dismissKeyboard]);

  const handleClearSearch = useCallback(() => {
    setInputText('');
    setSearchQuery('');
  }, []);

  // 点击搜索结果项
  const handleItemPress = useCallback((item: SearchResultItem) => {
    if (item.type === 'setting') {
      item.action?.();
    } else {
      // 应用类型
      openTab({
        tabId: item.tabId ?? '',
        appUrl: item.url || item.appId || '',
      });
    }
    exitSearch();
  }, []);

  // 渲染搜索结果项
  const renderResultItem = (item: SearchResultItem, isLast: boolean) => {
    if (item.type === 'setting') {
      return (
        <React.Fragment key={item.id}>
          <TouchableOpacity
            style={styles.listItem}
            onPress={() => handleItemPress(item)}
          >
            <Icon type={item.icon!} size={22} color='#9e9ea7' />
            <Text style={styles.listItemName} numberOfLines={1}>
              {item.name}
            </Text>
          </TouchableOpacity>
          {!isLast && <View style={styles.listItemDivider} />}
        </React.Fragment>
      );
    }
    // 应用类型（active/bookmark）
    return (
      <React.Fragment key={item.id}>
        <TouchableOpacity
          style={styles.listItem}
          onPress={() => handleItemPress(item)}
        >
          {item.logo ? (
            <Image source={{ uri: item.logo }} style={styles.listItemLogo} />
          ) : (
            <View style={styles.listItemLogo} />
          )}
          <Text style={styles.listItemName} numberOfLines={1}>
            {item.name}
          </Text>
        </TouchableOpacity>
        {!isLast && <View style={styles.listItemDivider} />}
      </React.Fragment>
    );
  };

  // 渲染搜索结果列表
  const renderResults = () => {
    if (!searchQuery.trim()) {
      return (
        <View style={styles.emptyState}>
          <Text style={styles.emptyStateText}>
            {t('actr.search.inputHint')}
          </Text>
        </View>
      );
    }

    if (searchResults.length === 0) {
      return (
        <View style={styles.emptyState}>
          <Text style={styles.emptyStateText}>
            {t('actr.search.noResults')}
          </Text>
        </View>
      );
    }

    return (
      <>
        {searchResults.map((group) => (
          <View key={group.type} style={styles.groupContainer}>
            <Text style={styles.groupTitle}>{group.title}</Text>
            <View style={styles.groupCard}>
              {group.items.map((item, index) =>
                renderResultItem(item, index === group.items.length - 1),
              )}
            </View>
          </View>
        ))}
      </>
    );
  };

  const renderSearchInput = (inputStyle: any, containerStyle: any) => (
    <View style={styles.searchRow}>
      <View style={containerStyle}>
        <Icon
          type='search'
          size={18}
          color='#999'
          style={styles.inputPrefixIcon}
        />
        <TextInput
          ref={inputRef}
          style={inputStyle}
          placeholder={t('actr.search.placeholder')}
          placeholderTextColor='#999'
          value={inputText}
          onChangeText={setInputText}
          onSubmitEditing={handleSubmit}
          returnKeyType='search'
          autoFocus={true}
          clearButtonMode='never'
          keyboardAppearance='dark'
        />
        {inputText.length > 0 && (
          <TouchableOpacity
            style={styles.clearButton}
            onPress={handleClearSearch}
          >
            <Icon type='input_clear' size={19} color='#666' />
          </TouchableOpacity>
        )}
      </View>
      <TouchableOpacity
        style={styles.standaloneCloseButton}
        onPress={() => {
          exitSearch();
        }}
      >
        <Icon type='close' size={16} color='#404040' />
      </TouchableOpacity>
    </View>
  );

  if (isPhone) {
    return (
      <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
        <View style={styles.phoneContainer}>
          {/* 搜索结果列表 */}
          <ScrollView
            style={styles.content}
            contentContainerStyle={styles.phoneScrollContent}
            showsVerticalScrollIndicator={false}
            keyboardShouldPersistTaps='handled'
          >
            {renderResults()}
          </ScrollView>

          {/* 底部输入框 */}
          <View style={styles.phoneInputDock}>
            <SafeBottomView style={styles.phoneInputSafeArea} minBottom={10}>
              <View style={styles.phoneInputSection}>
                {renderSearchInput(styles.phoneInput, [
                  styles.phoneInputContainer,
                ])}
              </View>
              {/* Phone: 键盘避让已由 SwiftUI（PhoneSystemAppView）在 Native 层处理，
                   通过缩小 RCTRootView.frame 实现上移，无需 RN 层重复添加 keyboardInset 偏移 */}
            </SafeBottomView>
          </View>
        </View>
      </TouchableWithoutFeedback>
    );
  }

  // iPad 布局
  return (
    <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
      <ScrollView
        contentContainerStyle={[
          styles.scrollContent,
          { paddingBottom: keyboardInset },
        ]}
        keyboardShouldPersistTaps='handled'
      >
        <View style={[styles.GlobalSearchContainer, { gap: containerGap }]}>
          {/* 结果区域 */}
          <View
            style={[
              styles.resultsArea,
              { width: inputWidth, height: resultsAreaSize },
            ]}
          >
            <View style={styles.resultsHeader}>
              <Text style={styles.resultsHeaderTitle}>
                {searchQuery.length > 0
                  ? t('actr.search.resultsIn')
                  : t('actr.search.recent')}
              </Text>
            </View>
            <ScrollView
              style={{ flex: 1 }}
              contentContainerStyle={styles.resultsListContent}
              showsVerticalScrollIndicator={false}
            >
              {renderResults()}
            </ScrollView>
          </View>

          {/* 搜索框 */}
          {renderSearchInput(
            [
              styles.input,
              { width: inputWidth, borderRadius: inputBorderRadius },
            ],
            [
              styles.inputContainer,
              { borderRadius: inputBorderRadius, backgroundColor: '#FFF' },
            ],
          )}
        </View>
      </ScrollView>
    </TouchableWithoutFeedback>
  );
}

const styles = StyleSheet.create({
  phoneContainer: {
    flex: 1,
    width: '100%',
    backgroundColor: '#000',
  },
  content: {
    flex: 1,
  },
  phoneScrollContent: {
    paddingHorizontal: 26,
    paddingTop: 60,
    paddingBottom: 100,
  },
  scrollContent: {
    flexGrow: 1,
  },
  GlobalSearchContainer: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    flexDirection: 'column',
    paddingVertical: 40,
  },
  resultsArea: {
    overflow: 'hidden',
    backgroundColor: 'transparent',
    borderRadius: 34,
  },
  resultsHeader: {
    paddingHorizontal: 20,
    paddingTop: 20,
    paddingBottom: 12,
    borderBottomWidth: 1,
    borderBottomColor: '#2C2C2E',
  },
  resultsHeaderTitle: {
    fontSize: 15,
    fontWeight: '600',
    color: '#8E8E93',
  },
  resultsListContent: {
    padding: 20,
  },
  groupContainer: {
    marginBottom: 16,
  },
  groupTitle: {
    fontSize: 13,
    fontWeight: '400',
    color: '#8E8E93',
    marginBottom: 8,
    marginLeft: 4,
  },
  groupCard: {
    borderRadius: 26,
    backgroundColor: '#1C1C1E',
    overflow: 'hidden',
  },
  listItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    height: 52,
  },
  listItemDivider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: 60,
  },
  listItemLogo: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#2C2C2E',
  },
  listItemName: {
    flex: 1,
    fontSize: 17,
    color: '#FFF',
    marginLeft: 11,
    fontWeight: '400',
  },
  emptyState: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 40,
  },
  emptyStateText: {
    fontSize: 15,
    color: '#8E8E93',
  },
  searchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    width: '100%',
  },
  inputContainer: {
    flex: 1,
    position: 'relative',
    backgroundColor: '#FFF',
    ...HOST_SHADOWS.inputPill,
    flexDirection: 'row',
    alignItems: 'center',
  },
  input: {
    flex: 1,
    paddingHorizontal: 12,
    paddingVertical: 14,
    fontSize: 17,
    color: '#000',
  },
  phoneInputDock: {
    width: '100%',
    paddingBottom: 10,
  },
  phoneInputSafeArea: {
    width: '100%',
    backgroundColor: '#000',
  },
  phoneInputSection: {
    width: '100%',
    alignItems: 'stretch',
    justifyContent: 'flex-end',
    paddingTop: 6,
    paddingHorizontal: 26,
    paddingBottom: 10,
  },
  phoneInputContainer: {
    flex: 1,
    position: 'relative',
    height: 48,
    backgroundColor: '#FFF',
    borderRadius: 24,
    ...HOST_SHADOWS.inputPill,
    flexDirection: 'row',
    alignItems: 'center',
  },
  phoneInput: {
    flex: 1,
    height: 48,
    borderRadius: 24,
    paddingHorizontal: 12,
    paddingVertical: 0,
    fontSize: 17,
    color: '#000',
  },
  inputPrefixIcon: {
    marginLeft: 11,
  },
  clearButton: {
    width: 32,
    height: 32,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: 4,
  },
  standaloneCloseButton: {
    width: 48,
    height: 48,
    borderRadius: 24,
    borderWidth: 1,
    borderColor: '#ECECEF',
    backgroundColor: '#FFF',
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.inputPill,
  },
});
