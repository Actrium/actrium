/**
 * TabRouter - Top-level routing for Canvas tabs
 *
 * Routes to:
 * - ActrApp: for .actr plugin tabs (via rill Engine)
 * - HistoryApp: for chrome://history
 * - BookmarksApp: for chrome://bookmarks
 */
import React, { useState, useEffect } from 'react';
import { View, Text, StyleSheet, DeviceEventEmitter } from 'react-native';
import type { TabRouterProps } from './actr-host/types';

// Lazy imports to avoid circular dependencies
const ActrApp = React.lazy(() => import('./actr-host/ActrApp').then(m => ({ default: m.ActrApp })));
const HistoryApp = React.lazy(() => import('./system-apps/HistoryApp'));
const BookmarksApp = React.lazy(() => import('./system-apps/BookmarksApp'));
const DebugApp = React.lazy(() => import('./system-apps/DebugApp').then(m => ({ default: m.DebugApp })));
const StartPageApp = React.lazy(() => import('./system-apps/StartPageApp'));

// Props cache for bridge stability
let cachedValidProps: TabRouterProps | null = null;

// Loading fallback - empty view to avoid "Loading..." text flash
const LoadingFallback = () => <View style={styles.loading} />;

export const TabRouter: React.FC<TabRouterProps> = (initialProps) => {
  // 🔑 使用 state 来存储动态更新的 props
  // appProperties 更新可能不可靠，所以通过 DeviceEventEmitter 接收更新
  const [dynamicProps, setDynamicProps] = useState<Partial<TabRouterProps>>({});

  useEffect(() => {
    console.warn('[TabRouter] Setting up DeviceEventEmitter listener for onPropsUpdate');

    const subscription = DeviceEventEmitter.addListener('onPropsUpdate', (newProps: any) => {
      // Log to native for visibility in IPC logs
      const { NativeModules } = require('react-native');
      const msg = `[TabRouter] onPropsUpdate received: size=${newProps?.containerWidth}x${newProps?.containerHeight}, isOverviewMode=${newProps?.isOverviewMode}`;
      NativeModules.NativeLogger?.log?.(msg);
      console.warn(msg);

      setDynamicProps(prev => ({
        ...prev,
        ...(newProps?.containerWidth !== undefined && { containerWidth: newProps.containerWidth }),
        ...(newProps?.containerHeight !== undefined && { containerHeight: newProps.containerHeight }),
        ...(newProps?.isOverviewMode !== undefined && { isOverviewMode: newProps.isOverviewMode }),
        ...(newProps?.isLeftPanelVisible !== undefined && { isLeftPanelVisible: newProps.isLeftPanelVisible }),
        ...(newProps?.isRightPanelVisible !== undefined && { isRightPanelVisible: newProps.isRightPanelVisible }),
      }));
    });

    return () => {
      console.warn('[TabRouter] Removing DeviceEventEmitter listener');
      subscription.remove();
    };
  }, []);

  // 合并 initial props 和 dynamic props
  const props = { ...initialProps, ...dynamicProps };

  // Debug: Log merged isOverviewMode
  const { NativeModules: NM } = require('react-native');
  NM.NativeLogger?.log?.(`[TabRouter] Merged props: initialProps.isOverviewMode=${initialProps?.isOverviewMode}, dynamicProps.isOverviewMode=${dynamicProps?.isOverviewMode}, final=${props?.isOverviewMode}`);

  // Debug: Log props received - use NativeModules for visibility
  const { NativeModules } = require('react-native');
  const NativeLogger = NativeModules.NativeLogger;
  const logMsg = `[TabRouter] render: singleTabMode=${props?.singleTabMode}, tabId=${props?.tab?.id?.substring(0,8)}, systemAppType=${props?.tab?.systemAppType}`;
  NativeLogger?.log?.(logMsg);
  console.log(logMsg);

  // Also log to stderr for XPC service visibility
  const propsJson = JSON.stringify({
    singleTabMode: props?.singleTabMode,
    tabId: props?.tab?.id,
    systemAppType: props?.tab?.systemAppType,
    title: props?.tab?.title,
  });
  console.warn(`[TabRouter] PROPS: ${propsJson}`);

  // Single tab mode - each RootView renders one tab independently
  if (props?.singleTabMode && props?.tab) {
    const tab = props.tab;
    const { isLeftPanelVisible = true, isRightPanelVisible = true, containerWidth, containerHeight, isOverviewMode } = props;

    // System app tab
    if (tab.systemAppType) {
      return (
        <View style={styles.container} key={`${tab.id}-${tab.systemAppType}-${tab.reloadKey ?? 0}`}>
          <React.Suspense fallback={<LoadingFallback />}>
            {tab.systemAppType === 'loading' && (
              <View style={styles.xpcLoading}>
                <Text style={styles.xpcLoadingTitle}>XPC Service Ready</Text>
                <Text style={styles.xpcLoadingSubtitle}>Waiting for tab config...</Text>
              </View>
            )}
            {tab.systemAppType === 'history' && (
              <HistoryApp
                items={tab.historyItems ?? []}
                groups={tab.historyGroups ?? []}
                containerWidth={containerWidth}
                containerHeight={containerHeight}
                isOverviewMode={isOverviewMode}
              />
            )}
            {tab.systemAppType === 'bookmarks' && (
              <BookmarksApp
                bookmarks={tab.bookmarks ?? []}
                containerWidth={containerWidth}
                containerHeight={containerHeight}
                isOverviewMode={isOverviewMode}
              />
            )}
            {tab.systemAppType === 'debug' && (
              <DebugApp
                tabId={tab.id}
                title={tab.title}
                backgroundColor={tab.backgroundColor}
                containerWidth={containerWidth}
                containerHeight={containerHeight}
              />
            )}
            {tab.systemAppType === 'startPage' && (
              <StartPageApp
                bookmarks={tab.bookmarks ?? []}
                recentHistory={tab.recentHistory ?? []}
                containerWidth={containerWidth}
                containerHeight={containerHeight}
                isOverviewMode={isOverviewMode}
              />
            )}
          </React.Suspense>
        </View>
      );
    }

    // ACTR tab - 使用 flex: 1 让 Yoga 自动适应 RootView 尺寸
    // 避免显式 width/height 触发不必要的 re-render
    const containerStyle = styles.container;

    return (
      <View style={containerStyle} key={`${tab.id}-${tab.reloadKey ?? 0}`}>
        <React.Suspense fallback={<LoadingFallback />}>
          <ActrApp
            tabs={[tab]}
            currentTabId={tab.id}
            isLeftPanelVisible={isLeftPanelVisible}
            isRightPanelVisible={isRightPanelVisible}
            containerWidth={containerWidth}
            containerHeight={containerHeight}
            isOverviewMode={isOverviewMode}
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

  const { tabs = [], currentTabId, isLeftPanelVisible = true, containerWidth, containerHeight, isOverviewMode } = effectiveProps || {};

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
          <View
            style={[
              StyleSheet.absoluteFill,
              { display: actrTabs.some(t => t.id === activeTabId) ? 'flex' : 'none' }
            ]}>
            <ActrApp
              tabs={actrTabs}
              currentTabId={activeTabId}
              isLeftPanelVisible={isLeftPanelVisible}
              containerWidth={containerWidth}
              containerHeight={containerHeight}
              isOverviewMode={isOverviewMode}
            />
          </View>
        </React.Suspense>
      )}

      {/* System tabs - rendered individually */}
      {systemTabs.map(tab => {
        const isActive = tab.id === activeTabId;
        return (
          <View
            key={`${tab.id}-${tab.systemAppType}-${tab.reloadKey ?? 0}`}
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
                  containerWidth={containerWidth}
                  containerHeight={containerHeight}
                  isOverviewMode={isOverviewMode}
                />
              )}
              {tab.systemAppType === 'bookmarks' && (
                <BookmarksApp
                  bookmarks={tab.bookmarks ?? []}
                  containerWidth={containerWidth}
                  containerHeight={containerHeight}
                  isOverviewMode={isOverviewMode}
                />
              )}
              {tab.systemAppType === 'debug' && (
                <DebugApp
                  tabId={tab.id}
                  title={tab.title}
                  backgroundColor={tab.backgroundColor}
                  containerWidth={containerWidth}
                  containerHeight={containerHeight}
                />
              )}
              {tab.systemAppType === 'startPage' && (
                <StartPageApp
                  bookmarks={tab.bookmarks ?? []}
                  recentHistory={tab.recentHistory ?? []}
                  containerWidth={containerWidth}
                  containerHeight={containerHeight}
                  isOverviewMode={isOverviewMode}
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
    backgroundColor: '#1f1f1f',
  },
  xpcLoading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#1f1f1f',
  },
  xpcLoadingTitle: {
    color: '#fff',
    fontSize: 20,
    fontWeight: 'bold',
  },
  xpcLoadingSubtitle: {
    color: '#93c5fd',
    fontSize: 12,
    marginTop: 8,
  },
});

export default TabRouter;
