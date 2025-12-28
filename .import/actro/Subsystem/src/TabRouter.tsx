/**
 * TabRouter - Top-level routing for Canvas tabs
 *
 * Routes to:
 * - ActrApp: for .actr plugin tabs (via rill Engine)
 * - HistoryApp: for chrome://history
 * - BookmarksApp: for chrome://bookmarks
 */
import React from 'react';
import { View, Text, StyleSheet } from 'react-native';
import type { TabRouterProps, TabConfig } from './actr-host/types';
import { logger } from './logger';

const log = logger.child('TabRouter');

// Lazy imports to avoid circular dependencies
const ActrApp = React.lazy(() => import('./actr-host/ActrApp').then(m => ({ default: m.ActrApp })));
const HistoryApp = React.lazy(() => import('./system-apps/HistoryApp'));
const BookmarksApp = React.lazy(() => import('./system-apps/BookmarksApp'));

// Props cache for bridge stability
let cachedValidProps: TabRouterProps | null = null;

// Loading fallback
const LoadingFallback = () => (
  <View style={styles.loading}>
    <Text style={styles.loadingText}>Loading...</Text>
  </View>
);

// Track previous containerWidth for logging
let prevContainerWidth: number | undefined;

export const TabRouter: React.FC<TabRouterProps> = (props) => {
  // Log containerWidth changes
  if (props?.containerWidth !== prevContainerWidth) {
    log.debug(`[ResizeFlow] TabRouter received containerWidth: ${prevContainerWidth} -> ${props?.containerWidth}`);
    prevContainerWidth = props?.containerWidth;
  }

  // Single tab mode - each RootView renders one tab independently
  if (props?.singleTabMode && props?.tab) {
    const tab = props.tab;
    const { isLeftPanelVisible = true, isRightPanelVisible = true, containerWidth, containerHeight } = props;

    // System app tab
    if (tab.systemAppType) {
      return (
        <View style={styles.container} key={`${tab.id}-${tab.reloadKey ?? 0}`}>
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
      <View style={styles.container} key={`${tab.id}-${tab.reloadKey ?? 0}`}>
        <React.Suspense fallback={<LoadingFallback />}>
          <ActrApp
            tabs={[tab]}
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

  const { tabs = [], currentTabId, isLeftPanelVisible = true, containerWidth, containerHeight } = effectiveProps || {};

  if (!tabs.length) {
    return <View style={styles.empty} />;
  }

  const activeTabId = currentTabId ?? tabs[0]!.id;

  // Separate tabs by type
  const actrTabs = tabs.filter(t => !t.systemAppType);
  const systemTabs = tabs.filter(t => !!t.systemAppType);

  return (
    <View style={styles.container}>
      {/* Actr tabs - rendered by ActrApp */}
      {actrTabs.length > 0 && (
        <React.Suspense fallback={<LoadingFallback />}>
          <View style={[
            // 使用显式尺寸而非 absoluteFill，确保缩小时能正确响应
            {
              position: 'absolute',
              left: 0,
              top: 0,
              right: 0,
              bottom: 0,
              width: containerWidth,
              height: containerHeight,
            },
            { display: actrTabs.some(t => t.id === activeTabId) ? 'flex' : 'none' }
          ]}>
            <ActrApp
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
      {systemTabs.map(tab => {
        const isActive = tab.id === activeTabId;
        return (
          <View
            key={`${tab.id}-${tab.reloadKey ?? 0}`}
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
