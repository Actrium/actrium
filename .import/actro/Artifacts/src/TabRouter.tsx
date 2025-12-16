/**
 * TabRouter - Top-level routing for Canvas tabs
 *
 * Routes to:
 * - ActrApp: for .actr plugin tabs (via rill Engine)
 * - HistoryApp: for chrome://history
 * - BookmarksApp: for chrome://bookmarks
 */
import React from 'react';
import { View, Text, StyleSheet, NativeModules } from 'react-native';
import type { TabRouterProps, TabConfig } from './actr-host/types';

// Lazy imports to avoid circular dependencies
const ActrApp = React.lazy(() => import('./actr-host/ActrApp').then(m => ({ default: m.ActrApp })));
const HistoryApp = React.lazy(() => import('./system-apps/HistoryApp'));
const BookmarksApp = React.lazy(() => import('./system-apps/BookmarksApp'));

// Remote logging
const LOG_SERVER = 'http://localhost:9999/log';
const sendLog = (level: string, msg: string) => {
  try {
    const fetchFn = (globalThis as any).fetch as unknown;
    if (typeof fetchFn === 'function') {
      (fetchFn as any)(LOG_SERVER, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ level, msg }),
      })?.catch?.(() => {});
    }
  } catch {
    // ignore
  }
  try {
    NativeModules.NativeLogger?.[level === 'error' ? 'error' : level === 'warn' ? 'warn' : 'log']?.(msg);
  } catch {
    // ignore
  }
};
const nlog = (msg: string) => sendLog('log', msg);

// Props cache for bridge stability
let cachedValidProps: TabRouterProps | null = null;

nlog('[TabRouter] Module loaded');

// Loading fallback
const LoadingFallback = () => (
  <View style={styles.loading}>
    <Text style={styles.loadingText}>Loading...</Text>
  </View>
);

export const TabRouter: React.FC<TabRouterProps> = (props) => {
  nlog('[TabRouter] Render, props keys: ' + Object.keys(props || {}).join(', '));

  // Single tab mode - each RootView renders one tab independently
  if (props?.singleTabMode && props?.tab) {
    const tab = props.tab;
    const { isLeftPanelVisible = true, isRightPanelVisible = true } = props;
    nlog(`[TabRouter] Single tab mode: ${tab.title} (${tab.id?.substring(0, 8)})`);

    // System app tab
    if (tab.systemAppType) {
      return (
        <View style={styles.container}>
          <React.Suspense fallback={<LoadingFallback />}>
            {tab.systemAppType === 'history' && (
              <HistoryApp
                items={tab.historyItems ?? []}
                groups={tab.historyGroups ?? []}
              />
            )}
            {tab.systemAppType === 'bookmarks' && (
              <BookmarksApp
                bookmarks={tab.bookmarks ?? []}
              />
            )}
          </React.Suspense>
        </View>
      );
    }

    // ACTR tab - render single tab
    return (
      <View style={styles.container}>
        <React.Suspense fallback={<LoadingFallback />}>
          <ActrApp
            tabs={[tab]}
            currentTabId={tab.id}
            isLeftPanelVisible={isLeftPanelVisible}
            isRightPanelVisible={isRightPanelVisible}
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
    nlog('[TabRouter] Cached valid props (tabs: ' + props.tabs.length + ')');
  } else if (cachedValidProps) {
    effectiveProps = cachedValidProps;
    nlog('[TabRouter] Using cached props (tabs: ' + cachedValidProps.tabs.length + ')');
  }

  const { tabs = [], currentTabId, isLeftPanelVisible = true } = effectiveProps || {};

  if (!tabs.length) {
    nlog('[TabRouter] No tabs, rendering empty view');
    return <View style={styles.empty} />;
  }

  const activeTabId = currentTabId ?? tabs[0]!.id;

  // Separate tabs by type
  const actrTabs = tabs.filter(t => !t.systemAppType);
  const systemTabs = tabs.filter(t => !!t.systemAppType);

  nlog(`[TabRouter] Routing: ${actrTabs.length} actr tabs, ${systemTabs.length} system tabs, active=${activeTabId}`);

  return (
    <View style={styles.container}>
      {/* Actr tabs - rendered by ActrApp */}
      {actrTabs.length > 0 && (
        <React.Suspense fallback={<LoadingFallback />}>
          <View style={[
            StyleSheet.absoluteFill,
            { display: actrTabs.some(t => t.id === activeTabId) ? 'flex' : 'none' }
          ]}>
            <ActrApp
              tabs={actrTabs}
              currentTabId={activeTabId}
              isLeftPanelVisible={isLeftPanelVisible}
            />
          </View>
        </React.Suspense>
      )}

      {/* System tabs - rendered individually */}
      {systemTabs.map(tab => {
        const isActive = tab.id === activeTabId;
        return (
          <View
            key={tab.id}
            style={[
              StyleSheet.absoluteFill,
              { display: isActive ? 'flex' : 'none' }
            ]}
          >
            <React.Suspense fallback={<LoadingFallback />}>
              {tab.systemAppType === 'history' && (
                <HistoryApp
                  items={tab.historyItems ?? []}
                  groups={tab.historyGroups ?? []}
                />
              )}
              {tab.systemAppType === 'bookmarks' && (
                <BookmarksApp
                  bookmarks={tab.bookmarks ?? []}
                />
              )}
            </React.Suspense>
          </View>
        );
      })}
    </View>
  );
};

const styles = StyleSheet.create({
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
