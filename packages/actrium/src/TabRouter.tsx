/**
 * TabRouter - Top-level routing for Actro tabs
 *
 * Routes to:
 * - RuntimeApp: for .actr plugin tabs (via keel Engine)
 * - HistoryApp: for chrome://history
 * - BookmarksApp: for chrome://bookmarks
 */
import React, { useEffect } from 'react';
import { View, Text, StyleSheet, Platform } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { I18nextProvider, useTranslation } from 'react-i18next';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { KeyboardProvider } from 'react-native-keyboard-controller';
import './i18n'; // Ensure i18n is initialized
import type { TabRouterProps } from './runtime-host/types';
import { TabProvider } from './runtime-host/contexts/TabContext';
import { SettingsApp } from './system-apps';
import { GlobalAlertHost } from './components';

// Lazy imports to avoid circular dependencies
const RuntimeApp = React.lazy(() =>
  import('./runtime-host/features/app/RuntimeApp').then((m) => ({
    default: m.RuntimeApp,
  })),
);
const HistoryApp = React.lazy(() => import('./system-apps/HistoryApp'));
const BookmarksApp = React.lazy(
  () => import('./system-apps/bookmarks/BookmarksApp'),
);
const BookmarkTagsApp = React.lazy(
  () => import('./system-apps/BookmarkTagsApp'),
);
const GlobalSearchApp = React.lazy(
  () => import('./system-apps/globalsearch/GlobalSearchApp'),
);

// Props cache for bridge stability
let cachedValidProps: TabRouterProps | null = null;

// Loading fallback
const LoadingFallback = () => {
  const { t } = useTranslation();
  return (
    <View style={styles.loading}>
      <Text style={styles.loadingText}>{t('common.loading')}</Text>
    </View>
  );
};

export const TabRouter: React.FC<TabRouterProps> = (props) => {
  const content = (
    <I18nextProvider i18n={require('./i18n').default}>
      <TabRouterInner {...props} />
      <GlobalAlertHost />
    </I18nextProvider>
  );

  const keyboardProviderProps =
    Platform.OS === 'android'
      ? {
          // Android Hosts use edge-to-edge/Transparency system bar,Provider It is to be calculated by the immersion system bar,
          // Otherwise, the status bar height will be added to the root view, resulting in tab The top of the page is down in its entirety.
          statusBarTranslucent: true,
          navigationBarTranslucent: true,
          preserveEdgeToEdge: true,
        }
      : undefined;

  return (
    <GestureHandlerRootView style={styles.gestureRoot}>
      <SafeAreaProvider>
        <KeyboardProvider {...keyboardProviderProps}>
          {content}
        </KeyboardProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
};

const TabRouterInner: React.FC<TabRouterProps> = (props) => {
  // Single tab mode - each RootView renders one tab independently
  if (props?.singleTabMode && props?.tab) {
    const tab = props.tab;
    const {
      isLeftPanelVisible = true,
      isRightPanelVisible = true,
      containerWidth,
      containerHeight,
    } = props;

    // System app tab
    if (tab.systemAppType) {
      return (
        <View style={styles.container} key={`${tab.id}-${tab.reloadKey ?? 0}`}>
          <React.Suspense fallback={<LoadingFallback />}>
            <TabProvider tab={tab}>
              {tab.systemAppType === 'history' && (
                <HistoryApp
                  items={tab.historyItems ?? []}
                  groups={tab.historyGroups ?? []}
                />
              )}
              {tab.systemAppType === 'bookmarks' && <BookmarksApp />}
              {tab.systemAppType === 'bookmarkTags' && <BookmarkTagsApp />}
              {tab.systemAppType === 'settings' && (
                <SettingsApp systemAppPayload={props.systemAppPayload} />
              )}
              {tab.systemAppType === 'globalSearch' && <GlobalSearchApp />}
            </TabProvider>
          </React.Suspense>
        </View>
      );
    }

    // ACTR tab - Use flex: 1 Jean. Yoga AutoAdaptation RootView Dimensions
    // Avoid Visibility width/height Unnecessary trigger re-render
    const containerStyle = styles.container;

    const prefix = 'https://gate-actrium.actrium-beta.example/with/';
    const appUrl =
      props.appUrl || (props.appId ? prefix + props.appId : undefined);

    return (
      <View style={containerStyle} key={`${tab.id}-${tab.reloadKey ?? 0}`}>
        <React.Suspense fallback={<LoadingFallback />}>
          <RuntimeApp
            tabs={[{ ...tab, appUrl }]}
            currentTabId={tab.id}
            isLeftPanelVisible={isLeftPanelVisible}
            isRightPanelVisible={isRightPanelVisible}
            containerWidth={containerWidth}
            containerHeight={containerHeight}
          />
        </React.Suspense>
      </View>
    );
  }

  // Multi-tab mode (legacy) - Props stabilization (RN bridge may send empty props)
  const hasValidTabs = props?.tabs && props.tabs.length > 0;
  let effectiveProps = props;

  if (hasValidTabs) {
    cachedValidProps = { ...props };
  } else if (cachedValidProps) {
    effectiveProps = cachedValidProps;
  }

  const {
    tabs = [],
    currentTabId,
    isLeftPanelVisible = true,
    containerWidth,
    containerHeight,
  } = effectiveProps || {};

  if (!tabs.length) {
    return <View style={styles.empty} />;
  }

  const activeTabId = currentTabId ?? tabs[0]!.id;

  // Separate tabs by type
  const actrTabs = tabs.filter((t) => !t.systemAppType);
  const systemTabs = tabs.filter((t) => !!t.systemAppType);

  return (
    <View style={styles.container}>
      {/* Actr tabs - rendered by RuntimeApp */}
      {actrTabs.length > 0 && (
        <React.Suspense fallback={<LoadingFallback />}>
          <View
            style={[
              StyleSheet.absoluteFill,
              {
                display: actrTabs.some((t) => t.id === activeTabId)
                  ? 'flex'
                  : 'none',
              },
            ]}
          >
            <RuntimeApp
              tabs={actrTabs}
              currentTabId={activeTabId}
              isLeftPanelVisible={isLeftPanelVisible}
              containerWidth={containerWidth}
              containerHeight={containerHeight}
            />
          </View>
        </React.Suspense>
      )}

      {/* System tabs - rendered individually */}
      {systemTabs.map((tab) => {
        const isActive = tab.id === activeTabId;
        return (
          <View
            key={`${tab.id}-${tab.reloadKey ?? 0}`}
            style={[
              StyleSheet.absoluteFill,
              { display: isActive ? 'flex' : 'none' },
            ]}
          >
            <React.Suspense fallback={<LoadingFallback />}>
              <TabProvider tab={tab}>
                {tab.systemAppType === 'history' && (
                  <HistoryApp
                    items={tab.historyItems ?? []}
                    groups={tab.historyGroups ?? []}
                  />
                )}
                {tab.systemAppType === 'bookmarks' && <BookmarksApp />}
                {tab.systemAppType === 'bookmarkTags' && <BookmarkTagsApp />}
                {tab.systemAppType === 'settings' && (
                  <SettingsApp
                    systemAppPayload={effectiveProps.systemAppPayload}
                  />
                )}
                {tab.systemAppType === 'globalSearch' && <GlobalSearchApp />}
              </TabProvider>
            </React.Suspense>
          </View>
        );
      })}
    </View>
  );
};

const styles = StyleSheet.create({
  gestureRoot: {
    flex: 1,
  },
  container: {
    flex: 1,
    position: 'relative',
  },
  empty: {
    flex: 1,
    backgroundColor: '#111',
  },
  loading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#1a1a2a',
  },
  loadingText: {
    color: '#888',
    fontSize: 14,
  },
});

export default TabRouter;
