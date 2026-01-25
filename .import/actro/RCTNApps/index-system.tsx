/**
 * System Apps Entry Point
 *
 * This bundle contains only system apps (StartPage, History, Bookmarks, Debug).
 * Used by XPC Service (RCTViewService) for rendering system tabs.
 *
 * Output: main.rctnapp.jsbundle
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

import React from 'react';
import { AppRegistry, LogBox, View, Text, NativeModules, Platform, StyleSheet } from 'react-native';

// Lazy imports for system apps
const HistoryApp = React.lazy(() => import('./src/system-apps/HistoryApp'));
const BookmarksApp = React.lazy(() => import('./src/system-apps/BookmarksApp'));
const DebugApp = React.lazy(() => import('./src/system-apps/DebugApp').then(m => ({ default: m.DebugApp })));
const StartPageApp = React.lazy(() => import('./src/system-apps/StartPageApp'));

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
  { error: Error | null }
> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error) {
    nerror(`💥 [ErrorBoundary] ${error.message}`);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <View style={styles.errorContainer}>
        <Text style={styles.errorTitle}>Render Error</Text>
        <Text style={styles.errorMessage}>{this.state.error.message}</Text>
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

const SystemAppRouter: React.FC<SystemAppProps> = (props) => {
  const tab = props?.tab;
  const { containerWidth, containerHeight, isOverviewMode, isLeftPanelVisible, isRightPanelVisible } = props;

  const hasActrScript = !!(tab?.unifiedScript || tab?.leftPanelScript || tab?.rightPanelScript || tab?.guestScript);
  nlog(`[SystemAppRouter] render: systemAppType=${tab?.systemAppType}, hasActrScript=${hasActrScript}, tabId=${tab?.id?.substring(0, 8)}, isOverviewMode=${isOverviewMode}, size=${containerWidth}x${containerHeight}`);

  // 🔑 ACTR app: has script but no systemAppType
  if (!tab?.systemAppType && hasActrScript && tab) {
    nlog(`[SystemAppRouter] Rendering ActrApp for tab=${tab.id?.substring(0, 8)}`);
    return (
      <View style={styles.container}>
        <ErrorBoundary>
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
        </ErrorBoundary>
      </View>
    );
  }

  // No systemAppType and no script - waiting for config
  if (!tab?.systemAppType) {
    return (
      <View style={styles.loading}>
        <Text style={styles.loadingText}>Waiting for tab config...</Text>
      </View>
    );
  }

  // 🔑 Log when startPage is about to be rendered
  if (tab.systemAppType === 'startPage') {
    nlog(`[SystemAppRouter] Rendering StartPageApp: isOverviewMode=${isOverviewMode}, size=${containerWidth}x${containerHeight}, bookmarks=${tab.bookmarks?.length ?? 0}`);
  }

  return (
    <View style={styles.container}>
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
});

// Global error handler
const originalErrorHandler = (globalThis as any).ErrorUtils?.getGlobalHandler?.();
(globalThis as any).ErrorUtils?.setGlobalHandler?.((error: any, isFatal: boolean) => {
  nerror(`🔴 [GLOBAL ERROR] ${isFatal ? 'FATAL' : ''}: ${error?.message || error}`);
});

// Register the app
AppRegistry.registerComponent('ActrApp', () => SystemAppRouter);
nlog('✅ [System Apps] ActrApp registered');
