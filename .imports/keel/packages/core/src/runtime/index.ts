/**
 * Rill Runtime
 *
 * Host-side runtime, responsible for sandbox management, operation receiving, and UI rendering
 */

// Core exports
// Development-time version check to help align React and react-reconciler peer versions
(() => {
  try {
    const maybeProcess = (globalThis as unknown as { process?: { env?: { NODE_ENV?: string } } })
      .process;
    const env = maybeProcess?.env?.NODE_ENV ?? 'production';
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
          // React 19.0.x currently aligns with react-reconciler 0.31.x peer constraints
          { r: /^19\.0\./, allowed: /^0\.31\./, recommend: 'react-reconciler ^0.31' },
          { r: /^19\.1\./, allowed: /^0\.32\./, recommend: 'react-reconciler ^0.32' },
          { r: /^19\.(2|3|4|5)\./, allowed: /^0\.33\./, recommend: 'react-reconciler ^0.33' },
        ];
        const match = guidance.find((g) => g.r.test(reactVersion!));
        if (match && !match.allowed.test(reconcilerVersion)) {
          const isReactNative =
            typeof (globalThis as unknown as { navigator?: { product?: string } }).navigator !==
              'undefined' &&
            (globalThis as unknown as { navigator?: { product?: string } }).navigator?.product ===
              'ReactNative';
          const logFn = isReactNative ? console.log : console.warn;
          // On React Native, console.warn triggers LogBox; macOS RN LogBox may crash on some versions (NULL CGColor).
          // eslint-disable-next-line no-console
          logFn(
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

// Backward compatibility type aliases
export type {
  EngineActivityStats,
  EngineDiagnostics,
  EngineEvents,
  EngineHealth,
  EngineOptions,
  GuestMessage,
  JSEngineContext,
  JSEngineContext as QuickJSContext,
  JSEngineProvider,
  JSEngineProvider as QuickJSProvider,
  JSEngineRuntime,
  JSEngineRuntime as QuickJSRuntime,
} from './engine';
// Core exports
export { Engine } from './engine';

// JSEngineProvider interface is provided by rill, with specific implementations supplied by the host app
// React Native hosts can use react-native-quickjs native module
// Node.js/Web hosts can use quickjs-emscripten or vm-based providers

export type { DefaultComponentName } from '../components';
// Default components
export { DefaultComponents } from '../components';
// Type exports
export type {
  AppendOperation,
  CallFunctionMessage,
  ConfigUpdateMessage,
  CreateOperation,
  DeleteOperation,
  DestroyMessage,
  HostEventMessage,
  HostMessage,
  HostMessageType,
  InsertOperation,
  NodeInstance,
  Operation,
  OperationBatch,
  OperationType,
  RemoveOperation,
  ReorderOperation,
  SerializedFunction,
  SerializedProps,
  SerializedValue,
  StyleObject,
  StyleProp,
  TextOperation,
  UpdateOperation,
} from '../types';
export type { EngineViewProps } from './EngineView';
export { EngineView } from './EngineView';
export type {
  BatchConfig,
  PerformanceMetrics,
  VirtualScrollConfig,
  VirtualScrollState,
} from './performance';

// Performance optimization
export {
  OperationMerger,
  PerformanceMonitor,
  ScrollThrottler,
  ThrottledScheduler,
  VirtualScrollCalculator,
} from './performance';
export type { SendToSandbox } from './receiver';
export { Receiver } from './receiver';
export type { ComponentMap, ComponentType } from './registry';
export { ComponentRegistry, createRegistry } from './registry';
