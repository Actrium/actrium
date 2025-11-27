/**
 * AskIt Core - Host Only Module
 *
 * This module is only available in Host App environment.
 * It provides the registry and bridge for connecting askit to Rill engine.
 *
 * Usage:
 * ```typescript
 * import { Engine } from 'rill';
 * import { AskitRegistry, createEngineAdapter } from 'askit/core';
 *
 * const engine = new Engine();
 * engine.register(AskitRegistry.components);
 * const adapter = createEngineAdapter(engine);
 * ```
 */

// Registry exports
export {
  AskitRegistry,
  AskitComponents,
  AskitModules,
  configureToast,
  configureHaptic,
  components,
  modules,
} from './registry';

// Bridge exports
export { Bridge, handleGuestMessage, createEngineAdapter } from './bridge';

// Type exports
export type { AskitRegistryConfig, ComponentMap, ModuleMap } from '../types';
