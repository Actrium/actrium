/**
 * Host-side Advanced Features Demo
 */

import React, { useEffect, useState } from 'react';
import { View, Text, StyleSheet } from 'react-native';
import { Engine, EngineView } from 'rill';

// Performance metrics collector
class MetricsCollector {
  private metrics: Array<{ name: string; value: number; timestamp: number }> = [];
  private readonly maxSize = 100;

  collect(name: string, value: number) {
    this.metrics.push({
      name,
      value,
      timestamp: Date.now(),
    });

    // Maintain window size
    if (this.metrics.length > this.maxSize) {
      this.metrics.shift();
    }

    // Performance alert
    this.checkPerformance(name, value);
  }

  private checkPerformance(name: string, value: number) {
    const thresholds: Record<string, number> = {
      'receiver.render': 16, // 60fps threshold
      'engine.sendToSandbox': 10,
      'receiver.applyOperations': 20,
    };

    const threshold = thresholds[name];
    if (threshold && value > threshold) {
      console.warn(`⚠️ Performance warning: ${name} took ${value}ms (threshold: ${threshold}ms)`);
    }
  }

  getStats(metricName: string) {
    const values = this.metrics
      .filter((m) => m.name === metricName)
      .map((m) => m.value);

    if (values.length === 0) return null;

    const sorted = [...values].sort((a, b) => a - b);
    const sum = values.reduce((a, b) => a + b, 0);

    return {
      count: values.length,
      avg: Math.round(sum / values.length * 100) / 100,
      min: Math.min(...values),
      max: Math.max(...values),
      p50: sorted[Math.floor(sorted.length * 0.5)],
      p95: sorted[Math.floor(sorted.length * 0.95)],
      p99: sorted[Math.floor(sorted.length * 0.99)],
    };
  }

  getAllMetrics() {
    const names = Array.from(new Set(this.metrics.map((m) => m.name)));
    return names.map((name) => ({
      name,
      stats: this.getStats(name),
    }));
  }
}

export default function HostDemo() {
  const [engine, setEngine] = useState<Engine | null>(null);
  const [metricsCollector] = useState(() => new MetricsCollector());
  const [stats, setStats] = useState<any[]>([]);

  useEffect(() => {
    // Create Engine instance
    const engineInstance = new Engine({
      provider: createQuickJSProvider(), // Need to provide actual provider
      timeout: 5000, // 5 second timeout
      debug: true,

      // Custom logger
      logger: {
        log: (...args) => console.log('[Rill]', ...args),
        warn: (...args) => console.warn('[Rill]', ...args),
        error: (...args) => console.error('[Rill]', ...args),
      },

      // Performance metrics callback
      onMetric: (name, value, extra) => {
        console.log(`[Metric] ${name}: ${value}ms`, extra);
        metricsCollector.collect(name, value);
      },

      // Module whitelist
      requireWhitelist: [
        'lodash',
        'date-fns',
      ],

      // Batch size limit
      receiverMaxBatchSize: 5000,
    });

    // Listen for errors
    engineInstance.on('error', (error) => {
      console.error('[Engine Error]', error);

      // Check health status
      const health = engineInstance.getHealth();
      console.log('[Engine Health]', health);

      // Consider restarting if too many errors
      if (health.errorCount > 10) {
        console.error('Too many errors detected, consider restarting');
      }
    });

    setEngine(engineInstance);

    // Periodically update statistics
    const interval = setInterval(() => {
      setStats(metricsCollector.getAllMetrics());
    }, 2000);

    return () => {
      clearInterval(interval);
      engineInstance.destroy();
    };
  }, []);

  if (!engine) {
    return (
      <View style={styles.container}>
        <Text>Initializing...</Text>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      {/* Performance Statistics Panel */}
      <View style={styles.statsPanel}>
        <Text style={styles.statsTitle}>Performance Statistics (Real-time)</Text>
        {stats.map((item) => (
          <View key={item.name} style={styles.statRow}>
            <Text style={styles.statName}>{item.name}</Text>
            {item.stats && (
              <Text style={styles.statValue}>
                avg: {item.stats.avg}ms | p95: {item.stats.p95}ms | count:{' '}
                {item.stats.count}
              </Text>
            )}
          </View>
        ))}
      </View>

      {/* Guest Render Area */}
      <EngineView
        engine={engine}
        bundleUrl="./dist/guest.js"
        onLoad={() => console.log('[Host] Guest loaded')}
        onError={(err) => console.error('[Host] Guest error:', err)}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  statsPanel: {
    backgroundColor: '#1C1C1E',
    padding: 12,
    borderBottomWidth: 1,
    borderBottomColor: '#38383A',
  },
  statsTitle: {
    color: '#FFFFFF',
    fontSize: 14,
    fontWeight: '600',
    marginBottom: 8,
  },
  statRow: {
    marginBottom: 4,
  },
  statName: {
    color: '#8E8E93',
    fontSize: 12,
  },
  statValue: {
    color: '#FFFFFF',
    fontSize: 11,
    fontFamily: 'monospace',
  },
});

// Mock provider - In actual usage, need to provide real QuickJS provider
function createQuickJSProvider() {
  return {
    createRuntime() {
      const globals = new Map();
      return {
        createContext() {
          return {
            eval(code: string) {
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              const fn = new Function(...globalNames, `"use strict"; ${code}`);
              return fn(...globalValues);
            },
            setGlobal(name: string, value: any) {
              globals.set(name, value);
            },
            getGlobal(name: string) {
              return globals.get(name);
            },
            dispose() {
              globals.clear();
            },
          };
        },
        dispose() {},
      };
    },
  };
}
