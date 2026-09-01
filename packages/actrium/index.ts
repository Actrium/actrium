// 🔴 Debugging at the earliest - In any of the import Before
const g = globalThis as any;
g.__ACTR_BUNDLE_LOADED__ = true;
g.__ACTR_BUNDLE_TIME__ = Date.now();

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

// 🔴 Debug polyfill Status
// Keep a minimal startup marker through unified logger.
import { logger } from './src/logger';
const log = logger.child('index');
log.debug('console.assert type (after guard): ' + typeof console.assert);
import { initDatabase } from './src/libs/Sqlite.init';
initDatabase();

// Close Reanimated strict Mode warning,@legendapp/list/keyboard Internal reading during rendering shared value This warning will be triggered.
import { configureReanimatedLogger } from 'react-native-reanimated';
configureReanimatedLogger({ strict: false });

import 'react-native-gesture-handler';
import 'react-native-get-random-values';
import NetInfo from '@react-native-community/netinfo';

import React from 'react';
import { AppRegistry, LogBox, View, Text, NativeModules, Platform } from 'react-native';
const PromiseRejectionTracking = require('promise/setimmediate/rejection-tracking');

// iOS Default Native reachability Yes. Wi-Fi The situation is unstable in the off-grid scene. NetInfo Your own access detection.
NetInfo.configure({
  useNativeReachability: false,
  // Wi-Fi The interface is still in place, but when the offline is broken, it is not possible to get the connection.NetInfo Only active detection can detect anomalies.
  // Default 60 The second round is too slow. It's on the ground. 3 sec, avoids prolonged non-upgrading of top network status.
  reachabilityLongTimeout: 3 * 1000,
  reachabilityShortTimeout: 3 * 1000,
  reachabilityRequestTimeout: 3 * 1000,
});

// 🔴 Unified Log Tool - Output To:
// 1. Remote Log Server (actr log server, Cross-platform)
// 2. Primary control (Pass. NativeLogger, Only iOS/macOS)
// Install as early as possible console Interceptor, make sure you follow up. console.xxx It's all on a remote basis. log server
import { installConsoleInterceptor } from './src/devtools-console';
installConsoleInterceptor();

const NativeLogger = NativeModules.NativeLogger;

const sendLog = (level: string, msg: string) => {
  // Remote Log Forward By devtools-console.ts It's... console The interceptors are all handled,
  // Here it goes. console Output. The interceptor will automatically transmit to a remote. log server.
  try {
    if (level === 'error') console.error(msg);
    else if (level === 'warn') console.warn(msg);
    else console.log(msg);
  } catch {
    // ignore
  }
};

const nlog = (msg: string) => sendLog('log', msg);
const nwarn = (msg: string) => sendLog('warn', msg);
const nerror = (msg: string) => sendLog('error', msg);

// Harmonize capture of unprocessed Promise Rejects, and avoids the loss of a walk-in error only when running.
PromiseRejectionTracking.enable({
  allRejections: true,
  onUnhandled: (id: number, error: unknown) => {
    const errorText =
      error instanceof Error
        ? `${error.message}\n${error.stack ?? ''}`
        : String(error);
    nerror(`🔴 [UNHANDLED PROMISE] id=${id}\n${errorText}`);
  },
  onHandled: (id: number) => {
    nwarn(`🟡 [PROMISE HANDLED LATE] id=${id}`);
  },
});

nlog('📱 Bundle Implementation');
nlog('📦 NativeModules keys: ' + Object.keys(NativeModules || {}).slice(0, 15).join(', '));
nlog('📦 NativeLogger Available: ' + (NativeLogger ? 'YES' : 'NO'));

// 🔴 Authenticated JS Normal execution, remove Alert
nlog('🏁 JS Bundle Under implementation...');

// Only specific warnings are disabled
const DISABLE_LOGBOX_ON_MACOS = true;
if (DISABLE_LOGBOX_ON_MACOS && Platform.OS === 'macos') {
  // RN macOS: LogBox Possible trigger on some versions `*** NULL CGColor argument in colorWithCGColor:.` Crash
  // Disable whole-system first to avoid frequent interruptions of the development period; the log will still pass console / NativeLogger Output.
  LogBox.ignoreAllLogs(true);
} else {
  LogBox.ignoreLogs([
    'Non-serializable values were found in the navigation state',
    'Sending `onAnimatedValueUpdate` with no listeners registered',
  ]);
}

// 🔴 Temporary test component - Verify basic rendering links
const TestComponent = (props: any) => {
  nlog('🧪 [TestComponent] Render Start');
  nlog('🧪 [TestComponent] props keys: ' + Object.keys(props || {}).join(', '));

  const tabs = props?.tabs || [];
  const currentTabId = props?.currentTabId;
  const currentTab = tabs.find((t: any) => t.id === currentTabId) || tabs[0];

  nlog('🧪 [TestComponent] tabsNumber: ' + tabs.length + ', currentTab: ' + (currentTab?.title || 'N/A'));

  return React.createElement(
    View,
    { style: { flex: 1, backgroundColor: '#ff00ff', padding: 20 } }, // Purple Background
    React.createElement(Text, { style: { color: '#fff', fontSize: 28, fontWeight: 'bold', marginBottom: 16 } },
      '🧪 TEST COMPONENT'),
    React.createElement(Text, { style: { color: '#fff', fontSize: 16, marginBottom: 8 } },
      `Tabs: ${tabs.length}`),
    React.createElement(Text, { style: { color: '#fff', fontSize: 16, marginBottom: 8 } },
      `Current Tab: ${currentTab?.title || 'N/A'}`),
    React.createElement(Text, { style: { color: '#0ff', fontSize: 14, marginTop: 20 } },
      'If you see this purple background, it means the basic link is working.')
  );
};

// Import Full Version ActrApp Component (Use keel/askit)
let Component;
const USE_TEST_COMPONENT = false; // 🟢 Cut back to the formal component test logic

// 🔴 Global props Cache - Prevention React Native bridge Empty props
let lastValidProps: any = null;

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
    nerror(`💥 [ErrorBoundary] ${error.message}\n${error.stack ?? ''}\n${componentStack ?? ''}`);
  }

  render() {
    if (!this.state.error) return this.props.children as any;
    const error = this.state.error as Error;
    return React.createElement(
      View,
      { style: { flex: 1, backgroundColor: '#2b0b0b', padding: 16 } },
      React.createElement(
        Text,
        { style: { color: '#fff', fontSize: 18, fontWeight: '700', marginBottom: 8 } },
        'Render Error'
      ),
      React.createElement(
        Text,
        { style: { color: '#fff', fontSize: 13 } },
        String(error.message || error)
      ),
      this.state.componentStack
        ? React.createElement(
            Text,
            { style: { color: '#ddd', fontSize: 11, marginTop: 12 } },
            this.state.componentStack
          )
        : null
    );
  }
}

if (USE_TEST_COMPONENT) {
  Component = TestComponent;
  nlog('🧪 Use test component');
} else {
  try {
    nlog('📦 Start Import TabRouter...');
    const module = require('./src/TabRouter');
    nlog('📦 TabRouter Modules: ' + Object.keys(module || {}).join(', '));
    Component = module.TabRouter || module.default;
    nlog('✅ TabRouter Import success, type: ' + typeof Component);

    // Packaging component: Adding an error boundary
    const OriginalComponent = Component;
    Component = (props: any) => {
      nlog('🎨 [Wrapper] TabRouter Start Rendering');
      // console.error(props.isLeftPanelVisible, '????')
      try {
        return React.createElement(ErrorBoundary, null, React.createElement(OriginalComponent, props));
      } catch (renderError: any) {
        nerror('❌ [Wrapper] TabRouter Rendering failed: ' + renderError?.message);
        return React.createElement(
          View,
          { style: { flex: 1, backgroundColor: '#aa0000', padding: 20 } },
          React.createElement(Text, { style: { color: '#fff', fontSize: 20, marginBottom: 16 } }, '❌ RENDER ERROR'),
          React.createElement(Text, { style: { color: '#ff8888', fontSize: 14 } }, String(renderError?.message || renderError))
        );
      }
    };
  } catch (error) {
    nerror('❌ TabRouter Import failed: ' + (error as any)?.message);
    nerror('Error Details: ' + (error as any)?.stack?.substring(0, 300));
    // Decline to Error Display
    Component = () => React.createElement(
      View,
      { style: { flex: 1, backgroundColor: '#0000ff', justifyContent: 'center', alignItems: 'center', padding: 20 } },
      React.createElement(Text, { style: { color: '#fff', fontSize: 24, marginBottom: 16 } }, '❌ IMPORT FAILED'),
      React.createElement(Text, { style: { color: '#fff', fontSize: 14, marginTop: 20 } }, String((error as any)?.message || error))
    );
  }
}

// 🔴 Global error management
const originalErrorHandler = (globalThis as any).ErrorUtils?.getGlobalHandler?.();
(globalThis as any).ErrorUtils?.setGlobalHandler?.((error: any, isFatal: boolean) => {
  nerror('🔴🔴🔴 [GLOBAL ERROR] ' + (isFatal ? 'FATAL' : 'NON-FATAL') + ': ' + (error?.message || error));
  nerror('Stack: ' + error?.stack?.substring(0, 500));

  // macOS RN: Default processor may trigger LogBox/RedBox Relevant logic (some versions would result in UIBlock Unusual/Whitescreen)
  // To ensure that the main interface is available, this default is not forwarded to original handler(If so, the provisional text should read true).
  const PASS_TO_DEFAULT_HANDLER = false;
  if (PASS_TO_DEFAULT_HANDLER && originalErrorHandler) originalErrorHandler(error, isFatal);
});

try {
  AppRegistry.registerComponent('ActrApp', () => Component);
  nlog('✅ ActrApp Component registered');

  // 🔴 Verify whether registration is successful
  const registered = (AppRegistry as any).getAppKeys?.();
  nlog('📱 Registered App keys: ' + (registered?.join(', ') || 'N/A'));
} catch (error) {
  nerror('❌ Failed to register component: ' + (error as any)?.message);
}

nlog('🏁 Bundle finished; waiting for runApplication...');


export default Component
