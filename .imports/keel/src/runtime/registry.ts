/**
 * Component Registry
 *
 * Manages renderable component mappings, implementing whitelist security mechanism
 */

import type React from 'react';

/**
 * Component type - React component for rendering
 * Core still uses React for rendering via receiver/reconciler
 */
export type ComponentType = React.ComponentType<Record<string, unknown>>;

/**
 * Component map
 */
export type ComponentMap = Record<string, ComponentType>;

/**
 * Registry options
 */
export interface ComponentRegistryOptions {
  /**
   * Enable debug mode for better error visibility
   * When true, throws on missing components instead of returning undefined
   */
  debug?: boolean;
}

/**
 * Component registry
 */
export class ComponentRegistry {
  private components = new Map<string, ComponentType>();
  private debug: boolean;
  private accessLog: Map<string, number> = new Map(); // Track access attempts

  constructor(options: ComponentRegistryOptions = {}) {
    this.debug = options.debug ?? false;
  }

  /**
   * Enable or disable debug mode
   */
  setDebug(debug: boolean): void {
    this.debug = debug;
  }

  /**
   * Register a single component
   *
   * @param name Component name
   * @param component Component implementation
   */
  register(name: string, component: ComponentType): void {
    if (this.components.has(name)) {
      console.warn(`[rill] Component "${name}" is already registered, overwriting`);
    }
    this.components.set(name, component);
  }

  /**
   * Register multiple components
   *
   * @param map Component mapping object
   */
  registerAll(map: ComponentMap): void {
    Object.entries(map).forEach(([name, component]) => {
      this.register(name, component);
    });
  }

  /**
   * Get a component
   *
   * @param name Component name
   * @returns Component implementation or undefined
   */
  get(name: string): ComponentType | undefined {
    const component = this.components.get(name);

    if (!component) {
      // Track access attempts for diagnostics
      this.accessLog.set(name, (this.accessLog.get(name) ?? 0) + 1);

      // Log missing component with helpful diagnostics
      const registeredList = this.getRegisteredNames().slice(0, 10).join(', ');
      const moreCount = this.size - 10;
      const availableHint =
        this.size > 0
          ? `Available: [${registeredList}${moreCount > 0 ? `, ... +${moreCount} more` : ''}]`
          : 'No components registered!';

      console.error(
        `[rill:ComponentRegistry] ❌ Component "${name}" not found. ${availableHint}`
      );

      // In debug mode, throw for immediate visibility
      if (this.debug) {
        throw new Error(
          `[rill] Component "${name}" not registered. Did you forget to register it? ${availableHint}`
        );
      }
    }

    return component;
  }

  /**
   * Check if a component is registered
   *
   * @param name Component name
   */
  has(name: string): boolean {
    return this.components.has(name);
  }

  /**
   * Unregister a component
   *
   * @param name Component name
   */
  unregister(name: string): boolean {
    return this.components.delete(name);
  }

  /**
   * Get all registered component names
   */
  getRegisteredNames(): string[] {
    return Array.from(this.components.keys());
  }

  /**
   * Clear all registrations
   */
  clear(): void {
    this.components.clear();
    this.accessLog.clear();
  }

  /**
   * Get registered component count
   */
  get size(): number {
    return this.components.size;
  }

  /**
   * Get diagnostic info about missing component access attempts
   * Useful for debugging which components Guest code tried to use but weren't registered
   */
  getMissingAccessLog(): Map<string, number> {
    return new Map(this.accessLog);
  }

  /**
   * Print diagnostic summary to console
   */
  printDiagnostics(): void {
    console.log('[rill:ComponentRegistry] === Diagnostics ===');
    console.log(`Registered components (${this.size}):`, this.getRegisteredNames());

    if (this.accessLog.size > 0) {
      console.log('Missing component access attempts:');
      for (const [name, count] of this.accessLog) {
        console.log(`  - "${name}": accessed ${count} time(s)`);
      }
    } else {
      console.log('No missing component access attempts recorded.');
    }
  }
}

/**
 * Create a registry with optional configuration
 *
 * @param options Registry options
 */
export function createRegistry(options?: ComponentRegistryOptions): ComponentRegistry {
  // Auto-enable debug in development (check global flag)
  const debug =
    options?.debug ??
    (typeof globalThis !== 'undefined' &&
      (globalThis as { __RILL_DEBUG__?: boolean }).__RILL_DEBUG__);

  return new ComponentRegistry({ ...options, debug });
}
