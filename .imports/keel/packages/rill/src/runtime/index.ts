/**
 * Rill Runtime
 *
 * Host-side runtime, responsible for sandbox management, operation receiving, and UI rendering
 */

// Core exports
// Development-time version check to help align React and react-reconciler peer versions
(function () {
  try {
    const env = (typeof process !== 'undefined' && process && process.env && process.env.NODE_ENV) || 'production';
    if (env !== 'production') {
      // Dynamically resolve versions without creating hard dependencies
      let reactVersion: string | undefined;
      let reconcilerVersion: string | undefined;
      try {
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        reactVersion = require('react/package.json')?.version;
      } catch {
        // Ignore - optional version check
      }
      try {
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        reconcilerVersion = require('react-reconciler/package.json')?.version;
      } catch {
        // Ignore - optional version check
      }

      if (reactVersion && reconcilerVersion) {
        // Basic guidance mapping; keep in sync with README compatibility table
        const guidance: Array<{ r: RegExp; allowed: RegExp; recommend: string }> = [
          { r: /^18\./, allowed: /^(0\.(29|30|31))\./, recommend: 'react-reconciler ^0.31' },
          { r: /^19\.0\./, allowed: /^0\.32\./, recommend: 'react-reconciler ^0.32' },
          { r: /^19\.(2|3|4|5)\./, allowed: /^0\.33\./, recommend: 'react-reconciler ^0.33' },
        ];
        const match = guidance.find(g => g.r.test(reactVersion!));
        if (match && !match.allowed.test(reconcilerVersion)) {
          // eslint-disable-next-line no-console
          console.warn(
            `[rill] Detected React ${reactVersion} with react-reconciler ${reconcilerVersion}. ` +
              `Recommended pairing: ${match.recommend}. ` +
              `Refer to README Compatibility for details.`
          );
        }
      }
    }
  } catch {
    // Swallow any errors – this is a best-effort dev hint only
  }
})();

// Core exports
export { Engine } from './engine';
export type {
  EngineOptions,
  EngineEvents,
  PluginMessage,
  QuickJSContext,
  QuickJSRuntime,
  QuickJSProvider,
} from './engine';

// QuickJSProvider 接口由 rill 提供，具体实现由宿主应用提供
// React Native 宿主可使用 react-native-quickjs 原生模块
// Node.js/Web 宿主可使用 quickjs-emscripten

export { EngineView } from './EngineView';
export type { EngineViewProps } from './EngineView';

export { Receiver } from './receiver';
export type { SendToSandbox } from './receiver';

export { ComponentRegistry, createRegistry } from './registry';
export type { ComponentType, ComponentMap } from './registry';

// Performance optimization
export {
  OperationMerger,
  ThrottledScheduler,
  VirtualScrollCalculator,
  ScrollThrottler,
  PerformanceMonitor,
} from './performance';
export type {
  BatchConfig,
  VirtualScrollConfig,
  VirtualScrollState,
  PerformanceMetrics,
} from './performance';

// Type exports
export type {
  Operation,
  OperationBatch,
  OperationType,
  CreateOperation,
  UpdateOperation,
  DeleteOperation,
  AppendOperation,
  InsertOperation,
  RemoveOperation,
  ReorderOperation,
  TextOperation,
  HostMessage,
  HostMessageType,
  CallFunctionMessage,
  HostEventMessage,
  ConfigUpdateMessage,
  DestroyMessage,
  SerializedProps,
  SerializedValue,
  SerializedFunction,
  NodeInstance,
  StyleObject,
  StyleProp,
} from '../types';

// Default components (need to be implemented in components/ directory)
// export { DefaultComponents } from '../components';
