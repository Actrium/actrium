/**
 * Component Registry
 *
 * Manages renderable component mappings, implementing whitelist security mechanism
 */

import type React from 'react';

/**
 * Component type - React component for rendering
 * Core still uses React for rendering via receiver/reconciler
 *
 * The props are deliberately unconstrained. Everything the registry hands a
 * component is a `Record<string, unknown>` decoded off the guest wire, so no
 * static prop signature can be honoured here -- and typing the slot as
 * `React.ComponentType<Record<string, unknown>>` does not express "props are
 * unknown", it demands "every prop is optional". That rules out any component
 * with a REQUIRED prop, including the library's own preset `Image` (`source`),
 * which made `register(DefaultComponents)` a type error for every consumer that
 * resolves real react-native types (issue #105). Whether the guest actually sent
 * a required prop is a runtime question the component itself answers; the
 * registry cannot answer it and should not pretend to.
 */
// biome-ignore lint/suspicious/noExplicitAny: wire boundary -- props arrive as Record<string,unknown> off the guest wire; see JSDoc above
export type ComponentType = React.ComponentType<any>;

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
      console.warn(`[keel] Component "${name}" is already registered, overwriting`);
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

      console.error(`[keel:ComponentRegistry] ❌ Component "${name}" not found. ${availableHint}`);

      // In debug mode, throw for immediate visibility
      if (this.debug) {
        throw new Error(
          `[keel] Component "${name}" not registered. Did you forget to register it? ${availableHint}`
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
    console.log('[keel:ComponentRegistry] === Diagnostics ===');
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
      (globalThis as { __KEEL_DEBUG__?: boolean }).__KEEL_DEBUG__);

  return new ComponentRegistry({ ...options, debug });
}
