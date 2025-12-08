/**
 * Host Application Example
 * Demonstrates how to integrate Rill into a React Native application
 */

import React, { useState, useEffect, useRef } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  SafeAreaView,
  Alert,
} from 'react-native';
import { Engine, EngineView } from 'rill';
import { createQuickJSProvider } from './QuickJSProvider';

export default function HostApp() {
  const [engine, setEngine] = useState<Engine | null>(null);
  const [isLoaded, setIsLoaded] = useState(false);
  const [theme, setTheme] = useState<'light' | 'dark'>('light');
  const [errorCount, setErrorCount] = useState(0);

  // Initialize Engine
  useEffect(() => {
    const engineInstance = new Engine({
      quickjs: createQuickJSProvider(),
      timeout: 5000,
      debug: true,
      logger: {
        log: console.log,
        warn: console.warn,
        error: console.error,
      },
      onMetric: (name, value) => {
        console.log(`[Metric] ${name}: ${value}ms`);
      },
    });

    // Listen for plugin messages
    engineInstance.on('message', (message) => {
      console.log('[Host] Received plugin message:', message.event, message.payload);
      Alert.alert('Plugin Message', `${message.event}: ${JSON.stringify(message.payload)}`);
    });

    // Listen for errors
    engineInstance.on('error', (error) => {
      console.error('[Host] Plugin error:', error);
      const health = engineInstance.getHealth();
      setErrorCount(health.errorCount);
    });

    setEngine(engineInstance);

    // Cleanup
    return () => {
      engineInstance.destroy();
    };
  }, []);

  // Send event to plugin
  const handleSendMessage = () => {
    if (engine) {
      engine.sendEvent('HOST_MESSAGE', {
        message: 'Hello from host!',
        timestamp: Date.now(),
      });
    }
  };

  // Toggle theme
  const handleToggleTheme = () => {
    const newTheme = theme === 'light' ? 'dark' : 'light';
    setTheme(newTheme);
    if (engine) {
      engine.sendEvent('THEME_CHANGE', { theme: newTheme });
    }
  };

  // Check health status
  const handleCheckHealth = () => {
    if (engine) {
      const health = engine.getHealth();
      Alert.alert(
        'Engine Health Status',
        JSON.stringify(health, null, 2)
      );
    }
  };

  if (!engine) {
    return (
      <SafeAreaView style={styles.container}>
        <Text>Initializing Engine...</Text>
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={styles.container}>
      {/* Control Panel */}
      <View style={styles.controlPanel}>
        <Text style={styles.title}>Host Control Panel</Text>

        <View style={styles.statusRow}>
          <Text style={styles.statusLabel}>Plugin Status:</Text>
          <Text style={isLoaded ? styles.statusLoaded : styles.statusLoading}>
            {isLoaded ? 'Loaded' : 'Loading...'}
          </Text>
        </View>

        {errorCount > 0 && (
          <View style={styles.errorBanner}>
            <Text style={styles.errorText}>Error Count: {errorCount}</Text>
          </View>
        )}

        <View style={styles.buttonGroup}>
          <TouchableOpacity
            style={styles.button}
            onPress={handleSendMessage}
            disabled={!isLoaded}
          >
            <Text style={styles.buttonText}>Send Message to Plugin</Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={styles.button}
            onPress={handleToggleTheme}
            disabled={!isLoaded}
          >
            <Text style={styles.buttonText}>Toggle Theme ({theme})</Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[styles.button, styles.buttonSecondary]}
            onPress={handleCheckHealth}
          >
            <Text style={styles.buttonText}>Check Health Status</Text>
          </TouchableOpacity>
        </View>
      </View>

      {/* Plugin Render Area */}
      <View style={styles.pluginContainer}>
        <EngineView
          engine={engine}
          bundleUrl="./dist/plugin.js"  // Locally bundled plugin
          initialProps={{
            userId: 'user-123',
            theme,
          }}
          onLoad={() => {
            console.log('[Host] Plugin loaded successfully');
            setIsLoaded(true);
          }}
          onError={(error) => {
            console.error('[Host] Plugin error:', error);
            Alert.alert('Plugin Error', error.message);
          }}
          onDestroy={() => {
            console.log('[Host] Plugin destroyed');
            setIsLoaded(false);
          }}
          fallback={
            <View style={styles.loadingContainer}>
              <Text style={styles.loadingText}>Loading plugin...</Text>
            </View>
          }
          renderError={(error) => (
            <View style={styles.errorContainer}>
              <Text style={styles.errorTitle}>Plugin Load Failed</Text>
              <Text style={styles.errorMessage}>{error.message}</Text>
              <TouchableOpacity
                style={styles.retryButton}
                onPress={() => {
                  // Reload
                  setEngine(null);
                  setTimeout(() => {
                    const newEngine = new Engine({
                      quickjs: createQuickJSProvider(),
                      timeout: 5000,
                      debug: true,
                    });
                    setEngine(newEngine);
                  }, 100);
                }}
              >
                <Text style={styles.retryButtonText}>Retry</Text>
              </TouchableOpacity>
            </View>
          )}
        />
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#F2F2F7',
  },
  controlPanel: {
    backgroundColor: 'white',
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#E5E5EA',
  },
  title: {
    fontSize: 20,
    fontWeight: 'bold',
    marginBottom: 12,
  },
  statusRow: {
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 12,
  },
  statusLabel: {
    fontSize: 14,
    marginRight: 8,
  },
  statusLoaded: {
    fontSize: 14,
    color: '#34C759',
    fontWeight: '600',
  },
  statusLoading: {
    fontSize: 14,
    color: '#FF9500',
    fontWeight: '600',
  },
  errorBanner: {
    backgroundColor: '#FFEBEE',
    padding: 8,
    borderRadius: 4,
    marginBottom: 12,
  },
  errorText: {
    color: '#D32F2F',
    fontSize: 14,
  },
  buttonGroup: {
    gap: 8,
  },
  button: {
    backgroundColor: '#007AFF',
    paddingVertical: 12,
    paddingHorizontal: 16,
    borderRadius: 8,
    alignItems: 'center',
  },
  buttonSecondary: {
    backgroundColor: '#8E8E93',
  },
  buttonText: {
    color: 'white',
    fontSize: 14,
    fontWeight: '600',
  },
  pluginContainer: {
    flex: 1,
  },
  loadingContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  loadingText: {
    fontSize: 16,
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
    fontWeight: 'bold',
    color: '#FF3B30',
    marginBottom: 8,
  },
  errorMessage: {
    fontSize: 14,
    color: '#666',
    textAlign: 'center',
    marginBottom: 20,
  },
  retryButton: {
    backgroundColor: '#007AFF',
    paddingVertical: 10,
    paddingHorizontal: 20,
    borderRadius: 8,
  },
  retryButtonText: {
    color: 'white',
    fontSize: 14,
    fontWeight: '600',
  },
});
