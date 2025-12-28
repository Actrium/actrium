import React from 'react';
import {
  ActivityIndicator as RNActivityIndicator,
  Pressable,
  ScrollView as RNScrollView,
  Text as RNText,
  View as RNView,
} from 'react-native';
import { logger } from '../logger';

// 导入 rill Engine 和 preset 组件 - 内置 JSC sandbox 自动检测
import { Engine } from 'rill';
// 使用 rill preset 组件 - 这些组件已正确处理事件序列化
import {
  ActivityIndicator,
  Button,
  Image,
  ScrollView,
  Text,
  TextInput,
  TouchableOpacity,
  View,
  FlatList,
  Switch,
} from 'rill/host/preset';

logger.child('BundleView').notice('✅ rill Engine 导入成功 (内置 JSC sandbox)');

// 简化版 EngineView - 正确使用 rill API
const LocalEngineView: React.FC<{
  containerHeight?: number;
  containerWidth?: number;
  engine: Engine;
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

  // 更新回调
  const handleUpdate = React.useCallback(() => {
    if (!mountedRef.current) return;
    const receiver = engine.getReceiver();
    if (receiver) {
      const newContent = receiver.render();
      // receiver.render() 可能返回 string | ReactElement | null
      if (typeof newContent === 'string') {
        // 如果是字符串，包装成 Text 组件
        setContent(<Text>{newContent}</Text>);
      } else {
        setContent(newContent as React.ReactElement | null);
      }
    }
  }, [engine]);

  // 加载插件
  React.useEffect(() => {
    mountedRef.current = true;

    async function loadPlugin() {
      if (engine.isLoaded || engine.isDestroyed) {
        return;
      }

      try {
        // 创建 Receiver
        engine.createReceiver(handleUpdate);

        // loadBundle 内部会自动初始化运行时
        nlog('[LocalEngineView] 调用 loadBundle...');
        await engine.loadBundle(bundleUrl, { containerHeight, containerWidth });
        nlog('[LocalEngineView] ✅ loadBundle 完成');

        if (mountedRef.current) {
          setState('loaded');
          onLoad?.();
        }
      } catch (err: unknown) {
        nerror('[LocalEngineView] ❌ 加载失败: ' + (err instanceof Error ? err.message : String(err)));
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
  }, [engine, bundleUrl, containerHeight, containerWidth, handleUpdate, onLoad, onError]);

  // 渲染 - 使用 RN 原生组件用于内部 UI
  if (state === 'loading') {
    return <RNView style={[{ flex: 1 }, style]}>{fallback}</RNView>;
  }

  if (state === 'error' && error) {
    return <RNView style={[{ flex: 1 }, style]}>{renderError?.(error)}</RNView>;
  }

  return content || (
    <RNView style={[{ flex: 1, backgroundColor: '#1a1a2a' }, style]}>
      <RNText style={{ color: '#888' }}>Loading...</RNText>
    </RNView>
  );
};

// EngineView 使用本地版本
const EngineView = LocalEngineView;

import type { ComponentMap } from 'rill';
import { ThemeView, StepList, UserAvatar, ChatBubble, PanelMarker } from 'askit';

// Patch ScrollView to add debugging - DO NOT force flexGrow:1
// 使用 rill preset 的 ScrollView 以正确处理事件序列化
const PatchedScrollView = React.forwardRef<any, any>((props, ref) => {
  const { contentContainerStyle, onContentSizeChange, ...rest } = props || {};
  // 🔴 移除强制 flexGrow:1 - 让内容使用自己的样式
  const patched = contentContainerStyle;
  const handleContentSizeChange = (w: number, h: number) => {
    try { nlog(`[PatchedScrollView] content size: ${w}x${h}`); } catch {}
    onContentSizeChange?.(w, h);
  };
  nlog(`[PatchedScrollView-v2-NO-FLEXGROW] render, contentContainerStyle: ${JSON.stringify(patched)}`);
  // 使用 rill preset ScrollView 来正确处理 scroll 事件序列化
  return <ScrollView ref={ref} contentContainerStyle={patched} onContentSizeChange={handleContentSizeChange} {...rest} />;
});
PatchedScrollView.displayName = 'PatchedScrollView';
import { createEngineAdapter } from 'askit/core';

const log = logger.child('BundleView');
const nlog = (msg: string) => log.debug(msg);
const nerror = (msg: string) => log.error(msg);

type Props = {
  script: string;
  containerHeight?: number;
  containerWidth?: number;
};

// 🔴 调试包装的 Pressable
const DebugPressable = React.forwardRef<RNView, any>((props, ref) => {
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

// Host 组件映射：使用 rill preset 组件，这些组件已正确处理事件序列化
// 防止 GestureResponderEvent 等复杂事件对象导致 JSI 边界序列化 OOM
export const hostComponents: ComponentMap = {
  // rill preset 组件 - 已处理事件序列化
  View: View as unknown as React.ComponentType<Record<string, unknown>>,
  Text: Text as unknown as React.ComponentType<Record<string, unknown>>,
  Image: Image as unknown as React.ComponentType<Record<string, unknown>>,
  ScrollView: PatchedScrollView as unknown as React.ComponentType<Record<string, unknown>>,
  TouchableOpacity: TouchableOpacity as unknown as React.ComponentType<Record<string, unknown>>,
  Button: Button as unknown as React.ComponentType<Record<string, unknown>>,
  ActivityIndicator: ActivityIndicator as unknown as React.ComponentType<Record<string, unknown>>,
  TextInput: TextInput as unknown as React.ComponentType<Record<string, unknown>>,
  FlatList: FlatList as unknown as React.ComponentType<Record<string, unknown>>,
  Switch: Switch as unknown as React.ComponentType<Record<string, unknown>>,
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
      <RNView style={{ flex: 1, backgroundColor: '#1a1a2e', padding: 20 }}>
        <RNText style={{ color: '#00ff00', fontSize: 24, fontWeight: 'bold', marginBottom: 20 }}>
          ✅ GuestAppView 简单模式
        </RNText>
        <RNText style={{ color: '#ffffff', fontSize: 16, marginBottom: 10 }}>
          React Native 渲染正常工作！
        </RNText>
        <RNText style={{ color: '#888888', fontSize: 12 }}>
          Script 长度: {script.length} 字符
        </RNText>
        <RNView style={{ marginTop: 20, padding: 10, backgroundColor: '#2a2a4e', borderRadius: 8 }}>
          <RNText style={{ color: '#aaaaff', fontSize: 11 }}>
            {script.substring(0, 200)}...
          </RNText>
        </RNView>
      </RNView>
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
            warn: (...args: unknown[]) => log.warning(args.map(a => String(a)).join(' ')),
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

  // 加载中 - 使用 RN 原生组件用于内部 UI
  if (isLoading) {
    return (
      <RNView style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#1a1a2a' }}>
        <RNActivityIndicator size="large" color="#007AFF" />
        <RNText style={{ color: '#888', fontSize: 14, marginTop: 16 }}>初始化 Engine...</RNText>
      </RNView>
    );
  }

  // 加载错误
  if (loadError) {
    return (
      <RNView style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
        <RNText style={{ color: '#ff6b6b', fontSize: 18, fontWeight: '600', marginBottom: 12 }}>
          Engine 初始化失败
        </RNText>
        <RNText style={{ color: '#ccc', fontSize: 14, textAlign: 'center', marginBottom: 8 }}>{loadError.message}</RNText>
        <RNText style={{ color: '#666', fontSize: 12, textAlign: 'center' }}>{loadError.stack?.substring(0, 200)}</RNText>
      </RNView>
    );
  }

  // Engine 未就绪
  if (!engine) {
    return (
      <RNView style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#1a1a2a' }}>
        <RNActivityIndicator size="large" color="#007AFF" />
        <RNText style={{ color: '#888', fontSize: 14, marginTop: 16 }}>准备 Engine...</RNText>
      </RNView>
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
        <RNView style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#1a1a2a' }}>
          <RNActivityIndicator size="large" color="#007AFF" />
          <RNText style={{ color: '#888', fontSize: 14, marginTop: 16 }}>加载插件中...</RNText>
        </RNView>
      }
      renderError={error => (
        <RNView style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
          <RNText style={{ color: '#ff6b6b', fontSize: 18, fontWeight: '600', marginBottom: 12 }}>
            插件运行异常
          </RNText>
          <RNText style={{ color: '#ccc', fontSize: 14, textAlign: 'center', marginBottom: 8 }}>{error.message}</RNText>
          <RNText style={{ color: '#666', fontSize: 12, textAlign: 'center' }}>{error.stack?.substring(0, 200)}</RNText>
        </RNView>
      )}
    />
  );
};

export default BundleView;
