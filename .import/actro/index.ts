// 🔴 最早期调试 - 在任何 import 之前
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

// 🔴 调试 polyfill 状态
// Keep a minimal startup marker through unified logger.
import { logger } from './src/logger';
const log = logger.child('index');
log.debug('console.assert type (after guard): ' + typeof console.assert);
import { initDatabase } from './src/libs/Sqlite.init';
initDatabase();

// 中文注释：关闭 Reanimated strict 模式警告，@legendapp/list/keyboard 内部在渲染期读取 shared value 会触发此警告。
import { configureReanimatedLogger } from 'react-native-reanimated';
configureReanimatedLogger({ strict: false });

import 'react-native-gesture-handler';
import 'react-native-get-random-values';
import NetInfo from '@react-native-community/netinfo';

import React from 'react';
import { AppRegistry, LogBox, View, Text, NativeModules, Platform } from 'react-native';
const PromiseRejectionTracking = require('promise/setimmediate/rejection-tracking');

// iOS 默认原生 reachability 在 Wi-Fi 断网场景下事件不稳定，这里统一切到 NetInfo 自己的可达性探测。
NetInfo.configure({
  useNativeReachability: false,
  // Wi-Fi 接口还在但外网断掉时，NetInfo 只能靠主动探测发现异常。
  // 默认 60 秒长轮询太慢，这里压到 3 秒，避免顶部网络状态长时间不更新。
  reachabilityLongTimeout: 3 * 1000,
  reachabilityShortTimeout: 3 * 1000,
  reachabilityRequestTimeout: 3 * 1000,
});

// 🔴 统一日志工具 - 输出到:
// 1. 远程日志服务器 (actr log server, 跨平台)
// 2. 原生控制台 (通过 NativeLogger, 仅 iOS/macOS)
// 尽早安装 console 拦截器，确保后续所有 console.xxx 都能转发到远程 log server
import { installConsoleInterceptor } from './src/devtools-console';
installConsoleInterceptor();

const NativeLogger = NativeModules.NativeLogger;

const sendLog = (level: string, msg: string) => {
  // 远程日志转发由 devtools-console.ts 的 console 拦截器统一处理，
  // 这里通过 console 输出，拦截器会自动转发到远程 log server。
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

// 统一捕获未处理的 Promise 拒绝，避免异步错误只在运行时静默丢失。
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

nlog('📱 Bundle 开始执行');
nlog('📦 NativeModules keys: ' + Object.keys(NativeModules || {}).slice(0, 15).join(', '));
nlog('📦 NativeLogger 可用: ' + (NativeLogger ? 'YES' : 'NO'));

// 🔴 已验证 JS 正常执行，移除 Alert
nlog('🏁 JS Bundle 执行中...');

// 只禁用特定警告
const DISABLE_LOGBOX_ON_MACOS = true;
if (DISABLE_LOGBOX_ON_MACOS && Platform.OS === 'macos') {
  // RN macOS：LogBox 在部分版本上可能触发 `*** NULL CGColor argument in colorWithCGColor:.` 崩溃
  // 先整体禁用，避免开发期频繁中断；日志仍会通过 console / NativeLogger 输出。
  LogBox.ignoreAllLogs(true);
} else {
  LogBox.ignoreLogs([
    'Non-serializable values were found in the navigation state',
    'Sending `onAnimatedValueUpdate` with no listeners registered',
  ]);
}

// 🔴 临时测试组件 - 验证基础渲染链路
const TestComponent = (props: any) => {
  nlog('🧪 [TestComponent] 渲染开始');
  nlog('🧪 [TestComponent] props keys: ' + Object.keys(props || {}).join(', '));

  const tabs = props?.tabs || [];
  const currentTabId = props?.currentTabId;
  const currentTab = tabs.find((t: any) => t.id === currentTabId) || tabs[0];

  nlog('🧪 [TestComponent] tabs数量: ' + tabs.length + ', currentTab: ' + (currentTab?.title || 'N/A'));

  return React.createElement(
    View,
    { style: { flex: 1, backgroundColor: '#ff00ff', padding: 20 } }, // 紫色背景
    React.createElement(Text, { style: { color: '#fff', fontSize: 28, fontWeight: 'bold', marginBottom: 16 } },
      '🧪 TEST COMPONENT'),
    React.createElement(Text, { style: { color: '#fff', fontSize: 16, marginBottom: 8 } },
      `Tabs: ${tabs.length}`),
    React.createElement(Text, { style: { color: '#fff', fontSize: 16, marginBottom: 8 } },
      `Current Tab: ${currentTab?.title || 'N/A'}`),
    React.createElement(Text, { style: { color: '#0ff', fontSize: 14, marginTop: 20 } },
      '如果看到这个紫色背景，说明基础链路正常工作')
  );
};

// 导入完整版 ActrApp 组件（使用 rill/askit）
let Component;
const USE_TEST_COMPONENT = false; // 🟢 切回正式组件测试逻辑

// 🔴 全局 props 缓存 - 防止 React Native bridge 传入空 props
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
  nlog('🧪 使用测试组件');
} else {
  try {
    nlog('📦 开始导入 TabRouter...');
    const module = require('./src/TabRouter');
    nlog('📦 TabRouter 模块: ' + Object.keys(module || {}).join(', '));
    Component = module.TabRouter || module.default;
    nlog('✅ TabRouter 导入成功，类型: ' + typeof Component);

    // 包装组件：添加错误边界
    const OriginalComponent = Component;
    Component = (props: any) => {
      nlog('🎨 [Wrapper] TabRouter 开始渲染');
      // console.error(props.isLeftPanelVisible, '????')
      try {
        return React.createElement(ErrorBoundary, null, React.createElement(OriginalComponent, props));
      } catch (renderError: any) {
        nerror('❌ [Wrapper] TabRouter 渲染失败: ' + renderError?.message);
        return React.createElement(
          View,
          { style: { flex: 1, backgroundColor: '#aa0000', padding: 20 } },
          React.createElement(Text, { style: { color: '#fff', fontSize: 20, marginBottom: 16 } }, '❌ RENDER ERROR'),
          React.createElement(Text, { style: { color: '#ff8888', fontSize: 14 } }, String(renderError?.message || renderError))
        );
      }
    };
  } catch (error) {
    nerror('❌ TabRouter 导入失败: ' + (error as any)?.message);
    nerror('错误详情: ' + (error as any)?.stack?.substring(0, 300));
    // 降级到错误显示
    Component = () => React.createElement(
      View,
      { style: { flex: 1, backgroundColor: '#0000ff', justifyContent: 'center', alignItems: 'center', padding: 20 } },
      React.createElement(Text, { style: { color: '#fff', fontSize: 24, marginBottom: 16 } }, '❌ IMPORT FAILED'),
      React.createElement(Text, { style: { color: '#fff', fontSize: 14, marginTop: 20 } }, String((error as any)?.message || error))
    );
  }
}

// 🔴 全局错误处理
const originalErrorHandler = (globalThis as any).ErrorUtils?.getGlobalHandler?.();
(globalThis as any).ErrorUtils?.setGlobalHandler?.((error: any, isFatal: boolean) => {
  nerror('🔴🔴🔴 [GLOBAL ERROR] ' + (isFatal ? 'FATAL' : 'NON-FATAL') + ': ' + (error?.message || error));
  nerror('Stack: ' + error?.stack?.substring(0, 500));

  // macOS RN：默认处理器可能会触发 LogBox/RedBox 相关逻辑（某些版本会导致 UIBlock 异常/白屏）
  // 为保证主界面优先可用，这里默认不再转发给原始 handler（如需恢复，可临时改为 true）。
  const PASS_TO_DEFAULT_HANDLER = false;
  if (PASS_TO_DEFAULT_HANDLER && originalErrorHandler) originalErrorHandler(error, isFatal);
});

try {
  AppRegistry.registerComponent('ActrApp', () => Component);
  nlog('✅ ActrApp 组件已注册');

  // 🔴 验证注册是否成功
  const registered = (AppRegistry as any).getAppKeys?.();
  nlog('📱 已注册的 App keys: ' + (registered?.join(', ') || 'N/A'));
} catch (error) {
  nerror('❌ 注册组件失败: ' + (error as any)?.message);
}

nlog('🏁 Bundle 执行完毕，等待 runApplication...');


export default Component
