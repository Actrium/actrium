/**
 * EngineView
 *
 * React Native component for rendering Guest UI in sandbox
 */

import type { ReactElement, ReactNode } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import type { Engine } from './engine';

/**
 * EngineView Props
 */
export interface EngineViewProps {
  /**
   * Engine instance
   */
  engine: Engine;

  /**
   * Bundle source (URL or code string)
   */
  bundleUrl: string;

  /**
   * Initial props to pass to the Guest
   */
  initialProps?: Record<string, unknown>;

  /**
   * Load complete callback
   */
  onLoad?: () => void;

  /**
   * Error callback
   */
  onError?: (error: Error) => void;

  /**
   * Destroy callback
   */
  onDestroy?: () => void;

  /**
   * Custom loading indicator
   */
  fallback?: ReactNode;

  /**
   * Custom error display
   */
  renderError?: (error: Error) => ReactNode;

  /**
   * Container style
   */
  style?: object;
}

/**
 * Loading state
 */
type LoadingState = 'idle' | 'loading' | 'loaded' | 'error';

/**
 * EngineView component
 *
 * @example
 * ```tsx
 * const engine = new Engine();
 * engine.register({ StepList: NativeStepList });
 *
 * <EngineView
 *   engine={engine}
 *   bundleUrl="https://cdn.example.com/bundle.js"
 *   initialProps={{ theme: 'dark' }}
 *   onLoad={() => console.log('Bundle loaded')}
 *   onError={(err) => console.error('Bundle error:', err)}
 * />
 * ```
 */
export function EngineView({
  engine,
  bundleUrl,
  initialProps,
  onLoad,
  onError,
  onDestroy,
  fallback,
  renderError,
  style,
}: EngineViewProps): ReactElement {
  const [loadingState, setLoadingState] = useState<LoadingState>('idle');
  const [error, setError] = useState<Error | null>(null);
  const [content, setContent] = useState<ReactElement | string | null>(null);
  const mountedRef = useRef(true);

  // Update callback
  const handleUpdate = useCallback(() => {
    if (!mountedRef.current) return;

    const receiver = engine.getReceiver();
    if (receiver) {
      setContent(receiver.render());
    }
  }, [engine]);

  // Load Guest
  useEffect(() => {
    mountedRef.current = true;

    async function loadGuest() {
      if (engine.isLoaded || engine.isDestroyed) {
        return;
      }

      setLoadingState('loading');
      setError(null);

      try {
        // Create Receiver
        engine.createReceiver(handleUpdate);

        // Load and execute Guest
        await engine.loadBundle(bundleUrl, initialProps);

        if (mountedRef.current) {
          setLoadingState('loaded');
          onLoad?.();
        }
      } catch (err) {
        console.error('[EngineView] loadGuest error:', err);
        if (mountedRef.current) {
          const loadError = err instanceof Error ? err : new Error(String(err));
          setError(loadError);
          setLoadingState('error');
          onError?.(loadError);
        }
      }
    }

    loadGuest();

    return () => {
      mountedRef.current = false;
    };
  }, [engine, bundleUrl, initialProps, handleUpdate, onLoad, onError]);

  // Listen to engine events
  useEffect(() => {
    const unsubscribeError = engine.on('error', (err) => {
      if (mountedRef.current) {
        setError(err);
        setLoadingState('error');
        onError?.(err);
      }
    });

    const unsubscribeDestroy = engine.on('destroy', () => {
      if (mountedRef.current) {
        setContent(null);
        onDestroy?.();
      }
    });

    return () => {
      unsubscribeError();
      unsubscribeDestroy();
    };
  }, [engine, onError, onDestroy]);

  // Cleanup
  useEffect(() => {
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Render loading state
  if (loadingState === 'loading' || loadingState === 'idle') {
    return (
      <View style={[styles.container, style]}>
        {fallback ?? (
          <View style={styles.loadingContainer}>
            <ActivityIndicator size="large" color="#007AFF" />
            <Text style={styles.loadingText}>Loading bundle...</Text>
          </View>
        )}
      </View>
    );
  }

  // Render error state
  if (loadingState === 'error' && error) {
    return (
      <View style={[styles.container, style]}>
        {renderError?.(error) ?? (
          <View style={styles.errorContainer}>
            <Text style={styles.errorTitle}>Bundle Error</Text>
            <Text style={styles.errorMessage}>{error.message}</Text>
          </View>
        )}
      </View>
    );
  }

  // Render Guest content
  return <View style={[styles.container, style]}>{content}</View>;
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  loadingContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  loadingText: {
    marginTop: 12,
    fontSize: 14,
    color: '#666',
  },
  errorContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 20,
  },
  errorTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#FF3B30',
    marginBottom: 8,
  },
  errorMessage: {
    fontSize: 14,
    color: '#666',
    textAlign: 'center',
  },
});

export default EngineView;
