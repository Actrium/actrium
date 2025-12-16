import React from 'react';
import {
  ActivityIndicator,
  Button,
  Image,
  NativeModules,
  Pressable,
  ScrollView,
  Text,
  TouchableOpacity,
  View,
} from 'react-native';

// 导入 rill Engine - 内置 JSC sandbox 自动检测
import { Engine } from 'rill';

console.log('[BundleView] ✅ rill Engine 导入成功 (内置 JSC sandbox)');

// 🔴 本地简化版 EngineView - 绕过 Metro 路径解析问题
const LocalEngineView: React.FC<{ containerHeight?: number; containerWidth?: number;
  engine: any;
  bundleUrl: string;
  style?: object;
  onLoad?: () => void;
  onError?: (error: Error) => void;
  fallback?: React.ReactNode;
  renderError?: (error: Error) => React.ReactNode;
}> = ({ engine, bundleUrl, containerHeight, containerWidth, style, onLoad, onError, fallback, renderError }) => {
  const [state, setState] = React.useState<'loading' | 'loaded' | 'error'>('loading');
  const [error, setError] = React.useState<Error | null>(null);
  const [content, setContent] = React.useState<React.ReactElement | null>(null);
  const mountedRef = React.useRef(true);

  console.log('[LocalEngineView] 🔵 渲染, state:', state);

  // 更新回调
  const handleUpdate = React.useCallback(() => {
    nlog('[LocalEngineView] handleUpdate called, mounted=' + mountedRef.current);
    if (!mountedRef.current) return;
    const receiver = engine.getReceiver();
    nlog('[LocalEngineView] receiver=' + !!receiver);
    if (receiver) {
      const newContent = receiver.render();
      nlog('[LocalEngineView] receiver.render() returned: ' + (newContent ? 'element' : 'null'));

      // 精简调试信息
      const debugInfo = (receiver as any).getDebugInfo?.();
      if (debugInfo && debugInfo.functionCount !== undefined) {
        nlog('[LocalEngineView] Functions: ' + debugInfo.functionCount + ', Last fnId: ' + debugInfo.lastFunctionFnId);
      }

      // 简化内容检查
      if (newContent) {
        nlog('[LocalEngineView] ✅ Got new content');
      }
      setContent(newContent);
      nlog('[LocalEngineView] setContent called');
    }
  }, [engine]);

  // 加载插件
  React.useEffect(() => {
    mountedRef.current = true;
    console.log('[LocalEngineView] useEffect 触发，准备加载插件');

    async function loadPlugin() {
      console.log('[LocalEngineView] loadPlugin 开始');
      if (engine.isLoaded || engine.isDestroyed) {
        console.log('[LocalEngineView] 跳过加载: isLoaded=' + engine.isLoaded);
        return;
      }

      try {
        console.log('[LocalEngineView] 创建 Receiver...');
        engine.createReceiver(handleUpdate);

        // 初始化运行时
        console.log('[LocalEngineView] 初始化运行时...');
        await (engine as any).initializeRuntime();

        // 注入回调保护机制
        console.log('[LocalEngineView] 注入回调保护...');
        const callbackProtectionScript = `
(function() {
  // 确保回调表存在并保护它不被意外清空
  if (!globalThis.__callbacks) {
    globalThis.__callbacks = new Map();
    globalThis.__callbackId = 0;
  }
  
  // 保存原始函数引用
  const originalRegister = globalThis.__registerCallback;
  const originalInvoke = globalThis.__invokeCallback;
  
  // 增强回调注册，添加调试信息
  globalThis.__registerCallback = function(fn) {
    const fnId = originalRegister ? originalRegister(fn) : ('fn_' + (++globalThis.__callbackId));
    if (!originalRegister) {
      globalThis.__callbacks.set(fnId, fn);
    }
    console.log('[CallbackProtection] Registered callback:', fnId, 'Total callbacks:', globalThis.__callbacks.size);
    return fnId;
  };
  
  // 增强回调调用，添加状态检查和调度
  globalThis.__invokeCallback = function(fnId, args) {
    console.log('[CallbackProtection] Invoking callback:', fnId, 'Available:', globalThis.__callbacks.size);
    console.log('[CallbackProtection] Available callbacks:', Array.from(globalThis.__callbacks.keys()).slice(0, 10));
    
    const result = originalInvoke ? originalInvoke(fnId, args) : (() => {
      const fn = globalThis.__callbacks.get(fnId);
      if (fn) {
        try {
          return fn.apply(null, args || []);
        } catch (e) {
          console.error('[CallbackProtection] Callback error:', e);
        }
      } else {
        console.warn('[CallbackProtection] Callback not found:', fnId);
      }
    })();
    
    // 强制触发一次调度来刷新UI
    Promise.resolve().then(() => {
      try {
        // 尝试触发reconciler刷新
        if (typeof globalThis.__flushUpdates === 'function') {
          globalThis.__flushUpdates();
        }
      } catch (e) {
        // 静默失败
      }
    });
    
    return result;
  };
  
  console.log('[CallbackProtection] Callback protection installed, current callbacks:', globalThis.__callbacks.size);
})();
`;
        // 尝试多种方式注入回调保护
        let injectionSuccess = false;
        for (const method of ['evaluate', 'runScript', 'exec', 'evaluateJavaScript']) {
          try {
            if (typeof (engine as any)[method] === 'function') {
              await (engine as any)[method](callbackProtectionScript);
              console.log('[LocalEngineView] ✅ 回调保护已安装 via', method);
              injectionSuccess = true;
              break;
            }
          } catch (e) {
            console.warn('[LocalEngineView] 方法', method, '失败:', e.message);
          }
        }
        if (!injectionSuccess) {
          console.warn('[LocalEngineView] ⚠️ 所有注入方法都失败，将在bundle加载后尝试');
        }

        console.log('[LocalEngineView] 调用 loadBundle...');
        await engine.loadBundle(bundleUrl, { containerHeight, containerWidth });
        console.log('[LocalEngineView] ✅ loadBundle 完成');

        // 在bundle加载后，通过receiver向沙箱注入回调修复
        console.log('[LocalEngineView] 注入回调修复补丁...');
        const receiver = engine.getReceiver();
        if (receiver && typeof (receiver as any).sendMessage === 'function') {
          try {
            await (receiver as any).sendMessage({
              type: 'EVAL',
              code: `
(function() {
  console.log('[CallbackFix] Installing callback fix...');
  
  if (!globalThis.__callbacks) {
    globalThis.__callbacks = new Map();
    globalThis.__callbackId = 0;
    console.log('[CallbackFix] Created new callback map');
  }
  
  const originalInvoke = globalThis.__invokeCallback;
  globalThis.__invokeCallback = function(fnId, args) {
    console.log('[CallbackFix] Invoking:', fnId, 'Available callbacks:', globalThis.__callbacks.size);
    console.log('[CallbackFix] Callback keys:', Array.from(globalThis.__callbacks.keys()).slice(0, 5));
    
    const fn = globalThis.__callbacks.get(fnId);
    if (fn) {
      console.log('[CallbackFix] Found callback, executing...');
      try {
        const result = fn.apply(null, args || []);
        console.log('[CallbackFix] Callback executed successfully');
        
        // 强制触发重渲染
        setTimeout(() => {
          try {
            if (typeof globalThis.__flushUpdates === 'function') {
              globalThis.__flushUpdates();
            }
          } catch (e) {}
        }, 0);
        
        return result;
      } catch (e) {
        console.error('[CallbackFix] Callback error:', e);
      }
    } else {
      console.warn('[CallbackFix] Callback not found:', fnId);
      if (originalInvoke) {
        return originalInvoke(fnId, args);
      }
    }
  };
  
  console.log('[CallbackFix] Callback fix installed, callbacks:', globalThis.__callbacks.size);
})();
`
            });
            console.log('[LocalEngineView] ✅ 回调修复补丁已发送');
          } catch (e) {
            console.warn('[LocalEngineView] 回调修复补丁发送失败:', e);
          }
        } else {
          console.warn('[LocalEngineView] receiver.sendMessage 不可用，尝试其他方式...');
        }

        if (mountedRef.current) {
          setState('loaded');
          onLoad?.();
        }
      } catch (err: any) {
        console.error('[LocalEngineView] ❌ 加载失败:', err?.message);
        if (mountedRef.current) {
          const loadError = err instanceof Error ? err : new Error(String(err));
          setError(loadError);
          setState('error');
          onError?.(loadError);
        }
      }
    }

    loadPlugin();

    return () => {
      mountedRef.current = false;
    };
  }, [engine, bundleUrl, handleUpdate, onLoad, onError]);

  // 渲染
  if (state === 'loading') {
    return <View style={[{ flex: 1 }, style]}>{fallback}</View>;
  }

  if (state === 'error' && error) {
    return <View style={[{ flex: 1 }, style]}>{renderError?.(error)}</View>;
  }

  // 🔴 直接返回 content,不要包装器
  // 测试: 包装器可能阻止了 children 的渲染
  nlog('[LocalEngineView] Rendering content directly (no wrapper)');
  return content || (
    <View style={[{ flex: 1, backgroundColor: '#ff0000' }, style]}>
      <Text style={{ color: '#ffffff' }}>No content</Text>
    </View>
  );
};

// EngineView 使用本地版本
const EngineView = LocalEngineView;

import type { ComponentMap } from 'rill';
import { ThemeView, StepList, UserAvatar, ChatBubble, PanelMarker } from 'askit';

// Patch ScrollView to add debugging - DO NOT force flexGrow:1
const PatchedScrollView = React.forwardRef<any, any>((props, ref) => {
  const { contentContainerStyle, onContentSizeChange, ...rest } = props || {};
  // 🔴 移除强制 flexGrow:1 - 让内容使用自己的样式
  const patched = contentContainerStyle;
  const handleContentSizeChange = (w: number, h: number) => {
    try { nlog(`[PatchedScrollView] content size: ${w}x${h}`); } catch {}
    onContentSizeChange?.(w, h);
  };
  nlog(`[PatchedScrollView-v2-NO-FLEXGROW] render, contentContainerStyle: ${JSON.stringify(patched)}`);
  return <ScrollView ref={ref} contentContainerStyle={patched} onContentSizeChange={handleContentSizeChange} {...rest} />;
});
PatchedScrollView.displayName = 'PatchedScrollView';
import { createEngineAdapter } from 'askit/core';

// 远程日志
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
const nerror = (msg: string) => sendLog('error', msg);

type Props = {
  script: string;
  containerHeight?: number;
  containerWidth?: number;
};

// 🔴 调试包装的 Pressable
const DebugPressable = React.forwardRef<View, any>((props, ref) => {
  nlog('[DebugPressable] 🔧 Rendered with onPress: ' + typeof props.onPress);
  nlog('[DebugPressable] 🔧 Props keys: ' + Object.keys(props).join(', '));

  const handlePress = () => {
    nlog('[DebugPressable] 🖱️ onPress called!');
    if (props.onPress) {
      nlog('[DebugPressable] 🖱️ Calling props.onPress');
      props.onPress();
    }
  };

  return (
    <Pressable {...props} onPress={handlePress} ref={ref}>
      {props.children}
    </Pressable>
  );
});

// Host 组件映射：Rill 插件使用字符串名称，映射到真实 React Native 组件
export const hostComponents: ComponentMap = {
  // 基础 React Native 组件
  View: View as unknown as React.ComponentType<Record<string, unknown>>,
  Text: Text as unknown as React.ComponentType<Record<string, unknown>>,
  Image: Image as unknown as React.ComponentType<Record<string, unknown>>,
  ScrollView: PatchedScrollView as unknown as React.ComponentType<Record<string, unknown>>,
  // Use native TouchableOpacity mapping to preserve adapter behavior
  TouchableOpacity: TouchableOpacity as unknown as React.ComponentType<Record<string, unknown>>,
  Button: Button as unknown as React.ComponentType<Record<string, unknown>>,
  ActivityIndicator: ActivityIndicator as unknown as React.ComponentType<Record<string, unknown>>,
  // Askit 组件
  ThemeView: ThemeView as unknown as React.ComponentType<Record<string, unknown>>,
  StepList: StepList as unknown as React.ComponentType<Record<string, unknown>>,
  UserAvatar: UserAvatar as unknown as React.ComponentType<Record<string, unknown>>,
  ChatBubble: ChatBubble as unknown as React.ComponentType<Record<string, unknown>>,
  // Panel 架构组件
  PanelMarker: PanelMarker as unknown as React.ComponentType<Record<string, unknown>>,
};

export const BundleView: React.FC<Props> = ({ script, containerHeight = 977, containerWidth = 240 }) => {
  // 只在首次加载时记录
  const isFirstLoad = !script.includes('__RillPlugin');
  if (isFirstLoad) {
    nlog(`[BundleView] 开始加载脚本 (${script.length} 字节)`);
  }

  // 🔴 临时测试：直接返回静态内容，跳过 EngineView
  const [useSimpleMode] = React.useState(false); // 🟢 使用真正的 EngineView

  if (useSimpleMode) {
    nlog('[GuestAppView-v3] 🟢 使用简单模式 (静态内容)');
    return (
      <View style={{ flex: 1, backgroundColor: '#1a1a2e', padding: 20 }}>
        <Text style={{ color: '#00ff00', fontSize: 24, fontWeight: 'bold', marginBottom: 20 }}>
          ✅ GuestAppView 简单模式
        </Text>
        <Text style={{ color: '#ffffff', fontSize: 16, marginBottom: 10 }}>
          React Native 渲染正常工作！
        </Text>
        <Text style={{ color: '#888888', fontSize: 12 }}>
          Script 长度: {script.length} 字符
        </Text>
        <View style={{ marginTop: 20, padding: 10, backgroundColor: '#2a2a4e', borderRadius: 8 }}>
          <Text style={{ color: '#aaaaff', fontSize: 11 }}>
            {script.substring(0, 200)}...
          </Text>
        </View>
      </View>
    );
  }

  // Engine 和 adapter 状态
  const [engine, setEngine] = React.useState<Engine | null>(null);
  const [isLoading, setIsLoading] = React.useState(true);
  const [loadError, setLoadError] = React.useState<Error | null>(null);

  // 创建 Engine（rill 内置自动检测，Apple 平台使用 JSC sandbox）
  React.useEffect(() => {
    let cancelled = false;
    let currentEngine: Engine | null = null;
    let currentAdapter: { dispose: () => void } | null = null;

    function init() {
      try {
        nlog('[BundleView] 开始初始化...');

        if (cancelled) return;

        nlog('[BundleView] 创建 Engine 实例 (rill 内置 JSC 自动检测)');
        currentEngine = new Engine({
          timeout: 10000,
          debug: true,
          logger: {
            log: (...args: unknown[]) => nlog(args.map(a => String(a)).join(' ')),
            warn: (...args: unknown[]) => sendLog('warn', args.map(a => String(a)).join(' ')),
            error: (...args: unknown[]) => nerror(args.map(a => String(a)).join(' ')),
          },
        });
        currentEngine.register(hostComponents);

        nlog('[BundleView] 创建 askit/core adapter');
        currentAdapter = createEngineAdapter(currentEngine);

        if (cancelled) {
          currentAdapter?.dispose();
          currentEngine?.destroy();
          return;
        }

        setEngine(currentEngine);
        setIsLoading(false);
        nlog('[BundleView] ✅ Engine 初始化完成 (rill 内置 JSC sandbox)');
      } catch (e: any) {
        if (cancelled) return;
        nerror('[BundleView] 初始化失败: ' + e?.message);
        setLoadError(e instanceof Error ? e : new Error(String(e)));
        setIsLoading(false);
      }
    }

    init();

    return () => {
      cancelled = true;
      if (currentAdapter) {
        nlog('[BundleView] cleanup: 销毁 adapter');
        currentAdapter.dispose();
      }
      if (currentEngine) {
        nlog('[BundleView] cleanup: 销毁 Engine');
        currentEngine.destroy();
      }
    };
  }, [script]);

  // 简化日志：只在状态变化时记录

  // 加载中
  if (isLoading) {
    return (
      <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#1a1a2a' }}>
        <ActivityIndicator size="large" color="#007AFF" />
        <Text style={{ color: '#888', fontSize: 14, marginTop: 16 }}>初始化 Engine...</Text>
      </View>
    );
  }

  // 加载错误
  if (loadError) {
    return (
      <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
        <Text style={{ color: '#ff6b6b', fontSize: 18, fontWeight: '600', marginBottom: 12 }}>
          Engine 初始化失败
        </Text>
        <Text style={{ color: '#ccc', fontSize: 14, textAlign: 'center', marginBottom: 8 }}>{loadError.message}</Text>
        <Text style={{ color: '#666', fontSize: 12, textAlign: 'center' }}>{loadError.stack?.substring(0, 200)}</Text>
      </View>
    );
  }

  // Engine 未就绪
  if (!engine) {
    return (
      <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#1a1a2a' }}>
        <ActivityIndicator size="large" color="#007AFF" />
        <Text style={{ color: '#888', fontSize: 14, marginTop: 16 }}>准备 Engine...</Text>
      </View>
    );
  }

  return (
    <EngineView containerHeight={containerHeight} containerWidth={containerWidth}
      key={script}
      engine={engine}
      bundleUrl={script}
      style={{ flex: 1 }}
      onLoad={() => {
        nlog('✅ [BundleView] EngineView 加载成功');
      }}
      onError={error => {
        nerror('❌ [BundleView] 加载失败: ' + error.message);
        nerror('错误堆栈: ' + error.stack?.substring(0, 500));
      }}
      fallback={
        <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#1a1a2a' }}>
          <ActivityIndicator size="large" color="#007AFF" />
          <Text style={{ color: '#888', fontSize: 14, marginTop: 16 }}>加载插件中...</Text>
        </View>
      }
      renderError={error => (
        <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
          <Text style={{ color: '#ff6b6b', fontSize: 18, fontWeight: '600', marginBottom: 12 }}>
            插件运行异常
          </Text>
          <Text style={{ color: '#ccc', fontSize: 14, textAlign: 'center', marginBottom: 8 }}>{error.message}</Text>
          <Text style={{ color: '#666', fontSize: 12, textAlign: 'center' }}>{error.stack?.substring(0, 200)}</Text>
        </View>
      )}
    />
  );
};

export default BundleView;
