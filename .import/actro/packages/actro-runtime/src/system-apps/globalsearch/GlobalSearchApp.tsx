/**
 * Global Search App - 全局搜索页面
 */
import React, {
  useState,
  useEffect,
  useMemo,
  useCallback,
} from 'react';
import {
  View,
  Text,
  StyleSheet,
  Image,
  useWindowDimensions,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from '../../components';
import { useBookmarkStore } from '../../actr-host/store/useBookmarkStore';
import { useUserStore } from '../../actr-host/store/useUserStore';
import { getTypographyStyle, globalSearch } from '../../theme/tokens';
import { TBookmarkItem } from '../../repository/BookmarkRepository';
import {
  openTab,
  getAllTabs,
  TabSummary,
} from '../../native-bridge/tab/TabModule';
import GlobalSearchPad from './GlobalSearchPad';
import GlobalSearchPhone from './GlobalSearchPhone';

type SearchResultType = 'active' | 'bookmark' | 'setting';
type SettingsPage = 'main' | 'accountDetail' | 'aboutActro';
type SettingsAboutPage = 'aboutApp' | 'userAgreement' | 'privacyPolicy' | 'icp';
type SettingsAction = 'feedback' | 'logout';

interface SearchResultItem {
  id: string;
  type: SearchResultType;
  name: string;
  searchKeywords?: string[];
  logo?: string;
  appId?: string;
  tabId?: string;
  url?: string;
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
    aboutName: 'Actro',
    aboutKeywords: ['Actro', '关于'],
    aboutAppName: '关于 Actro',
    aboutAppKeywords: ['关于 Actro', '关于'],
    icpName: 'ICP 备案',
    icpKeywords: ['ICP 备案', '备案'],
    logoutKeywords: ['退出登录', '退出'],
  },
  'zh-Hant': {
    settingsKeywords: ['設置', '偏好設置', '系統設置'],
    feedbackKeywords: ['吐個槽', '回饋', '檢舉', '意見'],
    aboutName: 'Actro',
    aboutKeywords: ['Actro', '關於'],
    aboutAppName: '關於 Actro',
    aboutAppKeywords: ['關於 Actro', '關於'],
    icpName: 'ICP 備案',
    icpKeywords: ['ICP 備案', '備案'],
    logoutKeywords: ['退出登入', '退出'],
  },
  en: {
    settingsKeywords: ['settings', 'preferences', 'system settings'],
    feedbackKeywords: ['feedback', 'report', 'support'],
    aboutName: 'Actro',
    aboutKeywords: ['actro', 'about'],
    aboutAppName: 'About Actro',
    aboutAppKeywords: ['about actro', 'about'],
    icpName: 'ICP Filing',
    icpKeywords: ['icp filing', 'icp'],
    logoutKeywords: ['logout', 'sign out'],
  },
  es: {
    settingsKeywords: ['configuracion', 'ajustes', 'preferencias'],
    feedbackKeywords: ['comentarios', 'reportes', 'opinion'],
    aboutName: 'Actro',
    aboutKeywords: ['actro', 'acerca de'],
    aboutAppName: 'Acerca de Actro',
    aboutAppKeywords: ['acerca de actro', 'acerca de'],
    icpName: 'Registro ICP',
    icpKeywords: ['registro icp', 'icp'],
    logoutKeywords: ['cerrar sesion'],
  },
  fr: {
    settingsKeywords: ['parametres', 'preferences', 'reglages'],
    feedbackKeywords: ['avis', 'signalement', 'retour'],
    aboutName: 'Actro',
    aboutKeywords: ['actro', 'a propos'],
    aboutAppName: 'A propos de Actro',
    aboutAppKeywords: ['a propos de actro', 'a propos'],
    icpName: 'Enregistrement ICP',
    icpKeywords: ['enregistrement icp', 'icp'],
    logoutKeywords: ['deconnexion'],
  },
  ja: {
    settingsKeywords: ['設定', '環境設定'],
    feedbackKeywords: ['フィードバック', '通報'],
    aboutName: 'Actro',
    aboutKeywords: ['actro', 'について'],
    aboutAppName: 'Actro について',
    aboutAppKeywords: ['actro について', 'について'],
    icpName: 'ICP 登録',
    icpKeywords: ['icp 登録', 'icp'],
    logoutKeywords: ['ログアウト'],
  },
  ko: {
    settingsKeywords: ['설정', '환경설정'],
    feedbackKeywords: ['피드백', '신고', '의견'],
    aboutName: 'Actro',
    aboutKeywords: ['actro', '정보'],
    aboutAppName: 'Actro 정보',
    aboutAppKeywords: ['actro 정보', '정보'],
    icpName: 'ICP 등록',
    icpKeywords: ['icp 등록', 'icp'],
    logoutKeywords: ['로그아웃'],
  },
  ru: {
    settingsKeywords: ['настроики', 'параметры'],
    feedbackKeywords: ['отзыв', 'жалоба'],
    aboutName: 'Actro',
    aboutKeywords: ['actro', 'о приложении'],
    aboutAppName: 'О Actro',
    aboutAppKeywords: ['о actro', 'о приложении'],
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
  const [searchQuery, setSearchQuery] = useState('');
  const [allTabs, setAllTabs] = useState<TabSummary[]>([]);
  const { width: screenWidth } = useWindowDimensions();
  const userInfo = useUserStore((s) => s.userInfo);
  const bookmarkGroups = useBookmarkStore((state) => state.groups);
  const loadBookmarks = useBookmarkStore((state) => state.loadBookmarks);
  const isLoggedIn = !!userInfo;
  const searchLocale = resolveSearchLocale(i18n.resolvedLanguage || i18n.language);
  const localizedSearchCopy = LOCALIZED_SETTING_SEARCH_COPY[searchLocale];
  const bookmarks = useMemo<TBookmarkItem[]>(
    () => bookmarkGroups.flatMap((group) => group.items),
    [bookmarkGroups],
  );

  const isPhone = screenWidth < 600;
  const resultsAreaSize = isPhone ? 'auto' : 500;
  const resultsHorizontalPadding = isPhone ? 16 : 40;
  const resultsCardRadius = 26;

  useEffect(() => {
    void loadBookmarks();
  }, [loadBookmarks]);

  useEffect(() => {
    // 活跃应用搜索改为基于页面打开时的 tab 快照做本地过滤，
    // 避免每次提交搜索都跨桥拉取一次，保持交互和书签搜索一致。
    getAllTabs().then((tabs) => {
      setAllTabs(tabs);
    });
  }, []);

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
      openSettingsWithDeepLink('aboutActro', aboutPage);
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
        icon: 'actro_gray',
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
        icon: 'actro_gray',
        action: () => openSettingsWithDeepLink('aboutActro'),
      },
      {
        id: 'setting-about-app',
        type: 'setting',
        name: localizedSearchCopy.aboutAppName,
        searchKeywords: localizedSearchCopy.aboutAppKeywords,
        icon: 'actro_gray',
        action: () => handleOpenAbout('aboutApp'),
      },
      {
        id: 'setting-user-agreement',
        type: 'setting',
        name: t('actr.auth.userAgreement'),
        searchKeywords: [t('actr.auth.userAgreement')],
        icon: 'actro_gray',
        action: () => handleOpenAbout('userAgreement'),
      },
      {
        id: 'setting-privacy-policy',
        type: 'setting',
        name: t('actr.auth.privacyPolicy'),
        searchKeywords: [t('actr.auth.privacyPolicy')],
        icon: 'actro_gray',
        action: () => handleOpenAbout('privacyPolicy'),
      },
      {
        id: 'setting-icp',
        type: 'setting',
        name: localizedSearchCopy.icpName,
        searchKeywords: localizedSearchCopy.icpKeywords,
        icon: 'actro_gray',
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

  const searchResults: SearchResultGroup[] = useMemo(() => {
    if (!searchQuery.trim()) return [];
    const q = searchQuery.toLowerCase();

    const activeResults: SearchResultItem[] = allTabs
      .filter((tab) => tab.title?.toLowerCase().includes(q))
      .map((tab) => ({
        id: `active-${tab.tabId}`,
        type: 'active' as const,
        name: tab.title,
        logo: tab.icon,
        tabId: tab.tabId,
        url: tab.appUrl,
      }));

    const bookmarkResults: SearchResultItem[] = bookmarks
      .filter((b) => b.instance_name?.toLowerCase().includes(q))
      .map((b) => ({
        id: `bookmark-${b.id}`,
        type: 'bookmark' as const,
        name: b.instance_name || '',
        logo: b.logo || undefined,
        appId: b.app_id,
        url: b.url || undefined,
      }));

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
  }, [searchQuery, allTabs, bookmarks, searchableSettings, t]);

  const handleItemPress = useCallback((item: SearchResultItem) => {
    if (item.type === 'setting') {
      item.action?.();
      return;
    }

    openTab({
      tabId: item.tabId ?? '',
      appUrl: item.url || '',
    });
  }, []);

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

  const renderResults = useCallback(() => {
    if (!searchQuery.trim()) {
      return null;
    }

    if (searchResults.length === 0) {
      return (
        <View style={styles.emptyState}>
          <Text style={styles.emptyStateTitle}>{t('actr.search.emptyStateTitle')}</Text>
          <Text style={styles.emptyStateDescription}>
            {t('actr.search.emptyStateDescription')}
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
  }, [searchQuery, searchResults, t]);

  if (isPhone) {
    return (
      <GlobalSearchPhone
        renderResults={renderResults}
        resultsHorizontalPadding={resultsHorizontalPadding}
        onSearchQueryChange={setSearchQuery}
      />
    );
  }

  return (
    <GlobalSearchPad
      renderResults={renderResults}
      screenWidth={screenWidth}
      resultsHorizontalPadding={resultsHorizontalPadding}
      resultsAreaSize={resultsAreaSize}
      resultsCardRadius={resultsCardRadius}
      onSearchQueryChange={setSearchQuery}
    />
  );
}

const styles = StyleSheet.create({
  groupContainer: {
    marginBottom: globalSearch.groupSpacing,
  },
  groupTitle: {
    ...getTypographyStyle(globalSearch.groupTitleTypographyVariant),
    color: '#8E8E93',
    marginBottom: globalSearch.groupTitleMarginBottom,
    marginLeft: globalSearch.groupTitleMarginLeft,
  },
  groupCard: {
    borderRadius: 26,
    backgroundColor: '#1C1C1E',
    overflow: 'hidden',
  },
  listItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: globalSearch.resultItemPaddingHorizontal,
    height: 52,
  },
  listItemDivider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: globalSearch.resultDividerInsetLeft,
  },
  listItemLogo: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#2C2C2E',
  },
  listItemName: {
    flex: 1,
    ...getTypographyStyle(globalSearch.resultNameTypographyVariant),
    color: '#FFF',
    marginLeft: globalSearch.resultItemTextGap,
  },
  emptyState: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: globalSearch.emptyStatePaddingVertical,
  },
  emptyStateTitle: {
    ...getTypographyStyle(globalSearch.emptyTitleTypographyVariant),
    color: '#FFF',
    textAlign: 'center',
  },
  emptyStateDescription: {
    marginTop: globalSearch.emptyStateDescriptionMarginTop,
    ...getTypographyStyle(globalSearch.emptyDescriptionTypographyVariant),
    color: '#8E8E93',
    textAlign: 'center',
  },
});
