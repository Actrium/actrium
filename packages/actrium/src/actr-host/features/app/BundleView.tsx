import React from 'react';
import {
  ActivityIndicator as RNActivityIndicator,
  Text as RNText,
  View as RNView,
} from 'react-native';
import { logger } from '../../../logger';

// Import keel Engine and preset Component - Internal JSC sandbox Autodetect
import { Engine, type ComponentMap } from 'keel/host';
// Use keel preset Component - These components have been correctly sequenced.
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
} from 'keel/host/preset';

// Module loaded - minimal logging

// Simplified version EngineView - Use correctly keel API
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

  // Update Retal
  const handleUpdate = React.useCallback(() => {
    if (!mountedRef.current) return;
    const receiver = engine.getReceiver();
    if (receiver) {
      const newContent = receiver.render();
      // receiver.render() Possible Return string | ReactElement | null
      if (typeof newContent === 'string') {
        // If it's a string, wrap it in. Text Component
        setContent(<Text>{newContent}</Text>);
      } else {
        setContent(newContent as React.ReactElement | null);
      }
    }
  }, [engine]);

  // Load Plugins
  React.useEffect(() => {
    mountedRef.current = true;
    const unsubscribeUpdate = engine.on('update', handleUpdate);

    async function loadPlugin() {
      if (engine.isLoaded || engine.isDestroyed) {
        return;
      }

      try {
        // Create Receiver
        engine.createReceiver();

        // loadBundle The internal runtime will automatically start
        await engine.loadBundle(bundleUrl, { containerHeight, containerWidth });

        if (mountedRef.current) {
          setState('loaded');
          onLoad?.();
        }
      } catch (err: unknown) {
        nerror(
          '[LocalEngineView] ❌ Loading failed: ' +
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
      unsubscribeUpdate();
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

  // Render - Use RN Native components for internal use UI
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

// EngineView Use local version
const EngineView = LocalEngineView;

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

// Packaging ScrollView Force hidden horizontal scrollbar (preventing blinking during scaling)
const WrappedScrollView: React.FC<Record<string, unknown>> = (props) => {
  return React.createElement(ScrollView, {
    ...props,
    showsHorizontalScrollIndicator: false,
  });
};

// Host Component Map: Use keel preset Component, which has correctly processed the sequence of events
// Prevention GestureResponderEvent When a complex subject causes JSI Sequencing of borders OOM
export const hostComponents: ComponentMap = {
  // keel preset Component - Sequenced processed events
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
  // Askit Component
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
  // Panel Structure component
  // PanelMarker: PanelMarker as unknown as React.ComponentType<Record<string, unknown>>,
};

export const BundleView: React.FC<Props> = ({
  script,
  containerHeight = 977,
  containerWidth = 240,
}) => {
  // Engine and adapter Status
  const [engine, setEngine] = React.useState<Engine | null>(null);
  const [isLoading, setIsLoading] = React.useState(true);
  const [loadError, setLoadError] = React.useState<Error | null>(null);

  // Create Engine(keel The Queen's Day of Peace is a very important event for the people of the country.Apple Platform use JSC sandbox)
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

  // Loading - Use RN Native components for internal use UI
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
          Initialize Engine...
        </RNText>
      </RNView>
    );
  }

  // Load error
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
          Engine Initialization failed
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

  // Engine Not ready
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
          Ready. Engine...
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
            Loading plugins...
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
            Plugin Run Anomalous
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
