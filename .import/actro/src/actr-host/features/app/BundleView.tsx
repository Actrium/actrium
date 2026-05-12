import React from 'react';
import {
  ActivityIndicator as RNActivityIndicator,
  Text as RNText,
  View as RNView,
} from 'react-native';
import { logger } from '../../../logger';

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

// Module loaded - minimal logging

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
}> = ({
  engine,
  bundleUrl,
  containerHeight,
  containerWidth,
  style,
  onLoad,
  onError,
  fallback,
  renderError,
}) => {
  const [state, setState] = React.useState<'loading' | 'loaded' | 'error'>(
    'loading',
  );
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
        await engine.loadBundle(bundleUrl, { containerHeight, containerWidth });

        if (mountedRef.current) {
          setState('loaded');
          onLoad?.();
        }
      } catch (err: unknown) {
        nerror(
          '[LocalEngineView] ❌ 加载失败: ' +
            (err instanceof Error ? err.message : String(err)),
        );
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
  }, [
    engine,
    bundleUrl,
    containerHeight,
    containerWidth,
    handleUpdate,
    onLoad,
    onError,
  ]);

  // 渲染 - 使用 RN 原生组件用于内部 UI
  if (state === 'loading') {
    return <RNView style={[{ flex: 1 }, style]}>{fallback}</RNView>;
  }

  if (state === 'error' && error) {
    return <RNView style={[{ flex: 1 }, style]}>{renderError?.(error)}</RNView>;
  }

  return (
    content || (
      <RNView style={[{ flex: 1, backgroundColor: '#1a1a2a' }, style]}>
        <RNText style={{ color: '#888' }}>Loading...</RNText>
      </RNView>
    )
  );
};

// EngineView 使用本地版本
const EngineView = LocalEngineView;

import type { ComponentMap } from 'rill';
import {
  ThemeView,
  StepList,
  UserAvatar,
  ChatBubble,
  MyTouchableOpacity,
  DropdownMenu,
  DropdownMenuItem,
  DropdownMenuText,
  DropdownMenuSubText,
  GlobalAlert,
} from 'askit';
import { createEngineAdapter } from 'askit/core';

const log = logger.child('BundleView');
const nerror = (msg: string) => log.error(msg);

type Props = {
  script: string;
  containerHeight?: number;
  containerWidth?: number;
};

// 包装 ScrollView 强制隐藏横向滚动条（防止缩放时闪烁）
const WrappedScrollView: React.FC<Record<string, unknown>> = (props) => {
  return React.createElement(ScrollView, {
    ...props,
    showsHorizontalScrollIndicator: false,
  });
};

// Host 组件映射：使用 rill preset 组件，这些组件已正确处理事件序列化
// 防止 GestureResponderEvent 等复杂事件对象导致 JSI 边界序列化 OOM
export const hostComponents: ComponentMap = {
  // rill preset 组件 - 已处理事件序列化
  View: View as unknown as React.ComponentType<Record<string, unknown>>,
  Text: Text as unknown as React.ComponentType<Record<string, unknown>>,
  Image: Image as unknown as React.ComponentType<Record<string, unknown>>,
  ScrollView: WrappedScrollView as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  TouchableOpacity: TouchableOpacity as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  Button: Button as unknown as React.ComponentType<Record<string, unknown>>,
  ActivityIndicator: ActivityIndicator as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  TextInput: TextInput as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  FlatList: FlatList as unknown as React.ComponentType<Record<string, unknown>>,
  Switch: Switch as unknown as React.ComponentType<Record<string, unknown>>,
  // Askit 组件
  ThemeView: ThemeView as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  StepList: StepList as unknown as React.ComponentType<Record<string, unknown>>,
  UserAvatar: UserAvatar as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  ChatBubble: ChatBubble as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  MyTouchableOpacity: MyTouchableOpacity as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  DropdownMenu: DropdownMenu as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  DropdownMenuItem: DropdownMenuItem as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  DropdownMenuText: DropdownMenuText as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  DropdownMenuSubText: DropdownMenuSubText as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  GlobalAlert: GlobalAlert as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  // Panel 架构组件
  // PanelMarker: PanelMarker as unknown as React.ComponentType<Record<string, unknown>>,
};

export const BundleView: React.FC<Props> = ({
  script,
  containerHeight = 977,
  containerWidth = 240,
}) => {
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
        if (cancelled) return;

        currentEngine = new Engine({
          timeout: 10000,
          debug: false,
          logger: {
            log: () => {},
            warn: (...args: unknown[]) =>
              log.warning(args.map((a) => String(a)).join(' ')),
            error: (...args: unknown[]) =>
              nerror(args.map((a) => String(a)).join(' ')),
          },
        });
        currentEngine.register(hostComponents);
        currentAdapter = createEngineAdapter(currentEngine);

        if (cancelled) {
          currentAdapter?.dispose();
          currentEngine?.destroy();
          return;
        }

        setEngine(currentEngine);
        setIsLoading(false);
      } catch (e: any) {
        if (cancelled) return;
        nerror('[BundleView] init failed: ' + e?.message);
        setLoadError(e instanceof Error ? e : new Error(String(e)));
        setIsLoading(false);
      }
    }

    init();

    return () => {
      cancelled = true;
      currentAdapter?.dispose();
      currentEngine?.destroy();
    };
  }, [script]);

  // 加载中 - 使用 RN 原生组件用于内部 UI
  if (isLoading) {
    return (
      <RNView
        style={{
          flex: 1,
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#1a1a2a',
        }}
      >
        <RNActivityIndicator size="large" color="#007AFF" />
        <RNText style={{ color: '#888', fontSize: 14, marginTop: 16 }}>
          初始化 Engine...
        </RNText>
      </RNView>
    );
  }

  // 加载错误
  if (loadError) {
    return (
      <RNView
        style={{
          flex: 1,
          alignItems: 'center',
          justifyContent: 'center',
          padding: 16,
          backgroundColor: '#1a1a1a',
        }}
      >
        <RNText
          style={{
            color: '#ff6b6b',
            fontSize: 18,
            fontWeight: '600',
            marginBottom: 12,
          }}
        >
          Engine 初始化失败
        </RNText>
        <RNText
          style={{
            color: '#ccc',
            fontSize: 14,
            textAlign: 'center',
            marginBottom: 8,
          }}
        >
          {loadError.message}
        </RNText>
        <RNText style={{ color: '#666', fontSize: 12, textAlign: 'center' }}>
          {loadError.stack?.substring(0, 200)}
        </RNText>
      </RNView>
    );
  }

  // Engine 未就绪
  if (!engine) {
    return (
      <RNView
        style={{
          flex: 1,
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#1a1a2a',
        }}
      >
        <RNActivityIndicator size="large" color="#007AFF" />
        <RNText style={{ color: '#888', fontSize: 14, marginTop: 16 }}>
          准备 Engine...
        </RNText>
      </RNView>
    );
  }

  return (
    <EngineView
      containerHeight={containerHeight}
      containerWidth={containerWidth}
      key={script}
      engine={engine}
      bundleUrl={script}
      style={{ flex: 1 }}
      onLoad={() => {}}
      onError={(error) => {
        nerror('[BundleView] load failed: ' + error.message);
      }}
      fallback={
        <RNView
          style={{
            flex: 1,
            alignItems: 'center',
            justifyContent: 'center',
            backgroundColor: '#1a1a2a',
          }}
        >
          <RNActivityIndicator size="large" color="#007AFF" />
          <RNText style={{ color: '#888', fontSize: 14, marginTop: 16 }}>
            加载插件中...
          </RNText>
        </RNView>
      }
      renderError={(error) => (
        <RNView
          style={{
            flex: 1,
            alignItems: 'center',
            justifyContent: 'center',
            padding: 16,
            backgroundColor: '#1a1a1a',
          }}
        >
          <RNText
            style={{
              color: '#ff6b6b',
              fontSize: 18,
              fontWeight: '600',
              marginBottom: 12,
            }}
          >
            插件运行异常
          </RNText>
          <RNText
            style={{
              color: '#ccc',
              fontSize: 14,
              textAlign: 'center',
              marginBottom: 8,
            }}
          >
            {error.message}
          </RNText>
          <RNText style={{ color: '#666', fontSize: 12, textAlign: 'center' }}>
            {error.stack?.substring(0, 200)}
          </RNText>
        </RNView>
      )}
    />
  );
};

export default BundleView;
