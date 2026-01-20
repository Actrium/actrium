/**
 * Actr Host Entry Point
 *
 * This bundle contains the Actr Host app (with rill/askit engine).
 * Used for rendering .actr plugin tabs.
 *
 * Output: actr.jsbundle
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

// Import ActrApp
const ActrApp = React.lazy(() => import('./src/actr-host/ActrApp').then(m => ({ default: m.ActrApp })));

const NativeLogger = NativeModules.NativeLogger;
const nlog = (msg: string) => { try { NativeLogger?.log?.(msg); } catch {} };
const nerror = (msg: string) => { try { NativeLogger?.error?.(msg); } catch {} };

nlog('📱 [Actr Host] Bundle loaded');

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

interface ActrHostProps {
  singleTabMode?: boolean;
  tab?: {
    id: string;
    title?: string;
    unifiedScript?: string;
    leftPanelScript?: string;
    rightPanelScript?: string;
    guestScript?: string;
    backgroundColor?: string;
    reloadKey?: number;
  };
  tabs?: any[];
  currentTabId?: string;
  isLeftPanelVisible?: boolean;
  isRightPanelVisible?: boolean;
  containerWidth?: number;
  containerHeight?: number;
}

const ActrHostRouter: React.FC<ActrHostProps> = (props) => {
  const { tab, tabs, currentTabId, isLeftPanelVisible, isRightPanelVisible, containerWidth, containerHeight } = props;

  nlog(`[ActrHostRouter] render: tabId=${tab?.id?.substring(0, 8) || currentTabId?.substring(0, 8)}`);

  // Single tab mode
  if (props.singleTabMode && tab) {
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

  // Multi-tab mode
  if (tabs && tabs.length > 0) {
    return (
      <View style={styles.container}>
        <ErrorBoundary>
          <React.Suspense fallback={<LoadingFallback />}>
            <ActrApp
              tabs={tabs}
              currentTabId={currentTabId || tabs[0].id}
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

  return (
    <View style={styles.loading}>
      <Text style={styles.loadingText}>Waiting for tab config...</Text>
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
AppRegistry.registerComponent('ActrApp', () => ActrHostRouter);
nlog('✅ [Actr Host] ActrApp registered');
