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
  TouchableOpacity,
  Platform,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from '../actr-host/components/Icon';
import { HostIconButton } from '../actr-host/components/HostIconButton';
import SafeBottomView from '../actr-host/components/SafeBottomView';
import { useKeyboardInset } from '../hooks/useKeyboardInset';
import { HOST_SHADOWS } from '../actr-host/styles/shadows';
import { useAppStore } from '../actr-host/store/useAppStore';
import { useTabIdMapStore } from '../actr-host/store/useTabIdMapStore';
import { useUserStore } from '../actr-host/store/useUserStore';
import {
  BookmarkRepository,
  TBookmarkItem,
} from '../repository/BookmarkRepository';
import { openTab, exitSearch } from '../native-bridge/tab/TabModule';

// 搜索结果类型
type SearchResultType = 'active' | 'bookmark' | 'setting';
type SettingsPage = 'main' | 'accountDetail' | 'aboutAskAway';
type SettingsAboutPage = 'aboutApp' | 'userAgreement' | 'privacyPolicy' | 'icp';
type SettingsAction = 'feedback' | 'logout';

interface SearchResultItem {
  id: string;
  type: SearchResultType;
  name: string;
  searchKeywords?: string[];
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

type SearchLocale = 'zh-Hans' | 'zh-Hant' | 'en' | 'es' | 'fr' | 'ja' | 'ko' | 'ru';

interface LocalizedSettingSearchCopy {
  settingsKeywords: string[];
  feedbackKeywords: string[];
  aboutName: string;
  aboutKeywords: string[];
  aboutAppName: string;
  aboutAppKeywords: string[];
  icpName: string;
  icpKeywords: string[];
  logoutKeywords: string[];
}

const LOCALIZED_SETTING_SEARCH_COPY: Record<SearchLocale, LocalizedSettingSearchCopy> = {
  'zh-Hans': {
    settingsKeywords: ['设置', '偏好设置', '系统设置'],
    feedbackKeywords: ['吐个槽', '反馈', '举报', '意见'],
    aboutName: 'AskAway',
    aboutKeywords: ['AskAway', '关于'],
    aboutAppName: '关于 AskAway',
    aboutAppKeywords: ['关于 AskAway', '关于'],
    icpName: 'ICP 备案',
    icpKeywords: ['ICP 备案', '备案'],
    logoutKeywords: ['退出登录', '退出'],
  },
  'zh-Hant': {
    settingsKeywords: ['設置', '偏好設置', '系統設置'],
    feedbackKeywords: ['吐個槽', '回饋', '檢舉', '意見'],
    aboutName: 'AskAway',
    aboutKeywords: ['AskAway', '關於'],
    aboutAppName: '關於 AskAway',
    aboutAppKeywords: ['關於 AskAway', '關於'],
    icpName: 'ICP 備案',
    icpKeywords: ['ICP 備案', '備案'],
    logoutKeywords: ['退出登入', '退出'],
  },
  en: {
    settingsKeywords: ['settings', 'preferences', 'system settings'],
    feedbackKeywords: ['feedback', 'report', 'support'],
    aboutName: 'AskAway',
    aboutKeywords: ['askaway', 'about'],
    aboutAppName: 'About AskAway',
    aboutAppKeywords: ['about askaway', 'about'],
    icpName: 'ICP Filing',
    icpKeywords: ['icp filing', 'icp'],
    logoutKeywords: ['logout', 'sign out'],
  },
  es: {
    settingsKeywords: ['configuracion', 'ajustes', 'preferencias'],
    feedbackKeywords: ['comentarios', 'reportes', 'opinion'],
    aboutName: 'AskAway',
    aboutKeywords: ['askaway', 'acerca de'],
    aboutAppName: 'Acerca de AskAway',
    aboutAppKeywords: ['acerca de askaway', 'acerca de'],
    icpName: 'Registro ICP',
    icpKeywords: ['registro icp', 'icp'],
    logoutKeywords: ['cerrar sesion'],
  },
  fr: {
    settingsKeywords: ['parametres', 'preferences', 'reglages'],
    feedbackKeywords: ['avis', 'signalement', 'retour'],
    aboutName: 'AskAway',
    aboutKeywords: ['askaway', 'a propos'],
    aboutAppName: 'A propos de AskAway',
    aboutAppKeywords: ['a propos de askaway', 'a propos'],
    icpName: 'Enregistrement ICP',
    icpKeywords: ['enregistrement icp', 'icp'],
    logoutKeywords: ['deconnexion'],
  },
  ja: {
    settingsKeywords: ['設定', '環境設定'],
    feedbackKeywords: ['フィードバック', '通報'],
    aboutName: 'AskAway',
    aboutKeywords: ['askaway', 'について'],
    aboutAppName: 'AskAway について',
    aboutAppKeywords: ['askaway について', 'について'],
    icpName: 'ICP 登録',
    icpKeywords: ['icp 登録', 'icp'],
    logoutKeywords: ['ログアウト'],
  },
  ko: {
    settingsKeywords: ['설정', '환경설정'],
    feedbackKeywords: ['피드백', '신고', '의견'],
    aboutName: 'AskAway',
    aboutKeywords: ['askaway', '정보'],
    aboutAppName: 'AskAway 정보',
    aboutAppKeywords: ['askaway 정보', '정보'],
    icpName: 'ICP 등록',
    icpKeywords: ['icp 등록', 'icp'],
    logoutKeywords: ['로그아웃'],
  },
  ru: {
    settingsKeywords: ['настроики', 'параметры'],
    feedbackKeywords: ['отзыв', 'жалоба'],
    aboutName: 'AskAway',
    aboutKeywords: ['askaway', 'о приложении'],
    aboutAppName: 'О AskAway',
    aboutAppKeywords: ['о askaway', 'о приложении'],
    icpName: 'Регистрация ICP',
    icpKeywords: ['регистрация icp', 'icp'],
    logoutKeywords: ['выити'],
  },
};

function resolveSearchLocale(language?: string): SearchLocale {
  const normalizedLanguage = (language || '').toLowerCase();

  if (
    normalizedLanguage.includes('zh-hant') ||
    normalizedLanguage.includes('zh-tw') ||
    normalizedLanguage.includes('zh-hk') ||
    normalizedLanguage.includes('zh-mo')
  ) {
    return 'zh-Hant';
  }
  if (normalizedLanguage.includes('zh')) {
    return 'zh-Hans';
  }
  if (normalizedLanguage.startsWith('es')) {
    return 'es';
  }
  if (normalizedLanguage.startsWith('fr')) {
    return 'fr';
  }
  if (normalizedLanguage.startsWith('ja')) {
    return 'ja';
  }
  if (normalizedLanguage.startsWith('ko')) {
    return 'ko';
  }
  if (normalizedLanguage.startsWith('ru')) {
    return 'ru';
  }
  return 'en';
}

export default function GlobalSearchApp() {
  const { t, i18n } = useTranslation();
  const [inputText, setInputText] = useState('');
  const [searchQuery, setSearchQuery] = useState(''); // 回车后才更新
  const [bookmarks, setBookmarks] = useState<TBookmarkItem[]>([]);
  const inputRef = useRef<TextInput>(null);
  const { keyboardInset, isKeyboardVisible, dismissKeyboard } = useKeyboardInset();
  const { width: screenWidth } = useWindowDimensions();
  const apps = useAppStore((s) => s.apps);
  const tabIdMap = useTabIdMapStore((s) => s.tabIdMap);
  const userInfo = useUserStore((s) => s.userInfo);
  const isLoggedIn = !!userInfo;
  const searchLocale = resolveSearchLocale(i18n.resolvedLanguage || i18n.language);
  const localizedSearchCopy = LOCALIZED_SETTING_SEARCH_COPY[searchLocale];

  const isPhone = screenWidth < 600;
  // 中文注释：Android 的 Phone/Pad 都交给宿主处理键盘抬升，
  // GlobalSearch 仅在 iOS Pad 追加 RN 的 keyboardInset，避免 Android 端重复位移。
  const shouldApplyKeyboardInset = Platform.OS === 'ios' && !isPhone;
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
  const openSettingsWithDeepLink = useCallback(
    (
      page: SettingsPage,
      aboutPage?: SettingsAboutPage,
      settingsAction?: SettingsAction,
    ) => {
      openTab({
        systemAppType: 'settings',
        systemAppPayload: {
          page,
          aboutPage,
          action: settingsAction,
          token: `${Date.now()}-${Math.random()}`,
        },
      });
    },
    [],
  );
  const handleOpenSettings = useCallback(() => {
    openSettingsWithDeepLink('main');
  }, [openSettingsWithDeepLink]);
  const handleOpenAbout = useCallback(
    (aboutPage: SettingsAboutPage = 'aboutApp') => {
      openSettingsWithDeepLink('aboutAskAway', aboutPage);
    },
    [openSettingsWithDeepLink],
  );

  const searchableSettings: SearchResultItem[] = useMemo(() => {
    const items: SearchResultItem[] = [
      {
        id: 'setting-main',
        type: 'setting',
        name: t('actr.search.settings'),
        // 只保留当前界面语言的关键词，避免中英混搜导致误命中。
        searchKeywords: localizedSearchCopy.settingsKeywords,
        icon: 'askaway_gery',
        action: handleOpenSettings,
      },
      {
        id: 'setting-feedback',
        type: 'setting',
        name: t('actr.settings.feedback'),
        searchKeywords: localizedSearchCopy.feedbackKeywords,
        icon: 'feedback',
        action: () => openSettingsWithDeepLink('main', undefined, 'feedback'),
      },
      {
        id: 'setting-about',
        type: 'setting',
        name: localizedSearchCopy.aboutName,
        searchKeywords: localizedSearchCopy.aboutKeywords,
        icon: 'askaway_gery',
        action: () => openSettingsWithDeepLink('aboutAskAway'),
      },
      {
        id: 'setting-about-app',
        type: 'setting',
        name: localizedSearchCopy.aboutAppName,
        searchKeywords: localizedSearchCopy.aboutAppKeywords,
        icon: 'askaway_gery',
        action: () => handleOpenAbout('aboutApp'),
      },
      {
        id: 'setting-user-agreement',
        type: 'setting',
        name: t('actr.auth.userAgreement'),
        searchKeywords: [t('actr.auth.userAgreement')],
        icon: 'askaway_gery',
        action: () => handleOpenAbout('userAgreement'),
      },
      {
        id: 'setting-privacy-policy',
        type: 'setting',
        name: t('actr.auth.privacyPolicy'),
        searchKeywords: [t('actr.auth.privacyPolicy')],
        icon: 'askaway_gery',
        action: () => handleOpenAbout('privacyPolicy'),
      },
      {
        id: 'setting-icp',
        type: 'setting',
        name: localizedSearchCopy.icpName,
        searchKeywords: localizedSearchCopy.icpKeywords,
        icon: 'askaway_gery',
        action: () => handleOpenAbout('icp'),
      },
    ];

    if (isLoggedIn) {
      items.push({
        id: 'setting-logout',
        type: 'setting',
        name: t('actr.settings.logout'),
        searchKeywords: localizedSearchCopy.logoutKeywords,
        icon: 'logout',
        action: () => openSettingsWithDeepLink('main', undefined, 'logout'),
      });
    }

    return items;
  }, [
    t,
    handleOpenSettings,
    handleOpenAbout,
    openSettingsWithDeepLink,
    isLoggedIn,
    localizedSearchCopy,
  ]);

  // 搜索结果
  const searchResults: SearchResultGroup[] = useMemo(() => {
    if (!searchQuery.trim()) return [];
    const q = searchQuery.toLowerCase();

    // 1. 搜索活跃应用
    const activeResults: SearchResultItem[] = Object.values(apps)
      .filter((app) => app.app_name?.toLowerCase().includes(q))
      .map((app) => {
        const tabId = Object.keys(tabIdMap).find(
          (key) => tabIdMap[key] === app.app_id,
        );
        return {
          id: `active-${app.app_id}`,
          type: 'active' as const,
          name: app.app_name,
          logo: app.info?.logo,
          appId: app.app_id,
          tabId,
        };
      });

    // 2. 搜索收藏
    const bookmarkResults: SearchResultItem[] = bookmarks
      .filter((b) => b.app_name?.toLowerCase().includes(q))
      .map((b) => ({
        id: `bookmark-${b.id}`,
        type: 'bookmark' as const,
        name: b.app_name || '',
        logo: b.logo || undefined,
        appId: b.app_id,
        url: b.url || undefined,
      }));

    // 3. 搜索设置
    const settingResults = searchableSettings.filter((s) => {
      if (s.name.toLowerCase().includes(q)) return true;
      return (s.searchKeywords || []).some((keyword) =>
        keyword.toLowerCase().includes(q),
      );
    });

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
  }, [searchQuery, apps, bookmarks, searchableSettings, t, tabIdMap]);

  // 回车触发搜索
  const handleSubmit = useCallback(() => {
    setSearchQuery(inputText.trim());
    dismissKeyboard();
  }, [inputText, dismissKeyboard]);

  const handleClearSearch = useCallback(() => {
    setInputText('');
    setSearchQuery('');
  }, []);

  const handleCloseSearch = useCallback(() => {
    dismissKeyboard();

    // 键盘和页面退出同帧触发时，Native 往往会直接执行关闭，导致键盘来不及先收起。
    if (isKeyboardVisible) {
      setTimeout(() => {
        exitSearch();
      }, 180);
      return;
    }

    exitSearch();
  }, [dismissKeyboard, isKeyboardVisible]);

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
      exitSearch();
    }
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
      return null;
    }

    if (searchResults.length === 0) {
      return (
        <View style={styles.emptyState}>
          <Text style={styles.emptyStateTitle}>无结果</Text>
          <Text style={styles.emptyStateDescription}>
            无结果。请尝试新搜索词。
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
          <HostIconButton
            iconType='input_clear'
            size={32}
            iconSize={19}
            iconColor='#666'
            backgroundColor='transparent'
            style={styles.clearButton}
            onPress={handleClearSearch}
          />
        )}
      </View>
      <HostIconButton
        iconType='close'
        size={48}
        iconSize={16}
        iconColor='#404040'
        backgroundColor='#FFF'
        style={styles.standaloneCloseButton}
        onPress={handleCloseSearch}
      />
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
          { paddingBottom: shouldApplyKeyboardInset ? keyboardInset : 0 },
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
            <ScrollView
              style={{ flex: 1 }}
              contentContainerStyle={styles.resultsListContent}
              showsVerticalScrollIndicator={false}
            >
              {renderResults()}
            </ScrollView>
          </View>

          {/* 中文注释：Pad 端输入区单独占一个底部锚点容器，
              通过 marginTop: 'auto' 把输入框稳定压到页面底部，避免继续跟随整体内容垂直居中。 */}
          <View style={[styles.padInputSection, { width: inputWidth }]}>
            {renderSearchInput(
              [
                styles.input,
                { borderRadius: inputBorderRadius },
              ],
              [
                styles.inputContainer,
                { borderRadius: inputBorderRadius, backgroundColor: '#FFF' },
              ],
            )}
          </View>
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
    flexDirection: 'column',
    paddingTop: 40,
    paddingBottom: 16,
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
  emptyStateTitle: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '500',
    color: '#FFF',
    textAlign: 'center',
  },
  emptyStateDescription: {
    marginTop: 8,
    fontSize: 14,
    lineHeight: 20,
    color: '#8E8E93',
    textAlign: 'center',
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
  padInputSection: {
    width: '100%',
    marginTop: 'auto',
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
