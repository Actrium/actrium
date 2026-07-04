/**
 * System Apps Entry Point
 *
 * This bundle contains system apps hosted by the macOS XPC view service.
 * Used by XPC Service (RCTViewService) for rendering system tabs.
 *
 * Output: system.jsbundle
 */

// Ensure console.assert exists as early as possible
try {
  const c: any = (globalThis as any).console || ((globalThis as any).console = {} as any);
  if (typeof c.assert !== 'function') {
    c.assert = function (expression: any, ...args: any[]) {
      if (!expression) {
        try {
          if (typeof c.error === 'function') {
            c.error('Assertion failed:', ...args);
          }
        } catch {}
      }
    };
  }
} catch {}

import React, { useState, useEffect, useCallback } from 'react';
import { AppRegistry, LogBox, View, Text, NativeModules, Platform, StyleSheet, DeviceEventEmitter, LayoutChangeEvent } from 'react-native';

// Lazy imports for system apps
const HistoryApp = React.lazy(() => import('./src/system-apps/HistoryApp'));
const BookmarksApp = React.lazy(() => import('./src/system-apps/bookmarks/BookmarksApp'));
const BookmarkTagsApp = React.lazy(() => import('./src/system-apps/BookmarkTagsApp'));
const SettingsApp = React.lazy(() => import('./src/system-apps/SettingsApp'));
const GlobalSearchApp = React.lazy(() => import('./src/system-apps/globalsearch/GlobalSearchApp'));

// Lazy import for ACTR apps (with unifiedScript)
const ActrApp = React.lazy(() => import('./src/actr-host/ActrApp').then(m => ({ default: m.ActrApp })));

const NativeLogger = NativeModules.NativeLogger;
const nlog = (msg: string) => { try { NativeLogger?.log?.(msg); } catch {} };
const nerror = (msg: string) => { try { NativeLogger?.error?.(msg); } catch {} };

nlog('📱 [System Apps] Bundle loaded');

// Disable LogBox on macOS to avoid CGColor crashes
if (Platform.OS === 'macos') {
  LogBox.ignoreAllLogs(true);
}

// Loading fallback
const LoadingFallback = () => <View style={styles.loading} />;

// Error boundary
class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null; componentStack: string | null }
> {
  state: { error: Error | null; componentStack: string | null } = { error: null, componentStack: null };

  static getDerivedStateFromError(error: Error) {
    return { error, componentStack: null };
  }

  componentDidCatch(error: Error, info: any) {
    const componentStack = typeof info?.componentStack === 'string' ? info.componentStack : null;
    this.setState({ error, componentStack });
    const stack = (error as any)?.stack ?? '';
    const msg = `💥 [ErrorBoundary] ${error.message}\n${stack}\n${componentStack ?? ''}`;
    nerror(msg);
    try { console.error(msg); } catch {}
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <View style={styles.errorContainer}>
        <Text style={styles.errorTitle}>Render Error</Text>
        <Text style={styles.errorMessage}>{this.state.error.message}</Text>
        {this.state.error.stack ? (
          <Text style={styles.errorStack}>{this.state.error.stack}</Text>
        ) : null}
        {this.state.componentStack ? (
          <Text style={styles.componentStack}>{this.state.componentStack}</Text>
        ) : null}
      </View>
    );
  }
}

interface SystemAppProps {
  singleTabMode?: boolean;
  tab?: {
    id: string;
    title?: string;
    systemAppType?: string;
    unifiedScript?: string;  // ACTR app script
    leftPanelScript?: string;
    rightPanelScript?: string;
    guestScript?: string;
    backgroundColor?: string;
    bookmarks?: any[];
    recentHistory?: any[];
    historyItems?: any[];
    historyGroups?: any[];
    reloadKey?: number;
  };
  containerWidth?: number;
  containerHeight?: number;
  isOverviewMode?: boolean;
  isLeftPanelVisible?: boolean;
  isRightPanelVisible?: boolean;
}

const SystemAppRouter: React.FC<SystemAppProps> = (initialProps) => {
  // 🔑 Use onLayout for live container size — most reliable on macOS
  // (appProperties, DeviceEventEmitter, useWindowDimensions all unreliable on macOS XPC)
  const [layoutSize, setLayoutSize] = useState<{ width: number; height: number } | null>(null);

  const handleLayout = useCallback((event: LayoutChangeEvent) => {
    const { width, height } = event.nativeEvent.layout;
    if (width > 0 && height > 0) {
      setLayoutSize(prev => {
        if (prev && Math.abs(prev.width - width) < 1 && Math.abs(prev.height - height) < 1) {
          return prev; // avoid re-render for sub-pixel changes
        }
        nlog(`[SystemAppRouter] onLayout: ${Math.round(width)}x${Math.round(height)}`);
        return { width, height };
      });
    }
  }, []);

  // 🔑 Dynamic props via DeviceEventEmitter (for non-size props like isOverviewMode)
  const [dynamicProps, setDynamicProps] = useState<Partial<SystemAppProps>>({});

  useEffect(() => {
    const subscription = DeviceEventEmitter.addListener('onPropsUpdate', (newProps: any) => {
      nlog(`[SystemAppRouter] onPropsUpdate: isOverviewMode=${newProps?.isOverviewMode}, leftPanel=${newProps?.isLeftPanelVisible}, rightPanel=${newProps?.isRightPanelVisible}`);
      setDynamicProps(prev => ({
        ...prev,
        isOverviewMode: newProps?.isOverviewMode ?? prev.isOverviewMode,
        isLeftPanelVisible: newProps?.isLeftPanelVisible ?? prev.isLeftPanelVisible,
        isRightPanelVisible: newProps?.isRightPanelVisible ?? prev.isRightPanelVisible,
      }));
    });
    return () => subscription.remove();
  }, []);

  // 🔑 Container size: prefer onLayout (live) > initial props
  const containerWidth = layoutSize?.width ?? initialProps?.containerWidth;
  const containerHeight = layoutSize?.height ?? initialProps?.containerHeight;

  // Merge initial props with dynamic updates (except size which comes from onLayout)
  const props = { ...initialProps, ...dynamicProps };
  const tab = props?.tab;
  const { isOverviewMode, isLeftPanelVisible, isRightPanelVisible } = props;

  const hasActrScript = !!(tab?.unifiedScript || tab?.leftPanelScript || tab?.rightPanelScript || tab?.guestScript);
  nlog(`[SystemAppRouter] render: systemAppType=${tab?.systemAppType}, hasActrScript=${hasActrScript}, tabId=${tab?.id?.substring(0, 8)}, isOverviewMode=${isOverviewMode}, size=${containerWidth}x${containerHeight}`);

  // 🔑 ACTR app: has script but no systemAppType
  if (!tab?.systemAppType && hasActrScript && tab) {
    nlog(`[SystemAppRouter] Rendering ActrApp for tab=${tab.id?.substring(0, 8)}`);
    return (
      <View style={styles.container} onLayout={handleLayout}>
        <ErrorBoundary>
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
        </ErrorBoundary>
      </View>
    );
  }

  // No systemAppType and no script - waiting for config
  if (!tab?.systemAppType) {
    return (
      <View style={styles.loading} onLayout={handleLayout}>
        <Text style={styles.loadingText}>Waiting for tab config...</Text>
      </View>
    );
  }

  return (
    <View style={styles.container} onLayout={handleLayout}>
      <ErrorBoundary>
        <React.Suspense fallback={<LoadingFallback />}>
          {tab.systemAppType === 'loading' && (
            <View style={styles.loading}>
              <Text style={styles.loadingTitle}>XPC Service Ready</Text>
              <Text style={styles.loadingSubtitle}>Waiting for tab config...</Text>
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
            <BookmarksApp />
          )}
          {tab.systemAppType === 'bookmarkTags' && (
            <BookmarkTagsApp />
          )}
          {tab.systemAppType === 'settings' && (
            <SettingsApp />
          )}
          {tab.systemAppType === 'globalSearch' && (
            <GlobalSearchApp />
          )}
          {!['loading', 'history', 'bookmarks', 'bookmarkTags', 'settings', 'globalSearch'].includes(tab.systemAppType) && (
            <View style={styles.loading}>
              <Text style={styles.loadingTitle}>Unsupported system app</Text>
              <Text style={styles.loadingSubtitle}>{tab.systemAppType}</Text>
            </View>
          )}
        </React.Suspense>
      </ErrorBoundary>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  loading: {
    flex: 1,
    backgroundColor: '#1f1f1f',
    justifyContent: 'center',
    alignItems: 'center',
  },
  loadingText: {
    color: '#666',
    fontSize: 14,
  },
  loadingTitle: {
    color: '#fff',
    fontSize: 20,
    fontWeight: 'bold',
  },
  loadingSubtitle: {
    color: '#93c5fd',
    fontSize: 12,
    marginTop: 8,
  },
  errorContainer: {
    flex: 1,
    backgroundColor: '#2b0b0b',
    padding: 16,
  },
  errorTitle: {
    color: '#fff',
    fontSize: 18,
    fontWeight: '700',
    marginBottom: 8,
  },
  errorMessage: {
    color: '#fff',
    fontSize: 13,
  },
  errorStack: {
    color: '#cbd5e1',
    fontSize: 10,
    marginTop: 10,
  },
  componentStack: {
    color: '#ddd',
    fontSize: 10,
    marginTop: 12,
  },
});

// Global error handler
try {
  const ErrorUtils = (globalThis as any).ErrorUtils;
  const originalErrorHandler = ErrorUtils?.getGlobalHandler?.();
  ErrorUtils?.setGlobalHandler?.((error: any, isFatal: boolean) => {
    try {
      const msg = String(error?.message ?? error ?? 'unknown error');
      const stack = typeof error?.stack === 'string' ? error.stack : '';
      const line = `🔴 [GLOBAL ERROR] ${isFatal ? 'FATAL' : ''}: ${msg}\n${stack}`;
      nerror(line);
      try { console.error(line); } catch {}
    } catch {}
    try {
      originalErrorHandler?.(error, isFatal);
    } catch {}
  });
} catch {}

// Register the app
AppRegistry.registerComponent('ActrApp', () => SystemAppRouter);
nlog('✅ [System Apps] ActrApp registered');
